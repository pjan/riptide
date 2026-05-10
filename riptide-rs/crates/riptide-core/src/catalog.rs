//! Provider-neutral catalog entities.

use crate::{
    CoverArtId, DiscNumber, ItemCount, MediaDuration, MediaRef, ProviderExternalIdKind, TrackNumber,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// External identifier map.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ExternalIds(pub BTreeMap<ExternalIdKind, String>);

/// Kind of external identifier.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalIdKind {
    /// ISRC recording identifier.
    Isrc,
    /// UPC release identifier.
    Upc,
    /// Provider-specific identifier kind.
    ProviderSpecific(ProviderExternalIdKind),
}

/// Artist role within a specific credit context.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtistRole {
    /// Main artist.
    Main,
    /// Featured artist.
    Featured,
    /// Contributor or credit-only artist.
    Contributor,
    /// Unknown or provider-specific role.
    Unknown,
}

/// Provider-neutral artist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artist {
    /// Provider-resolvable artist reference, if available.
    pub id: Option<MediaRef>,
    /// Display name.
    pub name: String,
}

/// Artist plus context-specific role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtistCredit {
    /// Artist identity.
    pub artist: Artist,
    /// Role in the current context.
    pub role: ArtistRole,
}

/// Lightweight album reference on tracks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlbumSummary {
    /// Provider-resolvable album reference.
    pub id: MediaRef,
    /// Album title.
    pub title: String,
    /// Album artists.
    pub artists: Vec<ArtistCredit>,
    /// Provider cover art identifier when available.
    #[serde(default)]
    pub cover: Option<CoverArtId>,
}

/// Provider-neutral track.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    /// Track reference.
    pub id: MediaRef,
    /// Track title.
    pub title: String,
    /// Track artists.
    pub artists: Vec<ArtistCredit>,
    /// Album summary when available.
    pub album: Option<AlbumSummary>,
    /// Media duration.
    pub duration: Option<MediaDuration>,
    /// Track number.
    pub track_number: Option<TrackNumber>,
    /// Disc number.
    pub disc_number: Option<DiscNumber>,
    /// Explicit content flag.
    pub explicit: Option<bool>,
    /// External IDs.
    pub external_ids: ExternalIds,
    /// Version/remix label.
    #[serde(default)]
    pub version: Option<String>,
    /// Copyright notice.
    #[serde(default)]
    pub copyright: Option<String>,
    /// Beats per minute.
    #[serde(default)]
    pub bpm: Option<i64>,
    /// Provider quality label.
    #[serde(default)]
    pub quality_label: Option<String>,
    /// Dolby Atmos flag.
    #[serde(default)]
    pub dolby_atmos: bool,
}

/// Provider-neutral video.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Video {
    /// Video reference.
    pub id: MediaRef,
    /// Video title.
    pub title: String,
    /// Video artists.
    pub artists: Vec<ArtistCredit>,
    /// Album summary when available.
    pub album: Option<AlbumSummary>,
    /// Media duration.
    pub duration: Option<MediaDuration>,
    /// Track number.
    pub track_number: Option<TrackNumber>,
    /// Disc number.
    pub disc_number: Option<DiscNumber>,
    /// Explicit content flag.
    pub explicit: Option<bool>,
    /// Provider quality label.
    #[serde(default)]
    pub quality_label: Option<String>,
}

/// Provider-neutral album.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Album {
    /// Album reference.
    pub id: MediaRef,
    /// Album title.
    pub title: String,
    /// Album artists.
    pub artists: Vec<ArtistCredit>,
    /// Release date.
    pub release_date: Option<time::Date>,
    /// Number of tracks.
    pub track_count: Option<ItemCount>,
    /// Explicit content flag.
    pub explicit: Option<bool>,
    /// Release type label.
    #[serde(default)]
    pub release_type: Option<String>,
    /// High-resolution lossless flag.
    #[serde(default)]
    pub hires_lossless: bool,
    /// Provider cover art identifier when available.
    #[serde(default)]
    pub cover: Option<CoverArtId>,
}

/// Provider-neutral playlist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Playlist {
    /// Playlist reference.
    pub id: MediaRef,
    /// Playlist title.
    pub title: String,
    /// Owner display name.
    pub owner: Option<String>,
    /// Number of items.
    pub item_count: Option<ItemCount>,
    /// Creation timestamp.
    pub created_at: Option<time::OffsetDateTime>,
    /// Last update timestamp.
    pub updated_at: Option<time::OffsetDateTime>,
}

/// Generic ordered provider collection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Collection {
    /// Collection reference.
    pub id: MediaRef,
    /// Collection kind.
    pub kind: CollectionKind,
    /// Optional title.
    pub title: Option<String>,
    /// Optional owner display name.
    pub owner: Option<String>,
}

/// Generic collection kind for non-playlist provider groupings.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollectionKind {
    /// Album-format collection such as album, EP, single, or compilation.
    AlbumFormat,
    /// Provider-generated mix.
    Mix,
    /// Artist discography expansion.
    ArtistDiscography,
    /// Search result collection.
    SearchResults,
    /// Local filesystem folder.
    LocalFolder,
}

/// Any provider-neutral catalog entity.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogEntity {
    /// Track entity.
    Track(Track),
    /// Video entity.
    Video(Video),
    /// Album entity.
    Album(Album),
    /// Playlist entity.
    Playlist(Playlist),
    /// Artist entity.
    Artist(Artist),
    /// Generic collection entity.
    Collection(Collection),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artist_credit_represents_credit_only_contributor() {
        let credit = ArtistCredit {
            artist: Artist {
                id: None,
                name: "Musician".to_owned(),
            },
            role: ArtistRole::Contributor,
        };

        assert!(credit.artist.id.is_none());
        assert_eq!(credit.role, ArtistRole::Contributor);
    }

    #[test]
    fn provider_native_playlist_uses_catalog_entity_playlist(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let playlist = Playlist {
            id: MediaRef::new(
                crate::ProviderId::new("tidal")?,
                crate::ResourceKind::Playlist,
                crate::ResourceId::new("playlist-id")?,
            ),
            title: "Playlist".to_owned(),
            owner: None,
            item_count: None,
            created_at: None,
            updated_at: None,
        };

        let entity = CatalogEntity::Playlist(playlist);
        assert!(matches!(entity, CatalogEntity::Playlist(_)));
        Ok(())
    }

    #[test]
    fn album_typed_fields_default_correctly() -> Result<(), Box<dyn std::error::Error>> {
        let album = Album {
            id: MediaRef::new(
                crate::ProviderId::new("tidal")?,
                crate::ResourceKind::Album,
                crate::ResourceId::new("1")?,
            ),
            title: "Album".to_owned(),
            artists: Vec::new(),
            release_date: None,
            track_count: None,
            explicit: None,
            release_type: Some("ALBUM".to_owned()),
            hires_lossless: true,
            cover: None,
        };
        assert!(album.hires_lossless);
        assert_eq!(album.release_type.as_deref(), Some("ALBUM"));
        Ok(())
    }
}
