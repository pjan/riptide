//! TIDAL read-only API client.

use crate::{
    auth::{TidalAuthClient, TidalTokenState, TidalTokenStore},
    cache::{ApiCachePolicy, CacheEntryToWrite, CacheKey, TidalApiCache},
    convert, model, TidalError, TidalResult,
};
use reqwest::{StatusCode, Url};
use riptide_core::{Album, Artist, CatalogEntity, Playlist, Track, Video};
use serde::de::DeserializeOwned;
use std::time::Duration;

const DEFAULT_TTL: Duration = Duration::from_secs(3600);
const DEFAULT_BACKOFF: Duration = Duration::from_secs(2);
const REFRESH_THRESHOLD: Duration = Duration::from_secs(60);

/// Provider-neutral search results preserving a top hit.
#[derive(Debug, Clone, PartialEq)]
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
    token_store: TidalTokenStore,
    cache: TidalApiCache,
}

impl TidalClient {
    /// Creates a client.
    pub fn new(
        api_base_url: Url,
        auth: TidalAuthClient,
        token_store: TidalTokenStore,
        cache: TidalApiCache,
    ) -> Self {
        Self {
            http: reqwest::Client::new(),
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
        convert::track(
            self.get_json(
                &format!("tracks/{id}"),
                &self.country_params().await?,
                ttl(),
            )
            .await?,
        )
    }

    /// Fetches a video.
    pub async fn video(&self, id: &str) -> TidalResult<Video> {
        convert::video(
            self.get_json(
                &format!("videos/{id}"),
                &self.country_params().await?,
                ttl(),
            )
            .await?,
        )
    }

    /// Fetches an album.
    pub async fn album(&self, id: &str) -> TidalResult<Album> {
        convert::album(
            self.get_json(
                &format!("albums/{id}"),
                &self.country_params().await?,
                ttl(),
            )
            .await?,
        )
    }

    /// Fetches a playlist.
    pub async fn playlist(&self, id: &str) -> TidalResult<Playlist> {
        convert::playlist(
            self.get_json(
                &format!("playlists/{id}"),
                &self.country_params().await?,
                ApiCachePolicy::Revalidate,
            )
            .await?,
        )
    }

    /// Fetches lyrics.
    pub async fn lyrics(&self, id: &str) -> TidalResult<riptide_core::Lyrics> {
        convert::lyrics(
            self.get_json(
                &format!("tracks/{id}/lyrics"),
                &self.country_params().await?,
                ttl(),
            )
            .await?,
        )
    }

    /// Searches TIDAL.
    pub async fn search(&self, query: &str) -> TidalResult<SearchResults> {
        let mut params = self.country_params().await?;
        params.push(("query".to_owned(), query.to_owned()));
        let raw: model::TidalSearch = self
            .get_json("search", &params, ApiCachePolicy::NoCache)
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
                                    riptide_core::ProviderId::new("tidal").map_err(|error| {
                                        TidalError::InvalidResponse {
                                            field: "provider".to_owned(),
                                            message: error.to_string(),
                                        }
                                    })?,
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
        params: &[(String, String)],
        policy: ApiCachePolicy,
    ) -> TidalResult<T> {
        let token = self.token_for_request(endpoint).await?;
        let country_code = params
            .iter()
            .find(|(key, _)| key == "countryCode")
            .map(|(_, value)| value.as_str());
        let key = CacheKey::new("GET", endpoint, country_code, params);
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

        let value = self.fetch_value(endpoint, params, &token, true).await?;
        match policy {
            ApiCachePolicy::NoCache => {}
            ApiCachePolicy::Ttl(duration) => {
                self.cache
                    .put(CacheEntryToWrite {
                        key,
                        value: value.clone(),
                        expires_at: time::OffsetDateTime::now_utc()
                            + time::Duration::seconds(
                                i64::try_from(duration.as_secs()).unwrap_or(3600),
                            ),
                    })
                    .await?;
            }
            ApiCachePolicy::Revalidate => {
                self.cache
                    .put(CacheEntryToWrite {
                        key,
                        value: value.clone(),
                        expires_at: time::OffsetDateTime::now_utc() + time::Duration::seconds(3600),
                    })
                    .await?;
            }
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

    async fn country_params(&self) -> TidalResult<Vec<(String, String)>> {
        let Some(token) = self.token_store.load().await? else {
            return Err(TidalError::MissingCredentials);
        };
        Ok(vec![(
            "countryCode".to_owned(),
            token.country_code.as_str().to_owned(),
        )])
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
                .map(Duration::from_secs)
                .unwrap_or(DEFAULT_BACKOFF);
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

fn ttl() -> ApiCachePolicy {
    ApiCachePolicy::Ttl(DEFAULT_TTL)
}

async fn response_json(
    endpoint: &str,
    response: reqwest::Response,
) -> TidalResult<serde_json::Value> {
    let status = response.status();
    if !status.is_success() {
        let message = response.text().await.unwrap_or_default();
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
