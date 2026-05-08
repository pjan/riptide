//! TIDAL provider implementation for riptide-rs.

use riptide_core::ProviderId;
use std::sync::LazyLock;

pub mod auth;
pub mod cache;
pub mod client;
pub(crate) mod convert;
pub mod cover;
pub mod error;
mod http;
pub(crate) mod model;
pub mod provider;
pub mod resource;
pub mod stream;

pub use auth::{
    AccessToken, CountryCode, DeviceAuthorization, DeviceCode, FileTokenStore, PollingInterval,
    RefreshToken, TidalAuthClient, TidalCredentials, TidalTokenState, TidalUserId, TokenExpiry,
    TokenStore,
};
pub use cache::{ApiCache, ApiCachePolicy, CacheEntryToWrite, CacheKey, RedbApiCache};
pub use client::{SearchResults, TidalClient};
pub use cover::{cover_url, CoverArtSize};
pub use error::{TidalError, TidalResult};
pub use provider::TidalProvider;
pub use resource::TidalResource;
pub use stream::{audio_quality_request_label, video_quality_request_label};

/// HTTP client type used by TIDAL API and auth clients.
pub type HttpClient = reqwest::Client;

/// Builds an HTTP client with optional environment-provided PEM roots.
pub fn http_client() -> TidalResult<HttpClient> {
    http::client()
}

/// Canonical TIDAL provider identifier.
pub(crate) static TIDAL_PROVIDER_ID: LazyLock<ProviderId> = LazyLock::new(|| {
    ProviderId::new("tidal").unwrap_or_else(|_| unreachable!("literal provider ID is valid"))
});
