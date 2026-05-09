//! Provider trait implementation for TIDAL.

use crate::{
    resource::TidalResource, ArtistAlbumSelection, TidalClient, TidalError, TidalResult,
    TIDAL_PROVIDER_ID,
};
use async_trait::async_trait;
use riptide_core::{
    Album, CatalogEntity, CollectionProvider, CoreError, CoreResult, Lyrics, LyricsProvider,
    MediaRef, Provider, ProviderCapabilities, ProviderId, QualityRequest, Resolver, StreamProvider,
    StreamVariant,
};

/// TIDAL provider.
#[derive(Debug, Clone)]
pub struct TidalProvider {
    id: ProviderId,
    client: TidalClient,
}

impl TidalProvider {
    /// Creates a provider wrapping a TIDAL API client.
    pub fn new(client: TidalClient) -> Self {
        Self {
            id: TIDAL_PROVIDER_ID.clone(),
            client,
        }
    }

    /// Fetches artist albums needed for a TIDAL-specific download selection.
    pub async fn artist_albums_for_selection(
        &self,
        id: &riptide_core::ResourceId,
        selection: ArtistAlbumSelection,
    ) -> TidalResult<Vec<Album>> {
        self.client.artist_albums_for_selection(id, selection).await
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
                .track(&reference.id)
                .await
                .map(CatalogEntity::Track)
                .map_err(TidalError::into_core),
            riptide_core::ResourceKind::Video => self
                .client
                .video(&reference.id)
                .await
                .map(CatalogEntity::Video)
                .map_err(TidalError::into_core),
            riptide_core::ResourceKind::Album => self
                .client
                .album(&reference.id)
                .await
                .map(CatalogEntity::Album)
                .map_err(TidalError::into_core),
            riptide_core::ResourceKind::Playlist => self
                .client
                .playlist(&reference.id)
                .await
                .map(CatalogEntity::Playlist)
                .map_err(TidalError::into_core),
            riptide_core::ResourceKind::Artist => self
                .client
                .artist(&reference.id)
                .await
                .map(CatalogEntity::Artist)
                .map_err(TidalError::into_core),
            riptide_core::ResourceKind::Mix => {
                Ok(CatalogEntity::Collection(riptide_core::Collection {
                    id: reference.clone(),
                    kind: riptide_core::CollectionKind::Mix,
                    title: None,
                    owner: None,
                }))
            }
            _ => Err(CoreError::UnsupportedResourceKind {
                provider: self.id.clone(),
                kind: reference.kind,
                operation: "resolve",
            }),
        }
    }
}

#[async_trait]
impl CollectionProvider for TidalProvider {
    async fn children(&self, reference: &MediaRef) -> CoreResult<Vec<CatalogEntity>> {
        self.client
            .children(reference)
            .await
            .map_err(TidalError::into_core)
    }
}

#[async_trait]
impl StreamProvider for TidalProvider {
    async fn streams(
        &self,
        reference: &MediaRef,
        requested: QualityRequest,
    ) -> CoreResult<Vec<StreamVariant>> {
        match (reference.kind, requested) {
            (riptide_core::ResourceKind::Track, QualityRequest::Audio(quality)) => self
                .client
                .track_stream(reference, quality)
                .await
                .map(|stream| vec![stream])
                .map_err(TidalError::into_core),
            (riptide_core::ResourceKind::Video, QualityRequest::Video(quality)) => self
                .client
                .video_stream(reference, quality)
                .await
                .map(|stream| vec![stream])
                .map_err(TidalError::into_core),
            (_, _) => Err(CoreError::UnsupportedResourceKind {
                provider: self.id.clone(),
                kind: reference.kind,
                operation: "streams",
            }),
        }
    }
}

#[async_trait]
impl LyricsProvider for TidalProvider {
    async fn lyrics(&self, reference: &MediaRef) -> CoreResult<Option<Lyrics>> {
        self.client
            .lyrics(&reference.id)
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
        ProviderCapabilities::RESOLVE
            | ProviderCapabilities::COLLECTIONS
            | ProviderCapabilities::STREAMS
            | ProviderCapabilities::LYRICS
            | ProviderCapabilities::SEARCH
    }
}
