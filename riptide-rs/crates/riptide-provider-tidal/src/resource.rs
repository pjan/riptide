//! TIDAL resource parsing.

use crate::{TidalError, TidalResult};
use riptide_core::{MediaRef, ProviderId, ResourceId, ResourceKind};

/// Parsed TIDAL resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TidalResource {
    /// Resource kind.
    pub kind: ResourceKind,
    /// Resource ID.
    pub id: ResourceId,
}

impl TidalResource {
    /// Parses `track/123` or full TIDAL listen URLs.
    pub fn parse(input: &str) -> TidalResult<Option<Self>> {
        let path = if let Ok(url) = url::Url::parse(input) {
            url.path().trim_matches('/').to_owned()
        } else {
            input.trim_matches('/').to_owned()
        };
        let mut parts = path.split('/');
        let Some(kind) = parts.next() else {
            return Ok(None);
        };
        let Some(id) = parts.next() else {
            return Ok(None);
        };
        let kind = match kind {
            "track" | "tracks" => ResourceKind::Track,
            "video" | "videos" => ResourceKind::Video,
            "album" | "albums" => ResourceKind::Album,
            "playlist" | "playlists" => ResourceKind::Playlist,
            "artist" | "artists" => ResourceKind::Artist,
            "mix" | "mixes" => ResourceKind::Mix,
            _ => return Ok(None),
        };
        Ok(Some(Self {
            kind,
            id: ResourceId::new(id).map_err(|error| TidalError::InvalidValue {
                field: "resource_id",
                message: error.to_string(),
            })?,
        }))
    }

    /// Converts to a provider-neutral reference.
    pub fn media_ref(self) -> TidalResult<MediaRef> {
        Ok(MediaRef::new(
            ProviderId::new("tidal").map_err(|error| TidalError::InvalidValue {
                field: "provider",
                message: error.to_string(),
            })?,
            self.kind,
            self.id,
        ))
    }
}
