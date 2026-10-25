//! Identifier and label newtypes.

use serde::{Deserialize, Serialize};

non_empty_string_type!(
    /// Identifier for a registered provider.
    ProviderId
);
non_empty_string_type!(
    /// Provider-local resource identifier.
    ResourceId
);
non_empty_string_type!(
    /// Codec label used in stream metadata.
    Codec
);
non_empty_string_type!(
    /// Provider-specific external ID kind label.
    ProviderExternalIdKind
);
non_empty_string_type!(
    /// Provider-specific quality label.
    ProviderQualityLabel
);
non_empty_string_type!(
    /// Provider-specific container label.
    ProviderContainerLabel
);
non_empty_string_type!(
    /// Provider cover art identifier used to request artwork bytes.
    CoverArtId
);

/// Provider-neutral media resource kind.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    /// Track resource.
    Track,
    /// Video resource.
    Video,
    /// Album resource.
    Album,
    /// Playlist resource.
    Playlist,
    /// Artist resource.
    Artist,
    /// Provider-specific mix resource.
    Mix,
}

/// Fully-qualified provider resource reference.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct MediaRef {
    /// Provider that owns the resource.
    pub provider: ProviderId,
    /// Resource kind.
    pub kind: ResourceKind,
    /// Provider-local resource ID.
    pub id: ResourceId,
}

impl MediaRef {
    /// Creates a new fully-qualified media reference.
    pub const fn new(provider: ProviderId, kind: ResourceKind, id: ResourceId) -> Self {
        Self { provider, kind, id }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_id_rejects_empty() {
        assert!(ProviderId::new(" ").is_err());
    }

    #[test]
    fn resource_id_rejects_empty() {
        assert!(ResourceId::new("").is_err());
    }

    #[test]
    fn valid_ids_preserve_value() -> Result<(), Box<dyn std::error::Error>> {
        let id = ProviderId::new("tidal")?;
        assert_eq!(id.as_str(), "tidal");
        Ok(())
    }

    #[test]
    fn resource_kind_uses_snake_case_serde() -> Result<(), Box<dyn std::error::Error>> {
        let json = serde_json::to_string(&ResourceKind::Playlist)?;
        assert_eq!(json, "\"playlist\"");
        let kind: ResourceKind = serde_json::from_str("\"artist\"")?;
        assert_eq!(kind, ResourceKind::Artist);
        Ok(())
    }
}
