//! TIDAL provider errors.

use riptide_core::{CoreError, ProviderErrorKind, ProviderId};
use thiserror::Error;

/// Convenient result alias for TIDAL operations.
pub type TidalResult<T> = Result<T, TidalError>;

/// Errors produced by the TIDAL provider.
#[derive(Debug, Error)]
pub enum TidalError {
    /// Credentials were not available.
    #[error(
        "missing TIDAL credentials; set RIPTIDE_TIDAL_CLIENT_ID and RIPTIDE_TIDAL_CLIENT_SECRET"
    )]
    MissingCredentials,

    /// Invalid typed value.
    #[error("invalid {field}: {message}")]
    InvalidValue {
        /// Field name.
        field: &'static str,
        /// Explanation.
        message: String,
    },

    /// HTTP request failed.
    #[error("TIDAL HTTP error during {operation}: {source}")]
    Http {
        /// Operation context.
        operation: &'static str,
        /// Source error.
        #[source]
        source: reqwest::Error,
    },

    /// HTTP client construction failed.
    #[error("failed to build TIDAL HTTP client: {message}")]
    HttpClient {
        /// Explanation.
        message: String,
    },

    /// TIDAL returned an unsuccessful status.
    #[error("TIDAL API error during {operation}: status {status}: {message}")]
    Api {
        /// Operation context.
        operation: String,
        /// HTTP status.
        status: u16,
        /// Provider message.
        message: String,
    },

    /// TIDAL rate limit persisted after retry.
    #[error("TIDAL rate limit during {operation}")]
    RateLimited {
        /// Operation context.
        operation: String,
    },

    /// Token refresh failed while handling an API request.
    #[error("failed to refresh token for {operation}: {source}")]
    Refresh {
        /// Operation context.
        operation: String,
        /// Source error.
        #[source]
        source: Box<TidalError>,
    },

    /// Cache file could not be opened or used.
    #[error("TIDAL cache unavailable at {path}: {message}")]
    CacheUnavailable {
        /// Cache path.
        path: String,
        /// Explanation.
        message: String,
    },

    /// Cache operation failed.
    #[error("TIDAL cache error: {message}")]
    Cache {
        /// Explanation.
        message: String,
    },

    /// Token store operation failed.
    #[error("TIDAL token store error at {path}: {message}")]
    TokenStore {
        /// Token file path.
        path: String,
        /// Explanation.
        message: String,
    },

    /// TIDAL returned data that could not be converted.
    #[error("invalid TIDAL response for {field}: {message}")]
    InvalidResponse {
        /// Field or endpoint.
        field: String,
        /// Explanation.
        message: String,
    },

    /// Capability is not supported in this slice.
    #[error("unsupported TIDAL capability: {capability}")]
    UnsupportedCapability {
        /// Capability.
        capability: &'static str,
    },
}

impl TidalError {
    /// Maps this provider error into a provider-neutral core error.
    pub fn into_core(self) -> CoreError {
        let kind = match self {
            Self::MissingCredentials | Self::Refresh { .. } => ProviderErrorKind::Authentication,
            Self::Api {
                status: 401 | 403, ..
            } => ProviderErrorKind::Authorization,
            Self::Api { status: 404, .. } => ProviderErrorKind::NotFound,
            Self::RateLimited { .. } => ProviderErrorKind::RateLimited,
            Self::InvalidResponse { .. } | Self::InvalidValue { .. } => {
                ProviderErrorKind::InvalidResponse
            }
            Self::UnsupportedCapability { .. } => ProviderErrorKind::UnsupportedCapability,
            Self::Http { .. }
            | Self::HttpClient { .. }
            | Self::CacheUnavailable { .. }
            | Self::Cache { .. }
            | Self::TokenStore { .. }
            | Self::Api { .. } => ProviderErrorKind::Unavailable,
        };
        CoreError::Provider {
            provider: ProviderId::new("tidal").unwrap_or_else(|_| unreachable_provider_id()),
            kind,
            source: Box::new(self),
        }
    }
}

fn unreachable_provider_id() -> ProviderId {
    match ProviderId::new("tidal") {
        Ok(id) => id,
        Err(_) => unreachable!("literal provider ID is valid"),
    }
}
