//! Identifier and label newtypes.

use crate::{CoreError, CoreResult};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{fmt, str::FromStr};

macro_rules! non_empty_string_type {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            /// Creates a new value after rejecting empty or whitespace-only input.
            pub fn new(value: impl Into<String>) -> CoreResult<Self> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err(CoreError::EmptyValue {
                        type_name: stringify!($name),
                    });
                }
                Ok(Self(value))
            }

            /// Returns the underlying string.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_tuple(stringify!($name)).field(&self.0).finish()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl TryFrom<&str> for $name {
            type Error = CoreError;

            fn try_from(value: &str) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl TryFrom<String> for $name {
            type Error = CoreError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl FromStr for $name {
            type Err = CoreError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value)
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::new(value).map_err(serde::de::Error::custom)
            }
        }
    };
}

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

/// Provider-neutral media resource kind.
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
    pub fn new(provider: ProviderId, kind: ResourceKind, id: ResourceId) -> Self {
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
