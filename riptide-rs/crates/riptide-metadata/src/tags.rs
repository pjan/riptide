//! Metadata model and tag-writing boundary.

use crate::{MetadataError, MetadataResult};
use lofty::{
    config::WriteOptions,
    file::{AudioFile, TaggedFileExt},
    picture::{Picture, PictureType},
    prelude::ItemKey,
    tag::{Tag, TagType},
};
use riptide_core::{
    Album, ArtistRole, ContainerExtension, DiscNumber, ExternalIdKind, Track, TrackNumber, Video,
};
use std::{io::Cursor, path::Path, sync::Arc};

/// Metadata to write to a media file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMetadata {
    /// Title.
    pub title: String,
    /// Primary artist.
    pub artist: String,
    /// Artists.
    pub artists: Vec<String>,
    /// Album title.
    pub album_title: Option<String>,
    /// Album artist.
    pub album_artist: Option<String>,
    /// Track number.
    pub track_number: Option<TrackNumber>,
    /// Disc number.
    pub disc_number: Option<DiscNumber>,
    /// Release date.
    pub date: Option<time::Date>,
    /// Copyright.
    pub copyright: Option<String>,
    /// ISRC.
    pub isrc: Option<String>,
    /// Beats per minute.
    pub bpm: Option<i64>,
    /// Lyrics text.
    pub lyrics: Option<String>,
    /// Cover bytes.
    pub cover_data: Option<Arc<[u8]>>,
    /// Comment.
    pub comment: Option<String>,
}

/// Builds file metadata from a track and optional album context.
pub fn metadata_from_track(track: &Track, album: Option<&Album>) -> FileMetadata {
    let artists = artist_names(&track.artists);
    let album_artists = album.map(|album| artist_names(&album.artists)).or_else(|| {
        track
            .album
            .as_ref()
            .map(|album| artist_names(&album.artists))
    });
    FileMetadata {
        title: track.title.clone(),
        artist: artists.first().cloned().unwrap_or_default(),
        artists,
        album_title: album
            .map(|album| album.title.clone())
            .or_else(|| track.album.as_ref().map(|album| album.title.clone())),
        album_artist: album_artists.and_then(|artists| artists.first().cloned()),
        track_number: track.track_number,
        disc_number: track.disc_number,
        date: album.and_then(|album| album.release_date),
        copyright: track.copyright.clone(),
        isrc: track.external_ids.0.get(&ExternalIdKind::Isrc).cloned(),
        bpm: track.bpm,
        lyrics: None,
        cover_data: None,
        comment: None,
    }
}

/// Builds file metadata from a video.
pub fn metadata_from_video(video: &Video) -> FileMetadata {
    let artists = artist_names(&video.artists);
    FileMetadata {
        title: video.title.clone(),
        artist: artists.first().cloned().unwrap_or_default(),
        artists,
        album_title: video.album.as_ref().map(|album| album.title.clone()),
        album_artist: video
            .album
            .as_ref()
            .and_then(|album| artist_names(&album.artists).first().cloned()),
        track_number: video.track_number,
        disc_number: video.disc_number,
        date: None,
        copyright: None,
        isrc: None,
        bpm: None,
        lyrics: None,
        cover_data: None,
        comment: None,
    }
}

/// Writes metadata tags to a media file.
pub fn write_tags(
    path: &Path,
    container: ContainerExtension,
    metadata: &FileMetadata,
) -> MetadataResult<()> {
    let tag_type = match container {
        ContainerExtension::Flac => TagType::VorbisComments,
        ContainerExtension::M4a | ContainerExtension::Mp4 => TagType::Mp4Ilst,
        ContainerExtension::Ts => {
            return Err(MetadataError::UnsupportedContainer {
                extension: container,
            })
        }
    };
    let mut tagged_file =
        lofty::read_from_path(path).map_err(|source| MetadataError::TagWrite { source })?;
    let tag = build_tag(tag_type, metadata)?;
    tagged_file.insert_tag(tag);
    tagged_file
        .save_to_path(path, WriteOptions::default())
        .map_err(|source| MetadataError::TagWrite { source })
}

fn build_tag(tag_type: TagType, metadata: &FileMetadata) -> MetadataResult<Tag> {
    let mut tag = Tag::new(tag_type);
    insert_non_empty(&mut tag, ItemKey::TrackTitle, metadata.title.as_str());
    insert_non_empty(&mut tag, ItemKey::TrackArtist, metadata.artist.as_str());
    insert_many(&mut tag, ItemKey::TrackArtists, &metadata.artists);
    if let Some(album_title) = &metadata.album_title {
        insert_non_empty(&mut tag, ItemKey::AlbumTitle, album_title);
    }
    if let Some(album_artist) = &metadata.album_artist {
        insert_non_empty(&mut tag, ItemKey::AlbumArtist, album_artist);
    }
    if let Some(track_number) = metadata.track_number {
        tag.insert_text(ItemKey::TrackNumber, track_number.get().to_string());
    }
    if let Some(disc_number) = metadata.disc_number {
        tag.insert_text(ItemKey::DiscNumber, disc_number.get().to_string());
    }
    if let Some(date) = metadata.date {
        tag.insert_text(ItemKey::RecordingDate, date.to_string());
    }
    if let Some(copyright) = &metadata.copyright {
        insert_non_empty(&mut tag, ItemKey::CopyrightMessage, copyright);
    }
    if let Some(isrc) = &metadata.isrc {
        insert_non_empty(&mut tag, ItemKey::Isrc, isrc);
    }
    if let Some(bpm) = metadata.bpm {
        tag.insert_text(ItemKey::Bpm, bpm.to_string());
    }
    if let Some(lyrics) = &metadata.lyrics {
        insert_non_empty(&mut tag, ItemKey::Lyrics, lyrics);
        insert_non_empty(&mut tag, ItemKey::UnsyncLyrics, lyrics);
    }
    if let Some(comment) = &metadata.comment {
        insert_non_empty(&mut tag, ItemKey::Comment, comment);
    }
    if let Some(cover_data) = &metadata.cover_data {
        let mut cursor = Cursor::new(cover_data.as_ref());
        let mut picture = Picture::from_reader(&mut cursor)
            .map_err(|source| MetadataError::TagWrite { source })?;
        picture.set_pic_type(PictureType::CoverFront);
        tag.remove_picture_type(PictureType::CoverFront);
        tag.push_picture(picture);
    }
    Ok(tag)
}

fn insert_non_empty(tag: &mut Tag, key: ItemKey, value: &str) {
    if !value.trim().is_empty() {
        tag.insert_text(key, value.to_owned());
    }
}

fn insert_many(tag: &mut Tag, key: ItemKey, values: &[String]) {
    for value in values.iter().filter(|value| !value.trim().is_empty()) {
        tag.push(lofty::tag::TagItem::new(
            key,
            lofty::tag::ItemValue::Text(value.clone()),
        ));
    }
}

fn artist_names(credits: &[riptide_core::ArtistCredit]) -> Vec<String> {
    let mut names: Vec<_> = credits
        .iter()
        .filter(|credit| {
            matches!(
                credit.role,
                ArtistRole::Main
                    | ArtistRole::Featured
                    | ArtistRole::Contributor
                    | ArtistRole::Unknown
            )
        })
        .map(|credit| credit.artist.name.clone())
        .collect();
    names.sort();
    names.dedup();
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use riptide_core::{
        Artist, ArtistCredit, ExternalIds, MediaRef, ProviderId, ResourceId, ResourceKind,
    };

    #[test]
    fn metadata_from_track_preserves_typed_numbers() -> Result<(), Box<dyn std::error::Error>> {
        let track = Track {
            id: MediaRef::new(
                ProviderId::new("tidal")?,
                ResourceKind::Track,
                ResourceId::new("1")?,
            ),
            title: "Track".to_owned(),
            artists: vec![ArtistCredit {
                artist: Artist {
                    id: None,
                    name: "Artist".to_owned(),
                },
                role: ArtistRole::Main,
            }],
            album: None,
            duration: None,
            track_number: Some(TrackNumber::new(2)?),
            disc_number: Some(DiscNumber::new(1)?),
            explicit: None,
            external_ids: ExternalIds::default(),
            version: None,
            copyright: None,
            bpm: Some(120),
            quality_label: None,
            dolby_atmos: false,
        };
        let metadata = metadata_from_track(&track, None);
        assert_eq!(metadata.track_number, Some(TrackNumber::new(2)?));
        assert_eq!(metadata.bpm, Some(120));
        Ok(())
    }
}
