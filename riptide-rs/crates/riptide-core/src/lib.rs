//! Provider-neutral types and traits for riptide-rs.

pub mod catalog;
pub mod error;
pub mod id;
pub mod metrics;
pub mod provider;
pub mod quality;
pub mod stream;

pub use catalog::{
    Album, AlbumSummary, Artist, ArtistCredit, ArtistRole, CatalogEntity, Collection,
    CollectionKind, ExternalIdKind, ExternalIds, Playlist, ProviderExtras, Track, Video,
};
pub use error::{CoreError, CoreResult};
pub use id::{
    Codec, ProviderContainerLabel, ProviderExternalIdKind, ProviderId, ProviderQualityLabel,
    ResourceId,
};
pub use metrics::{
    BitDepth, ConcurrencyLimit, DiscNumber, ItemCount, MediaDuration, SampleRateHz, TrackNumber,
};
pub use provider::{
    CollectionProvider, LyricsProvider, Provider, ProviderCapabilities, ProviderRegistry, Resolver,
    StreamProvider,
};
pub use quality::{
    AudioQuality, AudioQualityRequest, QualityRequest, ResolvedQuality, VideoQuality,
    VideoQualityRequest,
};
pub use stream::{
    ContainerKind, Lyrics, LyricsSource, LyricsSourceName, Manifest, PostProcessStep, StreamVariant,
};

pub use id::{MediaRef, ResourceKind};
