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
    ///
    /// When the path contains multiple recognised resource segments (e.g.
    /// `album/111/track/222`) the **last** one wins, so a track URL nested
    /// under an album still resolves as a track.
    pub fn parse(input: &str) -> TidalResult<Option<Self>> {
        let path = url::Url::parse(input).map_or_else(
            |_| input.trim_matches('/').to_owned(),
            |url| url.path().trim_matches('/').to_owned(),
        );
        let parts: Vec<&str> = path.split('/').collect();

        // Walk pairs from the end so that `album/111/track/222` resolves as
        // `track/222` rather than `album/111`.
        let mut i = parts.len().saturating_sub(1);
        while i >= 1 {
            if let Some(kind) = parse_resource_kind(parts[i - 1]) {
                let id = ResourceId::new(parts[i]).map_err(|error| TidalError::InvalidValue {
                    field: "resource_id",
                    message: error.to_string(),
                })?;
                return Ok(Some(Self { kind, id }));
            }
            // Step back by 2 so we test the next kind/id pair. When i == 1
            // the subtraction would underflow, so break explicitly.
            if i < 2 {
                break;
            }
            i -= 2;
        }

        Ok(None)
    }

    /// Converts to a provider-neutral reference.
    pub fn media_ref(self) -> TidalResult<MediaRef> {
        Ok(MediaRef::new(TIDAL_PROVIDER_ID.clone(), self.kind, self.id))
    }
}

fn parse_resource_kind(segment: &str) -> Option<ResourceKind> {
    match segment {
        "track" | "tracks" => Some(ResourceKind::Track),
        "video" | "videos" => Some(ResourceKind::Video),
        "album" | "albums" => Some(ResourceKind::Album),
        "playlist" | "playlists" => Some(ResourceKind::Playlist),
        "artist" | "artists" => Some(ResourceKind::Artist),
        "mix" | "mixes" => Some(ResourceKind::Mix),
        _ => None,
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
        let resource =
            TidalResource::parse("track/12345")?.ok_or_else(|| TidalError::InvalidValue {
                field: "test",
                message: "expected Some".to_owned(),
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
            let resource =
                TidalResource::parse(input)?.ok_or_else(|| TidalError::InvalidValue {
                    field: "test",
                    message: format!("expected Some for {input}"),
                })?;
            assert_eq!(resource.kind, expected_kind);
        }
        Ok(())
    }

    #[test]
    fn trims_leading_and_trailing_slashes() -> TidalResult<()> {
        let resource =
            TidalResource::parse("/track/12345/")?.ok_or_else(|| TidalError::InvalidValue {
                field: "test",
                message: "expected Some".to_owned(),
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
    fn nested_track_url_resolves_as_track() -> TidalResult<()> {
        let resource =
            TidalResource::parse("https://listen.tidal.com/album/111/track/222")?.ok_or_else(
                || TidalError::InvalidValue {
                    field: "test",
                    message: "expected Some".to_owned(),
                },
            )?;
        assert_eq!(resource.kind, ResourceKind::Track);
        assert_eq!(resource.id.as_str(), "222");
        Ok(())
    }

    #[test]
    fn browse_track_url_resolves_as_track() -> TidalResult<()> {
        let resource =
            TidalResource::parse("https://tidal.com/browse/track/12345")?.ok_or_else(|| {
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
    fn browse_album_url_resolves_as_album() -> TidalResult<()> {
        let resource =
            TidalResource::parse("https://tidal.com/browse/album/99999")?.ok_or_else(|| {
                TidalError::InvalidValue {
                    field: "test",
                    message: "expected Some".to_owned(),
                }
            })?;
        assert_eq!(resource.kind, ResourceKind::Album);
        assert_eq!(resource.id.as_str(), "99999");
        Ok(())
    }

    #[test]
    fn media_ref_produces_tidal_provider() -> TidalResult<()> {
        let resource =
            TidalResource::parse("track/99")?.ok_or_else(|| TidalError::InvalidValue {
                field: "test",
                message: "expected Some".to_owned(),
            })?;
        let reference = resource.media_ref()?;
        assert_eq!(reference.provider.as_str(), "tidal");
        assert_eq!(reference.kind, ResourceKind::Track);
        assert_eq!(reference.id.as_str(), "99");
        Ok(())
    }
}
