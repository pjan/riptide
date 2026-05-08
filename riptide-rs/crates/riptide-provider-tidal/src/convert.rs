//! TIDAL model conversion into provider-neutral core types.

use crate::{model::*, CountryCode, TidalError, TidalResult};
use riptide_core::{
    Album, AlbumSummary, Artist, ArtistCredit, ArtistRole, AudioQuality, CatalogEntity,
    ExternalIdKind, ExternalIds, ItemCount, Lyrics, LyricsSource, LyricsSourceName, MediaDuration,
    MediaRef, Playlist, ProviderExtras, ProviderId, ProviderQualityLabel, ResourceId, ResourceKind,
    Track, TrackNumber, Video, VideoQuality,
};
use std::collections::BTreeMap;

fn tidal_provider() -> ProviderId {
    match ProviderId::new("tidal") {
        Ok(id) => id,
        Err(_) => unreachable!("literal provider ID is valid"),
    }
}

fn media_ref(kind: ResourceKind, id: impl ToString) -> TidalResult<MediaRef> {
    Ok(MediaRef::new(
        tidal_provider(),
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
    let mut extras = serde_json::Map::new();
    if let Some(version) = track.version {
        extras.insert("version".to_owned(), serde_json::Value::String(version));
    }
    if let Some(copyright) = track.copyright {
        extras.insert("copyright".to_owned(), serde_json::Value::String(copyright));
    }
    if let Some(bpm) = track.bpm {
        extras.insert("bpm".to_owned(), serde_json::Value::Number(bpm.into()));
    }
    if let Some(quality) = track.audio_quality.clone() {
        extras.insert("quality".to_owned(), serde_json::Value::String(quality));
    }
    extras.insert(
        "dolby".to_owned(),
        serde_json::Value::Bool(
            track
                .media_metadata
                .tags
                .iter()
                .any(|tag| tag == "DOLBY_ATMOS"),
        ),
    );
    Ok(Track {
        id: media_ref(ResourceKind::Track, track.id)?,
        title: track.title,
        artists: artists(track.artist, track.artists)?,
        album: track
            .album
            .and_then(|album| album.id.zip(album.title))
            .map(|(id, title)| {
                Ok(AlbumSummary {
                    id: media_ref(ResourceKind::Album, id)?,
                    title,
                    artists: Vec::new(),
                })
            })
            .transpose()?,
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
        extras: ProviderExtras(serde_json::Value::Object(extras)),
    })
}

/// Converts a TIDAL video.
pub fn video(video: TidalVideo) -> TidalResult<Video> {
    let mut extras = serde_json::Map::new();
    if let Some(quality) = video.quality {
        extras.insert("quality".to_owned(), serde_json::Value::String(quality));
    }
    Ok(Video {
        id: media_ref(ResourceKind::Video, video.id)?,
        title: video.title,
        artists: artists(video.artist, video.artists)?,
        album: video
            .album
            .and_then(|album| album.id.zip(album.title))
            .map(|(id, title)| {
                Ok(AlbumSummary {
                    id: media_ref(ResourceKind::Album, id)?,
                    title,
                    artists: Vec::new(),
                })
            })
            .transpose()?,
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
        extras: ProviderExtras(serde_json::Value::Object(extras)),
    })
}

/// Converts a TIDAL album.
pub fn album(album: TidalAlbum) -> TidalResult<Album> {
    let mut extras = serde_json::Map::new();
    if let Some(release) = album.release_type {
        extras.insert("release".to_owned(), serde_json::Value::String(release));
    }
    extras.insert(
        "hires_lossless".to_owned(),
        serde_json::Value::Bool(
            album
                .media_metadata
                .tags
                .iter()
                .any(|tag| tag == "HIRES_LOSSLESS"),
        ),
    );
    Ok(Album {
        id: media_ref(ResourceKind::Album, album.id)?,
        title: album.title,
        artists: artists(album.artist, album.artists)?,
        release_date: album.release_date.map(parse_date).transpose()?,
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
        extras: ProviderExtras(serde_json::Value::Object(extras)),
    })
}

/// Converts a TIDAL playlist.
pub fn playlist(playlist: TidalPlaylist) -> TidalResult<Playlist> {
    let item_count = playlist
        .number_of_tracks
        .unwrap_or(0)
        .saturating_add(playlist.number_of_videos.unwrap_or(0));
    Ok(Playlist {
        id: media_ref(ResourceKind::Playlist, playlist.uuid.clone())?,
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
        created_at: playlist.created.map(parse_datetime).transpose()?,
        updated_at: playlist.last_updated.map(parse_datetime).transpose()?,
        extras: ProviderExtras::default(),
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
                        .map(|id| media_ref(ResourceKind::Artist, id))
                        .transpose()?,
                    name: artist.name,
                },
                role: match artist.kind.as_deref() {
                    Some("MAIN") => ArtistRole::Main,
                    Some("FEATURED") => ArtistRole::Featured,
                    Some(_) => ArtistRole::Contributor,
                    None => ArtistRole::Unknown,
                },
            })
        })
        .collect()
}

fn parse_date(value: String) -> TidalResult<time::Date> {
    time::Date::parse(&value, &time::format_description::well_known::Iso8601::DATE).map_err(
        |error| TidalError::InvalidResponse {
            field: "date".to_owned(),
            message: error.to_string(),
        },
    )
}

fn parse_datetime(value: String) -> TidalResult<time::OffsetDateTime> {
    time::OffsetDateTime::parse(&value, &time::format_description::well_known::Rfc3339).map_err(
        |error| TidalError::InvalidResponse {
            field: "datetime".to_owned(),
            message: error.to_string(),
        },
    )
}

/// Converts a top-hit raw value.
pub fn top_hit(kind: &str, value: serde_json::Value) -> TidalResult<Option<CatalogEntity>> {
    match kind {
        "TRACKS" => Ok(Some(CatalogEntity::Track(track(
            serde_json::from_value(value).map_err(|error| TidalError::InvalidResponse {
                field: "topHit.track".to_owned(),
                message: error.to_string(),
            })?,
        )?))),
        "ALBUMS" => Ok(Some(CatalogEntity::Album(album(
            serde_json::from_value(value).map_err(|error| TidalError::InvalidResponse {
                field: "topHit.album".to_owned(),
                message: error.to_string(),
            })?,
        )?))),
        "PLAYLISTS" => Ok(Some(CatalogEntity::Playlist(playlist(
            serde_json::from_value(value).map_err(|error| TidalError::InvalidResponse {
                field: "topHit.playlist".to_owned(),
                message: error.to_string(),
            })?,
        )?))),
        "ARTISTS" => Ok(None),
        _ => Ok(None),
    }
}
