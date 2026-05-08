//! TIDAL resource parsing.

use crate::{TidalError, TidalResult, TIDAL_PROVIDER_ID};
use riptide_core::{MediaRef, ResourceId, ResourceKind};

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
        let path = url::Url::parse(input).map_or_else(
            |_| input.trim_matches('/').to_owned(),
            |url| url.path().trim_matches('/').to_owned(),
        );
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
        Ok(MediaRef::new(TIDAL_PROVIDER_ID.clone(), self.kind, self.id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_tidal_url() -> TidalResult<()> {
        let resource =
            TidalResource::parse("https://listen.tidal.com/track/12345")?.ok_or_else(|| {
                TidalError::InvalidValue {
                    field: "test",
                    message: "expected Some".to_owned(),
                }
            })?;
        assert_eq!(resource.kind, ResourceKind::Track);
        assert_eq!(resource.id.as_str(), "12345");
        Ok(())
    }

    #[test]
    fn parses_path_style_input() -> TidalResult<()> {
        let resource = TidalResource::parse("track/12345")?.ok_or_else(|| {
            TidalError::InvalidValue {
                field: "test",
                message: "expected Some".to_owned(),
            }
        })?;
        assert_eq!(resource.kind, ResourceKind::Track);
        assert_eq!(resource.id.as_str(), "12345");
        Ok(())
    }

    #[test]
    fn parses_plural_resource_kinds() -> TidalResult<()> {
        let cases = [
            ("tracks/1", ResourceKind::Track),
            ("videos/2", ResourceKind::Video),
            ("albums/3", ResourceKind::Album),
            ("playlists/4", ResourceKind::Playlist),
            ("artists/5", ResourceKind::Artist),
            ("mixes/6", ResourceKind::Mix),
        ];
        for (input, expected_kind) in cases {
            let resource = TidalResource::parse(input)?.ok_or_else(|| {
                TidalError::InvalidValue {
                    field: "test",
                    message: format!("expected Some for {input}"),
                }
            })?;
            assert_eq!(resource.kind, expected_kind);
        }
        Ok(())
    }

    #[test]
    fn trims_leading_and_trailing_slashes() -> TidalResult<()> {
        let resource = TidalResource::parse("/track/12345/")?.ok_or_else(|| {
            TidalError::InvalidValue {
                field: "test",
                message: "expected Some".to_owned(),
            }
        })?;
        assert_eq!(resource.kind, ResourceKind::Track);
        assert_eq!(resource.id.as_str(), "12345");
        Ok(())
    }

    #[test]
    fn unknown_resource_kind_returns_none() -> TidalResult<()> {
        assert!(TidalResource::parse("unknown/12345")?.is_none());
        Ok(())
    }

    #[test]
    fn empty_string_returns_none() -> TidalResult<()> {
        assert!(TidalResource::parse("")?.is_none());
        Ok(())
    }

    #[test]
    fn kind_only_without_id_returns_none() -> TidalResult<()> {
        assert!(TidalResource::parse("track")?.is_none());
        Ok(())
    }

    #[test]
    fn media_ref_produces_tidal_provider() -> TidalResult<()> {
        let resource = TidalResource::parse("track/99")?.ok_or_else(|| {
            TidalError::InvalidValue {
                field: "test",
                message: "expected Some".to_owned(),
            }
        })?;
        let reference = resource.media_ref()?;
        assert_eq!(reference.provider.as_str(), "tidal");
        assert_eq!(reference.kind, ResourceKind::Track);
        assert_eq!(reference.id.as_str(), "99");
        Ok(())
    }
}
