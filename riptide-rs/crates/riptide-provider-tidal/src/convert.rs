//! TIDAL model conversion into provider-neutral core types.

use crate::{
    model::{
        TidalAlbum, TidalArtist, TidalArtistRole, TidalLyrics, TidalMediaTag, TidalPlaylist,
        TidalTopHitKind, TidalTrack, TidalVideo,
    },
    CountryCode, TidalError, TidalResult, TIDAL_PROVIDER_ID,
};
use riptide_core::{
    Album, AlbumSummary, Artist, ArtistCredit, ArtistRole, AudioQuality, CatalogEntity, CoverArtId,
    ExternalIdKind, ExternalIds, ItemCount, Lyrics, LyricsSource, LyricsSourceName, MediaDuration,
    MediaRef, Playlist, ProviderQualityLabel, ResourceId, ResourceKind, Track, TrackNumber, Video,
    VideoQuality,
};
use std::collections::BTreeMap;

fn media_ref(kind: ResourceKind, id: &impl ToString) -> TidalResult<MediaRef> {
    Ok(MediaRef::new(
        TIDAL_PROVIDER_ID.clone(),
        kind,
        ResourceId::new(id.to_string()).map_err(|error| TidalError::InvalidResponse {
            field: "id".to_owned(),
            message: error.to_string(),
        })?,
    ))
}

/// Converts a TIDAL track.
pub fn track(track: TidalTrack) -> TidalResult<Track> {
    let mut external = BTreeMap::new();
    if let Some(isrc) = track.isrc.clone() {
        external.insert(ExternalIdKind::Isrc, isrc);
    }
    Ok(Track {
        id: media_ref(ResourceKind::Track, &track.id)?,
        title: track.title,
        artists: artists(track.artist, track.artists)?,
        album: track.album.map(album_summary).transpose()?.flatten(),
        duration: track.duration.map(MediaDuration::from_secs),
        track_number: track
            .track_number
            .map(TrackNumber::new)
            .transpose()
            .map_err(|error| TidalError::InvalidResponse {
                field: "trackNumber".to_owned(),
                message: error.to_string(),
            })?,
        disc_number: track
            .volume_number
            .map(riptide_core::DiscNumber::new)
            .transpose()
            .map_err(|error| TidalError::InvalidResponse {
                field: "volumeNumber".to_owned(),
                message: error.to_string(),
            })?,
        explicit: track.explicit,
        external_ids: ExternalIds(external),
        version: track.version,
        copyright: track.copyright,
        bpm: track.bpm,
        quality_label: track.audio_quality,
        dolby_atmos: track
            .media_metadata
            .tags
            .contains(&TidalMediaTag::DolbyAtmos),
    })
}

/// Converts a TIDAL video.
pub fn video(video: TidalVideo) -> TidalResult<Video> {
    Ok(Video {
        id: media_ref(ResourceKind::Video, &video.id)?,
        title: video.title,
        artists: artists(video.artist, video.artists)?,
        album: video.album.map(album_summary).transpose()?.flatten(),
        duration: video.duration.map(MediaDuration::from_secs),
        track_number: video
            .track_number
            .map(TrackNumber::new)
            .transpose()
            .map_err(|error| TidalError::InvalidResponse {
                field: "trackNumber".to_owned(),
                message: error.to_string(),
            })?,
        disc_number: video
            .volume_number
            .map(riptide_core::DiscNumber::new)
            .transpose()
            .map_err(|error| TidalError::InvalidResponse {
                field: "volumeNumber".to_owned(),
                message: error.to_string(),
            })?,
        explicit: video.explicit,
        quality_label: video.quality,
    })
}

/// Converts a TIDAL album.
pub fn album(album: TidalAlbum) -> TidalResult<Album> {
    Ok(Album {
        id: media_ref(ResourceKind::Album, &album.id)?,
        title: album.title,
        artists: artists(album.artist, album.artists)?,
        release_date: album.release_date.as_deref().map(parse_date).transpose()?,
        track_count: album
            .number_of_tracks
            .map(ItemCount::new)
            .transpose()
            .map_err(|error| TidalError::InvalidResponse {
                field: "numberOfTracks".to_owned(),
                message: error.to_string(),
            })?,
        video_count: album
            .number_of_videos
            .map(ItemCount::new)
            .transpose()
            .map_err(|error| TidalError::InvalidResponse {
                field: "numberOfVideos".to_owned(),
                message: error.to_string(),
            })?,
        explicit: album.explicit,
        release_type: album.release_type,
        hires_lossless: album
            .media_metadata
            .tags
            .contains(&TidalMediaTag::HiResLossless),
        cover: album.cover.map(cover_art_id).transpose()?,
    })
}

fn album_summary(album: crate::model::TidalAlbumSummary) -> TidalResult<Option<AlbumSummary>> {
    let Some((id, title)) = album.id.zip(album.title) else {
        return Ok(None);
    };
    Ok(Some(AlbumSummary {
        id: media_ref(ResourceKind::Album, &id)?,
        title,
        artists: Vec::new(),
        cover: album.cover.map(cover_art_id).transpose()?,
    }))
}

fn cover_art_id(value: String) -> TidalResult<CoverArtId> {
    CoverArtId::new(value).map_err(|error| TidalError::InvalidResponse {
        field: "cover".to_owned(),
        message: error.to_string(),
    })
}

/// Converts a TIDAL playlist.
pub fn playlist(playlist: TidalPlaylist) -> TidalResult<Playlist> {
    let item_count = playlist
        .number_of_tracks
        .unwrap_or(0)
        .saturating_add(playlist.number_of_videos.unwrap_or(0));
    Ok(Playlist {
        id: media_ref(ResourceKind::Playlist, &playlist.uuid)?,
        title: playlist.title,
        owner: None,
        item_count: if item_count == 0 {
            None
        } else {
            Some(
                ItemCount::new(item_count).map_err(|error| TidalError::InvalidResponse {
                    field: "item_count".to_owned(),
                    message: error.to_string(),
                })?,
            )
        },
        created_at: playlist
            .created
            .as_deref()
            .map(parse_datetime)
            .transpose()?,
        updated_at: playlist
            .last_updated
            .as_deref()
            .map(parse_datetime)
            .transpose()?,
    })
}

/// Converts TIDAL lyrics.
pub fn lyrics(lyrics: TidalLyrics) -> TidalResult<Lyrics> {
    Ok(Lyrics {
        text: lyrics.lyrics,
        synchronized_lrc: lyrics.subtitles,
        source: lyrics
            .lyrics_provider
            .map(LyricsSourceName::new)
            .transpose()
            .map_err(|error| TidalError::InvalidResponse {
                field: "lyricsProvider".to_owned(),
                message: error.to_string(),
            })?
            .map(LyricsSource::External),
    })
}

/// Maps an audio quality label.
#[allow(dead_code)]
pub fn audio_quality(label: &str) -> TidalResult<AudioQuality> {
    Ok(match label {
        "LOW" => AudioQuality::Low,
        "HIGH" => AudioQuality::High,
        "LOSSLESS" => AudioQuality::Lossless,
        "HI_RES" | "HIRES_LOSSLESS" | "HI_RES_LOSSLESS" => AudioQuality::HiResLossless,
        other => {
            AudioQuality::ProviderSpecific(ProviderQualityLabel::new(other).map_err(|error| {
                TidalError::InvalidResponse {
                    field: "quality".to_owned(),
                    message: error.to_string(),
                }
            })?)
        }
    })
}

/// Maps a video quality label.
#[allow(dead_code)]
pub fn video_quality(label: &str) -> TidalResult<VideoQuality> {
    Ok(match label {
        "LOW" => VideoQuality::Low,
        "MEDIUM" => VideoQuality::Medium,
        "HIGH" => VideoQuality::High,
        other => {
            VideoQuality::ProviderSpecific(ProviderQualityLabel::new(other).map_err(|error| {
                TidalError::InvalidResponse {
                    field: "videoQuality".to_owned(),
                    message: error.to_string(),
                }
            })?)
        }
    })
}

/// Maps a session country code.
#[allow(dead_code)]
pub fn country_code(value: String) -> TidalResult<CountryCode> {
    CountryCode::new(value)
}

fn artists(
    primary: Option<TidalArtist>,
    mut artists: Vec<TidalArtist>,
) -> TidalResult<Vec<ArtistCredit>> {
    if artists.is_empty() {
        if let Some(primary) = primary {
            artists.push(primary);
        }
    }
    artists
        .into_iter()
        .map(|artist| {
            Ok(ArtistCredit {
                artist: Artist {
                    id: artist
                        .id
                        .map(|id| media_ref(ResourceKind::Artist, &id))
                        .transpose()?,
                    name: artist.name,
                },
                role: match artist.role {
                    Some(TidalArtistRole::Main) => ArtistRole::Main,
                    Some(TidalArtistRole::Featured) => ArtistRole::Featured,
                    Some(TidalArtistRole::Other) => ArtistRole::Contributor,
                    None => ArtistRole::Unknown,
                },
            })
        })
        .collect()
}

fn parse_date(value: &str) -> TidalResult<time::Date> {
    time::Date::parse(value, &time::format_description::well_known::Iso8601::DATE).map_err(
        |error| TidalError::InvalidResponse {
            field: "date".to_owned(),
            message: error.to_string(),
        },
    )
}

fn parse_datetime(value: &str) -> TidalResult<time::OffsetDateTime> {
    time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339).map_err(
        |error| TidalError::InvalidResponse {
            field: "datetime".to_owned(),
            message: error.to_string(),
        },
    )
}

/// Converts a top-hit raw value.
pub fn top_hit(
    kind: &TidalTopHitKind,
    value: serde_json::Value,
) -> TidalResult<Option<CatalogEntity>> {
    match kind {
        TidalTopHitKind::Tracks => Ok(Some(CatalogEntity::Track(track(
            serde_json::from_value(value).map_err(|error| TidalError::InvalidResponse {
                field: "topHit.track".to_owned(),
                message: error.to_string(),
            })?,
        )?))),
        TidalTopHitKind::Albums => Ok(Some(CatalogEntity::Album(album(
            serde_json::from_value(value).map_err(|error| TidalError::InvalidResponse {
                field: "topHit.album".to_owned(),
                message: error.to_string(),
            })?,
        )?))),
        TidalTopHitKind::Playlists => Ok(Some(CatalogEntity::Playlist(playlist(
            serde_json::from_value(value).map_err(|error| TidalError::InvalidResponse {
                field: "topHit.playlist".to_owned(),
                message: error.to_string(),
            })?,
        )?))),
        TidalTopHitKind::Artists | TidalTopHitKind::Other => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{MediaMetadata, TidalAlbumSummary};

    fn minimal_track() -> TidalTrack {
        TidalTrack {
            id: 1,
            title: "Test Track".to_owned(),
            duration: Some(300),
            track_number: Some(3),
            volume_number: Some(1),
            version: Some("Radio Edit".to_owned()),
            copyright: Some("2024 Label".to_owned()),
            bpm: Some(120),
            isrc: Some("USRC12345678".to_owned()),
            explicit: Some(true),
            audio_quality: Some("LOSSLESS".to_owned()),
            artist: Some(TidalArtist {
                id: Some(10),
                name: "Main Artist".to_owned(),
                role: Some(TidalArtistRole::Main),
            }),
            artists: vec![
                TidalArtist {
                    id: Some(10),
                    name: "Main Artist".to_owned(),
                    role: Some(TidalArtistRole::Main),
                },
                TidalArtist {
                    id: Some(20),
                    name: "Featured".to_owned(),
                    role: Some(TidalArtistRole::Featured),
                },
            ],
            album: Some(TidalAlbumSummary {
                id: Some(100),
                title: Some("Test Album".to_owned()),
                cover: Some("abcdef12-3456-7890-abcd-ef1234567890".to_owned()),
            }),
            media_metadata: MediaMetadata {
                tags: vec![TidalMediaTag::DolbyAtmos],
            },
        }
    }

    #[test]
    fn track_converts_all_fields() -> TidalResult<()> {
        let result = track(minimal_track())?;
        assert_eq!(result.title, "Test Track");
        assert_eq!(result.id.id.as_str(), "1");
        assert_eq!(result.id.kind, ResourceKind::Track);
        assert_eq!(
            result.duration.map(|d| d.as_duration().as_secs()),
            Some(300)
        );
        assert_eq!(result.track_number.map(TrackNumber::get), Some(3));
        assert_eq!(
            result.disc_number.map(riptide_core::DiscNumber::get),
            Some(1)
        );
        assert_eq!(result.version.as_deref(), Some("Radio Edit"));
        assert_eq!(result.copyright.as_deref(), Some("2024 Label"));
        assert_eq!(result.bpm, Some(120));
        assert_eq!(result.quality_label.as_deref(), Some("LOSSLESS"));
        assert!(result.dolby_atmos);
        assert_eq!(result.explicit, Some(true));
        assert_eq!(
            result.external_ids.0.get(&ExternalIdKind::Isrc),
            Some(&"USRC12345678".to_owned())
        );
        assert!(result.album.is_some());
        assert_eq!(result.artists.len(), 2);
        assert_eq!(result.artists[0].role, ArtistRole::Main);
        assert_eq!(result.artists[1].role, ArtistRole::Featured);
        Ok(())
    }

    #[test]
    fn track_with_missing_optional_fields() -> TidalResult<()> {
        let tidal_track = TidalTrack {
            id: 2,
            title: "Minimal".to_owned(),
            duration: None,
            track_number: None,
            volume_number: None,
            version: None,
            copyright: None,
            bpm: None,
            isrc: None,
            explicit: None,
            audio_quality: None,
            artist: None,
            artists: Vec::new(),
            album: None,
            media_metadata: MediaMetadata::default(),
        };
        let result = track(tidal_track)?;
        assert_eq!(result.title, "Minimal");
        assert!(result.duration.is_none());
        assert!(result.track_number.is_none());
        assert!(result.version.is_none());
        assert!(result.bpm.is_none());
        assert!(!result.dolby_atmos);
        assert!(result.album.is_none());
        assert!(result.artists.is_empty());
        assert!(result.external_ids.0.is_empty());
        Ok(())
    }

    #[test]
    fn artist_role_mapping() -> TidalResult<()> {
        let result = track(minimal_track())?;
        assert_eq!(result.artists[0].role, ArtistRole::Main);
        assert_eq!(result.artists[1].role, ArtistRole::Featured);

        // Test unknown role
        let tidal_track = TidalTrack {
            artists: vec![TidalArtist {
                id: Some(1),
                name: "Other".to_owned(),
                role: Some(TidalArtistRole::Other),
            }],
            ..minimal_track()
        };
        let result = track(tidal_track)?;
        assert_eq!(result.artists[0].role, ArtistRole::Contributor);

        // Test missing role
        let tidal_track = TidalTrack {
            artists: vec![TidalArtist {
                id: Some(1),
                name: "NoRole".to_owned(),
                role: None,
            }],
            ..minimal_track()
        };
        let result = track(tidal_track)?;
        assert_eq!(result.artists[0].role, ArtistRole::Unknown);
        Ok(())
    }

    #[test]
    fn primary_artist_fallback_when_artists_empty() -> TidalResult<()> {
        let tidal_track = TidalTrack {
            artists: Vec::new(),
            artist: Some(TidalArtist {
                id: Some(10),
                name: "Solo".to_owned(),
                role: Some(TidalArtistRole::Main),
            }),
            ..minimal_track()
        };
        let result = track(tidal_track)?;
        assert_eq!(result.artists.len(), 1);
        assert_eq!(result.artists[0].artist.name, "Solo");
        Ok(())
    }

    #[test]
    fn album_converts_with_hires_tag() -> TidalResult<()> {
        let tidal_album = TidalAlbum {
            id: 1,
            title: "HiRes Album".to_owned(),
            release_date: Some("2024-01-15".to_owned()),
            number_of_tracks: Some(12),
            number_of_videos: Some(2),
            explicit: Some(false),
            cover: Some("abcdef12-3456-7890-abcd-ef1234567890".to_owned()),
            release_type: Some("ALBUM".to_owned()),
            artist: None,
            artists: Vec::new(),
            media_metadata: MediaMetadata {
                tags: vec![TidalMediaTag::HiResLossless],
            },
        };
        let result = album(tidal_album)?;
        assert_eq!(result.title, "HiRes Album");
        assert!(result.hires_lossless);
        assert_eq!(result.release_type.as_deref(), Some("ALBUM"));
        assert_eq!(result.track_count.map(ItemCount::get), Some(12));
        assert_eq!(result.video_count.map(ItemCount::get), Some(2));
        assert!(result.release_date.is_some());
        Ok(())
    }

    #[test]
    fn playlist_converts_item_count() -> TidalResult<()> {
        let tidal_playlist = TidalPlaylist {
            uuid: "abc-123".to_owned(),
            title: "My Playlist".to_owned(),
            number_of_tracks: Some(10),
            number_of_videos: Some(5),
            created: Some("2024-01-15T12:00:00Z".to_owned()),
            last_updated: None,
        };
        let result = playlist(tidal_playlist)?;
        assert_eq!(result.title, "My Playlist");
        assert_eq!(result.item_count.map(ItemCount::get), Some(15));
        assert!(result.created_at.is_some());
        assert!(result.updated_at.is_none());
        Ok(())
    }

    #[test]
    fn lyrics_converts_with_provider() -> TidalResult<()> {
        let tidal_lyrics = TidalLyrics {
            lyrics: Some("Hello world".to_owned()),
            subtitles: Some("[00:01] Hello".to_owned()),
            lyrics_provider: Some("Musixmatch".to_owned()),
        };
        let result = lyrics(tidal_lyrics)?;
        assert_eq!(result.text.as_deref(), Some("Hello world"));
        assert_eq!(result.synchronized_lrc.as_deref(), Some("[00:01] Hello"));
        assert!(matches!(result.source, Some(LyricsSource::External(_))));
        Ok(())
    }

    #[test]
    fn audio_quality_maps_known_labels() -> TidalResult<()> {
        assert_eq!(audio_quality("LOW")?, AudioQuality::Low);
        assert_eq!(audio_quality("HIGH")?, AudioQuality::High);
        assert_eq!(audio_quality("LOSSLESS")?, AudioQuality::Lossless);
        assert_eq!(audio_quality("HI_RES")?, AudioQuality::HiResLossless);
        assert_eq!(
            audio_quality("HI_RES_LOSSLESS")?,
            AudioQuality::HiResLossless
        );
        assert!(matches!(
            audio_quality("UNKNOWN")?,
            AudioQuality::ProviderSpecific(_)
        ));
        Ok(())
    }

    #[test]
    fn video_quality_maps_known_labels() -> TidalResult<()> {
        assert_eq!(video_quality("LOW")?, VideoQuality::Low);
        assert_eq!(video_quality("MEDIUM")?, VideoQuality::Medium);
        assert_eq!(video_quality("HIGH")?, VideoQuality::High);
        assert!(matches!(
            video_quality("4K")?,
            VideoQuality::ProviderSpecific(_)
        ));
        Ok(())
    }

    #[test]
    fn top_hit_artists_returns_none() -> TidalResult<()> {
        assert!(top_hit(&TidalTopHitKind::Artists, serde_json::Value::Null)?.is_none());
        assert!(top_hit(&TidalTopHitKind::Other, serde_json::Value::Null)?.is_none());
        Ok(())
    }
}
