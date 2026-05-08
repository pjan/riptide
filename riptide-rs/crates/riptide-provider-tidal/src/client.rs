//! TIDAL read-only API client.

use crate::{
    auth::{TidalAuthClient, TidalTokenState, TokenStore},
    cache::{ApiCache, ApiCachePolicy, CacheEntryToWrite, CacheKey},
    convert, model, stream, TidalError, TidalResult, TIDAL_PROVIDER_ID,
};
use reqwest::{StatusCode, Url};
use riptide_core::{
    Album, Artist, AudioQualityRequest, CatalogEntity, MediaRef, Playlist, ResourceKind,
    StreamVariant, Track, Video, VideoQualityRequest,
};
use serde::de::DeserializeOwned;
use std::sync::Arc;
use std::time::Duration;

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
    pub async fn track(&self, id: &str) -> TidalResult<Track> {
        convert::track(self.get_json(&format!("tracks/{id}"), &[], ttl()).await?)
    }

    /// Fetches a video.
    pub async fn video(&self, id: &str) -> TidalResult<Video> {
        convert::video(self.get_json(&format!("videos/{id}"), &[], ttl()).await?)
    }

    /// Fetches an album.
    pub async fn album(&self, id: &str) -> TidalResult<Album> {
        convert::album(self.get_json(&format!("albums/{id}"), &[], ttl()).await?)
    }

    /// Fetches a playlist.
    pub async fn playlist(&self, id: &str) -> TidalResult<Playlist> {
        convert::playlist(
            self.get_json(&format!("playlists/{id}"), &[], ApiCachePolicy::Revalidate)
                .await?,
        )
    }

    /// Fetches lyrics.
    pub async fn lyrics(&self, id: &str) -> TidalResult<riptide_core::Lyrics> {
        convert::lyrics(
            self.get_json(&format!("tracks/{id}/lyrics"), &[], ttl())
                .await?,
        )
    }

    /// Fetches a track stream variant.
    pub async fn track_stream(
        &self,
        reference: &MediaRef,
        quality: AudioQualityRequest,
    ) -> TidalResult<StreamVariant> {
        let params = vec![(
            "soundQuality".to_owned(),
            stream::audio_quality_request_label(quality).to_owned(),
        )];
        let raw: model::TidalStreamResponse = self
            .get_json(
                &format!("tracks/{}/streamUrl", reference.id.as_str()),
                &params,
                ApiCachePolicy::NoCache,
            )
            .await?;
        stream::decode_track_manifest(&raw, reference, quality)
    }

    /// Fetches a video stream variant.
    pub async fn video_stream(
        &self,
        reference: &MediaRef,
        quality: VideoQualityRequest,
    ) -> TidalResult<StreamVariant> {
        let params = vec![(
            "videoQuality".to_owned(),
            stream::video_quality_request_label(quality).to_owned(),
        )];
        let raw: model::TidalVideoStreamResponse = self
            .get_json(
                &format!("videos/{}/streamUrl", reference.id.as_str()),
                &params,
                ApiCachePolicy::NoCache,
            )
            .await?;
        stream::decode_video_manifest(&raw, reference)
    }

    /// Fetches ordered children for album and playlist resources.
    pub async fn children(&self, reference: &MediaRef) -> TidalResult<Vec<CatalogEntity>> {
        match reference.kind {
            ResourceKind::Album => self.album_items(reference.id.as_str()).await,
            ResourceKind::Playlist => self.playlist_items(reference.id.as_str()).await,
            kind => Err(TidalError::UnsupportedCapability {
                capability: match kind {
                    ResourceKind::Track => "track children",
                    ResourceKind::Video => "video children",
                    ResourceKind::Artist => "artist children",
                    ResourceKind::Mix => "mix children",
                    ResourceKind::Album | ResourceKind::Playlist => unreachable!(),
                },
            }),
        }
    }

    async fn album_items(&self, id: &str) -> TidalResult<Vec<CatalogEntity>> {
        self.paged_items(&format!("albums/{id}/items"), 20).await
    }

    async fn playlist_items(&self, id: &str) -> TidalResult<Vec<CatalogEntity>> {
        self.paged_items(&format!("playlists/{id}/items"), 20).await
    }

    async fn paged_items(&self, endpoint: &str, limit: u32) -> TidalResult<Vec<CatalogEntity>> {
        let mut offset = 0;
        let mut output = Vec::new();
        loop {
            let params = vec![
                ("limit".to_owned(), limit.to_string()),
                ("offset".to_owned(), offset.to_string()),
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
        let extra_params = vec![("query".to_owned(), query.to_owned())];
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
        extra_params: &[(String, String)],
        policy: ApiCachePolicy,
    ) -> TidalResult<T> {
        let token = self.token_for_request(endpoint).await?;
        let country_code = token.country_code.as_str().to_owned();
        let mut params = vec![("countryCode".to_owned(), country_code.clone())];
        params.extend_from_slice(extra_params);
        let key = CacheKey::new("GET", endpoint, Some(&country_code), &params);
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
        params: &[(String, String)],
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
    if kind.eq_ignore_ascii_case("video") || item.get("quality").is_some() {
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
