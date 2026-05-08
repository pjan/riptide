//! Provider capability traits and registry.

use crate::{
    CatalogEntity, CoreError, CoreResult, Lyrics, MediaRef, ProviderId, QualityRequest,
    StreamVariant,
};
use async_trait::async_trait;
use std::{collections::HashMap, sync::Arc};

/// Resolves input strings and provider references into catalog entities.
#[async_trait]
pub trait Resolver: Send + Sync {
    /// Attempts to parse an input string into a provider-owned media reference.
    async fn parse_ref(&self, input: &str) -> CoreResult<Option<MediaRef>>;

    /// Resolves a media reference into a catalog entity.
    async fn resolve(&self, reference: &MediaRef) -> CoreResult<CatalogEntity>;
}

/// Lists children of collection-like provider resources.
#[async_trait]
pub trait CollectionProvider: Send + Sync {
    /// Returns ordered children for a provider reference.
    async fn children(&self, reference: &MediaRef) -> CoreResult<Vec<CatalogEntity>>;
}

/// Resolves media references into stream variants.
#[async_trait]
pub trait StreamProvider: Send + Sync {
    /// Returns stream variants for a media reference.
    async fn streams(
        &self,
        reference: &MediaRef,
        requested: QualityRequest,
    ) -> CoreResult<Vec<StreamVariant>>;
}

/// Fetches lyrics for provider resources.
#[async_trait]
pub trait LyricsProvider: Send + Sync {
    /// Returns lyrics if available.
    async fn lyrics(&self, reference: &MediaRef) -> CoreResult<Option<Lyrics>>;
}

/// First-phase provider supertrait.
///
/// This shape is intentionally simple for TIDAL parity. Slice 8 must revisit
/// whether the registry should split by capability before local/import-only
/// providers are added.
pub trait Provider:
    Resolver + CollectionProvider + StreamProvider + LyricsProvider + Send + Sync
{
    /// Provider identifier.
    fn id(&self) -> &ProviderId;

    /// Capabilities exposed by this provider.
    fn capabilities(&self) -> ProviderCapabilities;
}

/// Capability flags for a provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProviderCapabilities {
    /// Can parse and resolve references.
    pub resolve: bool,
    /// Can list collection children.
    pub collections: bool,
    /// Can resolve streams.
    pub streams: bool,
    /// Can fetch lyrics.
    pub lyrics: bool,
    /// Can fetch artwork.
    pub artwork: bool,
    /// Can fetch credits.
    pub credits: bool,
    /// Can search catalog.
    pub search: bool,
}

/// In-process provider registry keyed by provider ID.
#[derive(Default)]
pub struct ProviderRegistry {
    providers: HashMap<ProviderId, Arc<dyn Provider>>,
}

impl ProviderRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a provider, rejecting duplicate IDs.
    pub fn register(&mut self, provider: Arc<dyn Provider>) -> CoreResult<()> {
        let id = provider.id().clone();
        if self.providers.contains_key(&id) {
            return Err(CoreError::DuplicateProvider { provider: id });
        }
        self.providers.insert(id, provider);
        Ok(())
    }

    /// Returns a registered provider.
    pub fn get(&self, provider: &ProviderId) -> CoreResult<Arc<dyn Provider>> {
        self.providers
            .get(provider)
            .cloned()
            .ok_or_else(|| CoreError::MissingProvider {
                provider: provider.clone(),
            })
    }

    /// Returns the number of registered providers.
    pub fn len(&self) -> usize {
        self.providers.len()
    }

    /// Returns true when the registry has no providers.
    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ResourceId, ResourceKind};

    struct FakeProvider {
        id: ProviderId,
    }

    impl FakeProvider {
        fn new(id: &str) -> CoreResult<Self> {
            Ok(Self {
                id: ProviderId::new(id)?,
            })
        }
    }

    #[async_trait]
    impl Resolver for FakeProvider {
        async fn parse_ref(&self, _input: &str) -> CoreResult<Option<MediaRef>> {
            Ok(None)
        }

        async fn resolve(&self, reference: &MediaRef) -> CoreResult<CatalogEntity> {
            Err(CoreError::UnsupportedResourceKind {
                provider: self.id.clone(),
                kind: reference.kind,
                operation: "resolve",
            })
        }
    }

    #[async_trait]
    impl CollectionProvider for FakeProvider {
        async fn children(&self, _reference: &MediaRef) -> CoreResult<Vec<CatalogEntity>> {
            Err(CoreError::UnsupportedCapability {
                provider: self.id.clone(),
                capability: "collections",
            })
        }
    }

    #[async_trait]
    impl StreamProvider for FakeProvider {
        async fn streams(
            &self,
            _reference: &MediaRef,
            _requested: QualityRequest,
        ) -> CoreResult<Vec<StreamVariant>> {
            Err(CoreError::UnsupportedCapability {
                provider: self.id.clone(),
                capability: "streams",
            })
        }
    }

    #[async_trait]
    impl LyricsProvider for FakeProvider {
        async fn lyrics(&self, _reference: &MediaRef) -> CoreResult<Option<Lyrics>> {
            Err(CoreError::UnsupportedCapability {
                provider: self.id.clone(),
                capability: "lyrics",
            })
        }
    }

    impl Provider for FakeProvider {
        fn id(&self) -> &ProviderId {
            &self.id
        }

        fn capabilities(&self) -> ProviderCapabilities {
            ProviderCapabilities {
                resolve: true,
                ..ProviderCapabilities::default()
            }
        }
    }

    #[test]
    fn registry_rejects_duplicate_provider_ids() -> Result<(), Box<dyn std::error::Error>> {
        let mut registry = ProviderRegistry::new();
        registry.register(Arc::new(FakeProvider::new("fake")?))?;
        let duplicate = registry.register(Arc::new(FakeProvider::new("fake")?));
        assert!(matches!(
            duplicate,
            Err(CoreError::DuplicateProvider { .. })
        ));
        Ok(())
    }

    #[test]
    fn registry_returns_provider_by_id() -> Result<(), Box<dyn std::error::Error>> {
        let mut registry = ProviderRegistry::new();
        let id = ProviderId::new("fake")?;
        registry.register(Arc::new(FakeProvider { id: id.clone() }))?;
        let provider = registry.get(&id)?;
        assert_eq!(provider.id(), &id);
        Ok(())
    }

    #[test]
    fn registry_returns_missing_provider_error() -> Result<(), Box<dyn std::error::Error>> {
        let registry = ProviderRegistry::new();
        let missing = registry.get(&ProviderId::new("missing")?);
        assert!(matches!(missing, Err(CoreError::MissingProvider { .. })));
        Ok(())
    }

    #[test]
    fn fake_provider_can_construct_unsupported_capability_error(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let provider = FakeProvider::new("fake")?;
        let error = CoreError::UnsupportedCapability {
            provider: provider.id().clone(),
            capability: "streams",
        };
        assert_eq!(
            error.to_string(),
            "provider `fake` does not support streams"
        );
        Ok(())
    }

    #[test]
    fn media_ref_uses_hashable_provider_id() -> Result<(), Box<dyn std::error::Error>> {
        let reference = MediaRef::new(
            ProviderId::new("fake")?,
            ResourceKind::Track,
            ResourceId::new("1")?,
        );
        assert_eq!(reference.provider.as_str(), "fake");
        Ok(())
    }
}
