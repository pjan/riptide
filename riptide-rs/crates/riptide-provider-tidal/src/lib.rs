//! TIDAL provider implementation for riptide-rs.

pub mod auth;
pub mod cache;
pub mod client;
pub mod convert;
pub mod error;
pub mod model;
pub mod provider;
pub mod resource;

pub use auth::{
    AccessToken, CountryCode, DeviceAuthorization, DeviceCode, PollingInterval, RefreshToken,
    TidalAuthClient, TidalCredentials, TidalTokenState, TidalTokenStore, TidalUserId, TokenExpiry,
};
pub use cache::{ApiCachePolicy, CacheEntryToWrite, CacheKey, TidalApiCache};
pub use client::{SearchResults, TidalClient};
pub use error::{TidalError, TidalResult};
pub use provider::{TidalProvider, TidalProviderOptions};
pub use resource::TidalResource;
