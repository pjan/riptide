//! Provider trait implementation for TIDAL.

use crate::{
    resource::TidalResource, TidalApiCache, TidalAuthClient, TidalClient, TidalError,
    TidalTokenStore,
};
use async_trait::async_trait;
use riptide_core::{
    CatalogEntity, CollectionProvider, CoreError, CoreResult, Lyrics, LyricsProvider, MediaRef,
    Provider, ProviderCapabilities, ProviderId, QualityRequest, Resolver, StreamProvider,
    StreamVariant,
};

/// TIDAL provider construction options.
#[derive(Debug, Clone)]
pub struct TidalProviderOptions {
    /// Auth client.
    pub auth_client: TidalAuthClient,
    /// API client.
    pub api_client: TidalClient,
    /// Token store.
    pub token_store: TidalTokenStore,
    /// API cache.
    pub cache: TidalApiCache,
}

/// TIDAL provider.
#[derive(Debug, Clone)]
pub struct TidalProvider {
    id: ProviderId,
    client: TidalClient,
}

impl TidalProvider {
    /// Creates a provider.
    pub fn new(options: TidalProviderOptions) -> CoreResult<Self> {
        Ok(Self {
            id: ProviderId::new("tidal")?,
            client: options.api_client,
        })
    }
}

#[async_trait]
impl Resolver for TidalProvider {
    async fn parse_ref(&self, input: &str) -> CoreResult<Option<MediaRef>> {
        TidalResource::parse(input)
            .map(|resource| resource.map(TidalResource::media_ref).transpose())
            .and_then(|resource| resource)
            .map_err(TidalError::into_core)
    }

    async fn resolve(&self, reference: &MediaRef) -> CoreResult<CatalogEntity> {
        match reference.kind {
            riptide_core::ResourceKind::Track => self
                .client
                .track(reference.id.as_str())
                .await
                .map(CatalogEntity::Track)
                .map_err(TidalError::into_core),
            riptide_core::ResourceKind::Video => self
                .client
                .video(reference.id.as_str())
                .await
                .map(CatalogEntity::Video)
                .map_err(TidalError::into_core),
            riptide_core::ResourceKind::Album => self
                .client
                .album(reference.id.as_str())
                .await
                .map(CatalogEntity::Album)
                .map_err(TidalError::into_core),
            riptide_core::ResourceKind::Playlist => self
                .client
                .playlist(reference.id.as_str())
                .await
                .map(CatalogEntity::Playlist)
                .map_err(TidalError::into_core),
            kind => Err(CoreError::UnsupportedResourceKind {
                provider: self.id.clone(),
                kind,
                operation: "resolve",
            }),
        }
    }
}

#[async_trait]
impl CollectionProvider for TidalProvider {
    async fn children(&self, _reference: &MediaRef) -> CoreResult<Vec<CatalogEntity>> {
        Err(CoreError::UnsupportedCapability {
            provider: self.id.clone(),
            capability: "collections",
        })
    }
}

#[async_trait]
impl StreamProvider for TidalProvider {
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
impl LyricsProvider for TidalProvider {
    async fn lyrics(&self, reference: &MediaRef) -> CoreResult<Option<Lyrics>> {
        self.client
            .lyrics(reference.id.as_str())
            .await
            .map(Some)
            .map_err(TidalError::into_core)
    }
}

impl Provider for TidalProvider {
    fn id(&self) -> &ProviderId {
        &self.id
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            resolve: true,
            collections: true,
            lyrics: true,
            search: true,
            ..ProviderCapabilities::default()
        }
    }
}
