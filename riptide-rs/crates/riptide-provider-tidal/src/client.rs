//! TIDAL read-only API client.

use crate::{
    auth::{TidalAuthClient, TidalTokenState, TokenStore},
    cache::{ApiCache, ApiCachePolicy, CacheEntryToWrite, CacheKey},
    convert, model, stream, TidalError, TidalResult, TIDAL_PROVIDER_ID,
};
use reqwest::{StatusCode, Url};
use riptide_core::{
    Album, Artist, AudioQualityRequest, CatalogEntity, MediaRef, Playlist, ResourceId,
    ResourceKind, StreamVariant, Track, Video,
};
use serde::de::DeserializeOwned;
use std::time::Duration;
use std::{collections::HashSet, sync::Arc};

const DEFAULT_TTL: Duration = Duration::from_secs(3600);
const DEFAULT_BACKOFF: Duration = Duration::from_secs(2);
const REFRESH_THRESHOLD: Duration = Duration::from_secs(60);

/// Provider-neutral search results preserving a top hit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResults {
    /// Top hit, when provided by TIDAL.
    pub top_hit: Option<CatalogEntity>,
    /// Track results.
    pub tracks: Vec<Track>,
    /// Album results.
    pub albums: Vec<Album>,
    /// Playlist results.
    pub playlists: Vec<Playlist>,
    /// Artist results.
    pub artists: Vec<Artist>,
    /// Video results.
    pub videos: Vec<Video>,
}

/// TIDAL artist album endpoint selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtistAlbumSelection {
    /// Fetch album releases only.
    AlbumsOnly,
    /// Fetch singles and EPs only.
    SinglesAndEpsOnly,
    /// Fetch both album and singles/EP endpoints.
    AlbumsAndSingles,
}

/// TIDAL read-only API client.
#[derive(Debug, Clone)]
pub struct TidalClient {
    http: reqwest::Client,
    api_base_url: Url,
    auth: TidalAuthClient,
    token_store: Arc<dyn TokenStore>,
    cache: Arc<dyn ApiCache>,
}

impl TidalClient {
    /// Creates a client.
    pub fn new(
        http: reqwest::Client,
        api_base_url: Url,
        auth: TidalAuthClient,
        token_store: Arc<dyn TokenStore>,
        cache: Arc<dyn ApiCache>,
    ) -> Self {
        Self {
            http,
            api_base_url,
            auth,
            token_store,
            cache,
        }
    }

    /// Fetches session data.
    pub async fn session(&self) -> TidalResult<model::SessionResponse> {
        self.get_json("sessions", &[], ApiCachePolicy::NoCache)
            .await
    }

    /// Fetches a track.
    pub async fn track(&self, id: &ResourceId) -> TidalResult<Track> {
        convert::track(
            self.get_json(&format!("tracks/{}", id.as_str()), &[], ttl())
                .await?,
        )
    }

    /// Fetches a video.
    pub async fn video(&self, id: &ResourceId) -> TidalResult<Video> {
        convert::video(
            self.get_json(&format!("videos/{}", id.as_str()), &[], ttl())
                .await?,
        )
    }

    /// Fetches an album.
    pub async fn album(&self, id: &ResourceId) -> TidalResult<Album> {
        convert::album(
            self.get_json(&format!("albums/{}", id.as_str()), &[], ttl())
                .await?,
        )
    }

    /// Fetches a playlist.
    pub async fn playlist(&self, id: &ResourceId) -> TidalResult<Playlist> {
        convert::playlist(
            self.get_json(
                &format!("playlists/{}", id.as_str()),
                &[],
                ApiCachePolicy::Revalidate,
            )
            .await?,
        )
    }

    /// Fetches an artist.
    pub async fn artist(&self, id: &ResourceId) -> TidalResult<Artist> {
        convert::artist(
            self.get_json(&format!("artists/{}", id.as_str()), &[], ttl())
                .await?,
        )
    }

    /// Fetches lyrics.
    pub async fn lyrics(&self, id: &ResourceId) -> TidalResult<riptide_core::Lyrics> {
        convert::lyrics(
            self.get_json(&format!("tracks/{}/lyrics", id.as_str()), &[], ttl())
                .await?,
        )
    }

    /// Fetches a track stream variant.
    pub async fn track_stream(
        &self,
        reference: &MediaRef,
        quality: AudioQualityRequest,
    ) -> TidalResult<StreamVariant> {
        let params = [
            ("audioquality", stream::audio_quality_request_label(quality)),
            ("playbackmode", "STREAM"),
            ("assetpresentation", "FULL"),
        ];
        let raw: model::TidalStreamResponse = self
            .get_json(
                &format!("tracks/{}/playbackinfopostpaywall", reference.id.as_str()),
                &params,
                ApiCachePolicy::NoCache,
            )
            .await?;
        stream::decode_track_manifest(&raw, reference, quality)
    }

    /// Fetches ordered children for album and playlist resources.
    pub async fn children(&self, reference: &MediaRef) -> TidalResult<Vec<CatalogEntity>> {
        match reference.kind {
            ResourceKind::Album => self.album_items(&reference.id).await,
            ResourceKind::Playlist => self.playlist_items(&reference.id).await,
            ResourceKind::Artist => Ok(self
                .artist_albums_all(&reference.id)
                .await?
                .into_iter()
                .map(CatalogEntity::Album)
                .collect()),
            ResourceKind::Mix => self.mix_items(&reference.id).await,
            kind => Err(TidalError::UnsupportedCapability {
                capability: match kind {
                    ResourceKind::Track => "track children",
                    ResourceKind::Video => "video children",
                    ResourceKind::Album
                    | ResourceKind::Playlist
                    | ResourceKind::Artist
                    | ResourceKind::Mix => unreachable!(),
                    _ => "unknown resource children",
                },
            }),
        }
    }

    /// Paginates artist albums.
    pub async fn artist_albums(&self, id: &ResourceId) -> TidalResult<Vec<Album>> {
        self.paged_albums(&format!("artists/{}/albums", id.as_str()), "ALBUMS")
            .await
    }

    /// Paginates artist singles and EPs.
    pub async fn artist_singles(&self, id: &ResourceId) -> TidalResult<Vec<Album>> {
        self.paged_albums(&format!("artists/{}/albums", id.as_str()), "EPSANDSINGLES")
            .await
    }

    pub(crate) async fn artist_albums_all(&self, id: &ResourceId) -> TidalResult<Vec<Album>> {
        let albums = self.artist_albums(id).await?;
        let singles = self.artist_singles(id).await?;
        Ok(deduplicate_albums_by_id(
            albums.into_iter().chain(singles),
        ))
    }

    pub(crate) async fn artist_albums_for_selection(
        &self,
        id: &ResourceId,
        selection: ArtistAlbumSelection,
    ) -> TidalResult<Vec<Album>> {
        match selection {
            ArtistAlbumSelection::AlbumsOnly => self.artist_albums(id).await,
            ArtistAlbumSelection::SinglesAndEpsOnly => self.artist_singles(id).await,
            ArtistAlbumSelection::AlbumsAndSingles => self.artist_albums_all(id).await,
        }
    }

    /// Paginates mix items.
    pub async fn mix_items(&self, id: &ResourceId) -> TidalResult<Vec<CatalogEntity>> {
        self.paged_items(&format!("mixes/{}/items", id.as_str()), 20)
            .await
    }

    async fn album_items(&self, id: &ResourceId) -> TidalResult<Vec<CatalogEntity>> {
        self.paged_items(&format!("albums/{}/items", id.as_str()), 20)
            .await
    }

    async fn playlist_items(&self, id: &ResourceId) -> TidalResult<Vec<CatalogEntity>> {
        self.paged_items(&format!("playlists/{}/items", id.as_str()), 20)
            .await
    }

    async fn paged_albums(&self, endpoint: &str, filter: &str) -> TidalResult<Vec<Album>> {
        let mut offset = 0;
        let limit = 20;
        let mut output = Vec::new();
        loop {
            let limit_param = limit.to_string();
            let offset_param = offset.to_string();
            let params = [
                ("filter", filter),
                ("limit", limit_param.as_str()),
                ("offset", offset_param.as_str()),
            ];
            let page: model::Items<model::TidalAlbum> =
                self.get_json(endpoint, &params, ttl()).await?;
            let count = page.items.len();
            output.extend(
                page.items
                    .into_iter()
                    .map(convert::album)
                    .collect::<TidalResult<Vec<_>>>()?,
            );
            if count < usize::try_from(limit).unwrap_or(usize::MAX) {
                break;
            }
            offset += limit;
        }
        Ok(output)
    }

    async fn paged_items(&self, endpoint: &str, limit: u32) -> TidalResult<Vec<CatalogEntity>> {
        let mut offset = 0;
        let mut output = Vec::new();
        loop {
            let limit_param = limit.to_string();
            let offset_param = offset.to_string();
            let params = [
                ("limit", limit_param.as_str()),
                ("offset", offset_param.as_str()),
            ];
            let page: model::Items<serde_json::Value> =
                self.get_json(endpoint, &params, ttl()).await?;
            let count = page.items.len();
            for value in page.items {
                output.push(convert_catalog_item(&value)?);
            }
            if count < usize::try_from(limit).unwrap_or(usize::MAX) {
                break;
            }
            offset += limit;
        }
        Ok(output)
    }

    /// Searches TIDAL.
    pub async fn search(&self, query: &str) -> TidalResult<SearchResults> {
        let extra_params = [("query", query)];
        let raw: model::TidalSearch = self
            .get_json("search", &extra_params, ApiCachePolicy::NoCache)
            .await?;
        let top_hit = match raw.top_hit {
            Some(hit) => convert::top_hit(&hit.kind, hit.value)?,
            None => None,
        };
        Ok(SearchResults {
            top_hit,
            tracks: raw
                .tracks
                .items
                .into_iter()
                .map(convert::track)
                .collect::<TidalResult<_>>()?,
            albums: raw
                .albums
                .items
                .into_iter()
                .map(convert::album)
                .collect::<TidalResult<_>>()?,
            playlists: raw
                .playlists
                .items
                .into_iter()
                .map(convert::playlist)
                .collect::<TidalResult<_>>()?,
            artists: raw
                .artists
                .items
                .into_iter()
                .map(|artist| {
                    Ok(Artist {
                        id: artist
                            .id
                            .map(|id| {
                                Ok(riptide_core::MediaRef::new(
                                    TIDAL_PROVIDER_ID.clone(),
                                    riptide_core::ResourceKind::Artist,
                                    riptide_core::ResourceId::new(id.to_string()).map_err(
                                        |error| TidalError::InvalidResponse {
                                            field: "artist.id".to_owned(),
                                            message: error.to_string(),
                                        },
                                    )?,
                                ))
                            })
                            .transpose()?,
                        name: artist.name,
                    })
                })
                .collect::<TidalResult<_>>()?,
            videos: raw
                .videos
                .items
                .into_iter()
                .map(convert::video)
                .collect::<TidalResult<_>>()?,
        })
    }

    async fn get_json<T: DeserializeOwned>(
        &self,
        endpoint: &str,
        extra_params: &[(&str, &str)],
        policy: ApiCachePolicy,
    ) -> TidalResult<T> {
        let token = self.token_for_request(endpoint).await?;
        let country_code = token.country_code.as_str();
        let mut params = vec![("countryCode", country_code)];
        params.extend_from_slice(extra_params);
        let key = CacheKey::new("GET", endpoint, Some(country_code), &params);
        if let ApiCachePolicy::Ttl(_) = policy {
            if let Some(value) = self.cache.get(&key).await? {
                return serde_json::from_value(value).map_err(|error| {
                    TidalError::InvalidResponse {
                        field: endpoint.to_owned(),
                        message: error.to_string(),
                    }
                });
            }
        }

        let value = self.fetch_value(endpoint, &params, &token, true).await?;
        let cache_ttl = match policy {
            ApiCachePolicy::NoCache => None,
            ApiCachePolicy::Ttl(duration) => Some(duration),
            ApiCachePolicy::Revalidate => Some(DEFAULT_TTL),
        };
        if let Some(ttl) = cache_ttl {
            self.cache
                .put(CacheEntryToWrite {
                    key,
                    value: value.clone(),
                    expires_at: time::OffsetDateTime::now_utc()
                        + time::Duration::seconds(i64::try_from(ttl.as_secs()).unwrap_or(3600)),
                })
                .await?;
        }
        serde_json::from_value(value).map_err(|error| TidalError::InvalidResponse {
            field: endpoint.to_owned(),
            message: error.to_string(),
        })
    }

    async fn token_for_request(&self, operation: &str) -> TidalResult<TidalTokenState> {
        let Some(token) = self.token_store.load().await? else {
            return Err(TidalError::MissingCredentials);
        };
        if token
            .expires_at
            .expires_within(time::OffsetDateTime::now_utc(), REFRESH_THRESHOLD)
        {
            return self
                .auth
                .refresh(&token.refresh_token)
                .await
                .map_err(|source| TidalError::Refresh {
                    operation: operation.to_owned(),
                    source: Box::new(source),
                });
        }
        Ok(token)
    }

    async fn fetch_value(
        &self,
        endpoint: &str,
        params: &[(&str, &str)],
        token: &TidalTokenState,
        allow_refresh: bool,
    ) -> TidalResult<serde_json::Value> {
        let url = self
            .api_base_url
            .join(endpoint)
            .map_err(|error| TidalError::InvalidValue {
                field: "api_base_url",
                message: error.to_string(),
            })?;
        let response = self
            .http
            .get(url)
            .query(params)
            .bearer_auth(token.access_token.expose_secret())
            .send()
            .await
            .map_err(|source| TidalError::Http {
                operation: "api get",
                source,
            })?;
        if response.status() == StatusCode::UNAUTHORIZED && allow_refresh {
            let refreshed = self
                .auth
                .refresh(&token.refresh_token)
                .await
                .map_err(|source| TidalError::Refresh {
                    operation: endpoint.to_owned(),
                    source: Box::new(source),
                })?;
            self.token_store.save(&refreshed).await?;
            return Box::pin(self.fetch_value(endpoint, params, &refreshed, false)).await;
        }
        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            let retry_after = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok())
                .map_or(DEFAULT_BACKOFF, Duration::from_secs);
            tokio::time::sleep(retry_after).await;
            let retry = self
                .http
                .get(self.api_base_url.join(endpoint).map_err(|error| {
                    TidalError::InvalidValue {
                        field: "api_base_url",
                        message: error.to_string(),
                    }
                })?)
                .query(params)
                .bearer_auth(token.access_token.expose_secret())
                .send()
                .await
                .map_err(|source| TidalError::Http {
                    operation: "api retry",
                    source,
                })?;
            if retry.status() == StatusCode::TOO_MANY_REQUESTS {
                return Err(TidalError::RateLimited {
                    operation: endpoint.to_owned(),
                });
            }
            return response_json(endpoint, retry).await;
        }
        response_json(endpoint, response).await
    }
}

fn convert_catalog_item(value: &serde_json::Value) -> TidalResult<CatalogEntity> {
    let item = value.get("item").cloned().unwrap_or_else(|| value.clone());
    let kind = value
        .get("type")
        .or_else(|| item.get("type"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    // TIDAL item lists expose the catalog kind in the wrapper or item `type` field.
    // Do not infer video-ness from media-specific fields; those can drift by endpoint.
    if kind.eq_ignore_ascii_case("video") {
        return serde_json::from_value::<model::TidalVideo>(item)
            .map_err(|error| TidalError::InvalidResponse {
                field: "items.video".to_owned(),
                message: error.to_string(),
            })
            .and_then(convert::video)
            .map(CatalogEntity::Video);
    }
    serde_json::from_value::<model::TidalTrack>(item)
        .map_err(|error| TidalError::InvalidResponse {
            field: "items.track".to_owned(),
            message: error.to_string(),
        })
        .and_then(convert::track)
        .map(CatalogEntity::Track)
}

fn deduplicate_albums_by_id(albums: impl IntoIterator<Item = Album>) -> Vec<Album> {
    let mut seen = HashSet::new();
    let mut deduplicated = Vec::new();
    for album in albums {
        if seen.insert(album.id.clone()) {
            deduplicated.push(album);
        }
    }
    deduplicated
}

const fn ttl() -> ApiCachePolicy {
    ApiCachePolicy::Ttl(DEFAULT_TTL)
}

async fn response_json(
    endpoint: &str,
    response: reqwest::Response,
) -> TidalResult<serde_json::Value> {
    let status = response.status();
    if !status.is_success() {
        let message = response
            .text()
            .await
            .unwrap_or_else(|e| format!("<body read failed: {e}>"));
        return Err(TidalError::Api {
            operation: endpoint.to_owned(),
            status: status.as_u16(),
            message,
        });
    }
    response.json().await.map_err(|source| TidalError::Http {
        operation: "api json",
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use riptide_core::{ArtistCredit, ResourceId};

    #[test]
    fn album_deduplication_preserves_first_seen_order() -> TidalResult<()> {
        let albums = deduplicate_albums_by_id(vec![
            album("1", "First")?,
            album("2", "Second")?,
            album("1", "Duplicate")?,
        ]);

        assert_eq!(albums.len(), 2);
        assert_eq!(albums[0].title, "First");
        assert_eq!(albums[1].title, "Second");
        Ok(())
    }

    fn album(id: &str, title: &str) -> TidalResult<Album> {
        Ok(Album {
            id: MediaRef::new(
                TIDAL_PROVIDER_ID.clone(),
                ResourceKind::Album,
                ResourceId::new(id).map_err(|error| TidalError::InvalidValue {
                    field: "id",
                    message: error.to_string(),
                })?,
            ),
            title: title.to_owned(),
            artists: Vec::<ArtistCredit>::new(),
            release_date: None,
            track_count: None,
            explicit: None,
            release_type: None,
            hires_lossless: false,
            cover: None,
        })
    }

    #[test]
    fn catalog_item_uses_wrapper_type_for_video_detection() -> TidalResult<()> {
        let value = serde_json::json!({
            "type": "video",
            "item": {
                "id": 42,
                "title": "Video Title",
                "duration": 120,
                "quality": "HIGH"
            }
        });

        let entity = convert_catalog_item(&value)?;

        assert!(matches!(entity, CatalogEntity::Video(_)));
        Ok(())
    }

    #[test]
    fn catalog_item_uses_item_type_for_video_detection() -> TidalResult<()> {
        let value = serde_json::json!({
            "item": {
                "type": "video",
                "id": 42,
                "title": "Video Title",
                "duration": 120,
                "quality": "HIGH"
            }
        });

        let entity = convert_catalog_item(&value)?;

        assert!(matches!(entity, CatalogEntity::Video(_)));
        Ok(())
    }

    #[test]
    fn catalog_item_quality_field_does_not_imply_video() -> TidalResult<()> {
        let value = serde_json::json!({
            "item": {
                "id": 42,
                "title": "Track Title",
                "duration": 120,
                "quality": "HIGH"
            }
        });

        let entity = convert_catalog_item(&value)?;

        assert!(matches!(entity, CatalogEntity::Track(_)));
        Ok(())
    }
}
