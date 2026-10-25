//! Provider-neutral types and traits for riptide-rs.

macro_rules! non_empty_string_type {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            /// Creates a new value after rejecting empty or whitespace-only input.
            pub fn new(value: impl Into<String>) -> $crate::CoreResult<Self> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err($crate::CoreError::EmptyValue {
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

        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_tuple(stringify!($name)).field(&self.0).finish()
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl TryFrom<&str> for $name {
            type Error = $crate::CoreError;

            fn try_from(value: &str) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl TryFrom<String> for $name {
            type Error = $crate::CoreError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl std::str::FromStr for $name {
            type Err = $crate::CoreError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value)
            }
        }

        impl serde::Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                serializer.serialize_str(&self.0)
            }
        }

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let value = <String as serde::Deserialize>::deserialize(deserializer)?;
                Self::new(value).map_err(serde::de::Error::custom)
            }
        }
    };
}

mod catalog;
mod error;
mod id;
mod metrics;
mod provider;
mod quality;
mod stream;

pub use catalog::{
    Album, AlbumSummary, Artist, ArtistCredit, ArtistRole, CatalogEntity, Collection,
    CollectionKind, ExternalIdKind, ExternalIds, Playlist, Track, Video,
};
pub use error::{CoreError, CoreResult, ProviderErrorKind};
pub use id::{
    Codec, CoverArtId, ProviderContainerLabel, ProviderExternalIdKind, ProviderId,
    ProviderQualityLabel, ResourceId,
};
pub use metrics::{
    BitDepth, ConcurrencyLimit, DiscNumber, ItemCount, MediaDuration, SampleRateHz, TrackNumber,
};
pub use provider::{
    CollectionProvider, LyricsProvider, Provider, ProviderCapabilities, ProviderRegistry, Resolver,
    StreamProvider,
};
pub use quality::{AudioQuality, AudioQualityRequest, QualityRequest, ResolvedQuality};
pub use stream::{
    ContainerExtension, ContainerKind, Lyrics, LyricsSource, LyricsSourceName, Manifest,
    PostProcessStep, StreamVariant,
};

pub use id::{MediaRef, ResourceKind};
