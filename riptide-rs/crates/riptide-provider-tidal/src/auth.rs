//! TIDAL OAuth device-flow authentication and token storage.

use crate::{TidalError, TidalResult};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::{
    env, fmt, fs,
    path::{Path, PathBuf},
    time::Duration,
};

const SCOPE: &str = "r_usr+w_usr+w_sub";
const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";
const DEFAULT_CREDENTIALS_B64: &str =
    "ZlgySnhkbW50WldLMGl4VDsxTm45QWZEQWp4cmdKRkpiS05XTGVBeUtHVkdtSU51WFBQTEhWWEF2eEFnPQ==";

macro_rules! secret_string {
    ($name:ident) => {
        /// Redacted secret string.
        #[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
        pub struct $name(String);

        impl $name {
            /// Creates a new non-empty secret value.
            pub fn new(value: impl Into<String>) -> TidalResult<Self> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err(TidalError::InvalidValue {
                        field: stringify!($name),
                        message: "cannot be empty".to_owned(),
                    });
                }
                Ok(Self(value))
            }

            /// Exposes the secret for request construction.
            pub fn expose_secret(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_tuple(stringify!($name)).field(&"***").finish()
            }
        }
    };
}

macro_rules! non_empty_string {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        pub struct $name(String);

        impl $name {
            /// Creates a new non-empty value.
            pub fn new(value: impl Into<String>) -> TidalResult<Self> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err(TidalError::InvalidValue {
                        field: stringify!($name),
                        message: "cannot be empty".to_owned(),
                    });
                }
                Ok(Self(value))
            }

            /// Returns the underlying string.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

non_empty_string!(TidalClientId, "TIDAL OAuth client ID.");
secret_string!(TidalClientSecret);
secret_string!(AccessToken);
secret_string!(RefreshToken);
non_empty_string!(DeviceCode, "OAuth device code.");
non_empty_string!(UserCode, "OAuth user code.");
non_empty_string!(TidalUserId, "TIDAL user ID.");

/// TIDAL country code, ISO 3166-1 alpha-2 uppercase.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CountryCode(String);

impl CountryCode {
    /// Creates a country code.
    pub fn new(value: impl Into<String>) -> TidalResult<Self> {
        let value = value.into();
        if value.len() != 2 || !value.bytes().all(|byte| byte.is_ascii_uppercase()) {
            return Err(TidalError::InvalidValue {
                field: "CountryCode",
                message: "expected two uppercase ASCII letters".to_owned(),
            });
        }
        Ok(Self(value))
    }

    /// Returns the country code.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// OAuth polling interval.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PollingInterval(Duration);

impl PollingInterval {
    /// Creates a new polling interval.
    pub fn new(duration: Duration) -> TidalResult<Self> {
        if duration.is_zero() {
            return Err(TidalError::InvalidValue {
                field: "PollingInterval",
                message: "must be greater than zero".to_owned(),
            });
        }
        Ok(Self(duration))
    }

    /// Returns the duration.
    pub const fn as_duration(self) -> Duration {
        self.0
    }

    /// Applies RFC 8628 `slow_down`.
    pub fn slow_down(self) -> Self {
        Self(self.0 + Duration::from_secs(5))
    }
}

/// Access token expiry timestamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenExpiry(time::OffsetDateTime);

impl TokenExpiry {
    /// Creates an expiry from `expires_in` seconds.
    pub fn from_now(expires_in: u64, now: time::OffsetDateTime) -> Self {
        let seconds = i64::try_from(expires_in).unwrap_or(i64::MAX);
        Self(now + time::Duration::seconds(seconds))
    }

    /// Returns true when the expiry falls within the threshold.
    pub fn expires_within(self, now: time::OffsetDateTime, threshold: Duration) -> bool {
        let threshold = time::Duration::seconds(i64::try_from(threshold.as_secs()).unwrap_or(0));
        self.0 <= now + threshold
    }

    /// Returns the expiry timestamp.
    pub const fn as_datetime(self) -> time::OffsetDateTime {
        self.0
    }
}

/// TIDAL OAuth credentials.
#[derive(Debug, Clone)]
pub struct TidalCredentials {
    /// Client ID.
    pub client_id: TidalClientId,
    /// Client secret.
    pub client_secret: TidalClientSecret,
}

impl TidalCredentials {
    /// Loads credentials from supported environment variables.
    pub fn from_env() -> TidalResult<Self> {
        Self::from_sources(
            env::var("RIPTIDE_TIDAL_CLIENT_ID").ok(),
            env::var("RIPTIDE_TIDAL_CLIENT_SECRET").ok(),
        )
    }

    fn from_sources(client_id: Option<String>, client_secret: Option<String>) -> TidalResult<Self> {
        match (client_id, client_secret) {
            (Some(client_id), Some(client_secret)) => Ok(Self {
                client_id: TidalClientId::new(client_id)?,
                client_secret: TidalClientSecret::new(client_secret)?,
            }),
            (None, None) => Self::from_default_fallback(),
            _ => Err(TidalError::InvalidValue {
                field: "TIDAL credentials",
                message:
                    "set both RIPTIDE_TIDAL_CLIENT_ID and RIPTIDE_TIDAL_CLIENT_SECRET, or neither"
                        .to_owned(),
            }),
        }
    }

    fn from_semicolon_pair(field: &'static str, value: &str) -> TidalResult<Self> {
        let Some((client_id, client_secret)) = value.split_once(';') else {
            return Err(TidalError::InvalidValue {
                field,
                message: "expected client_id;client_secret".to_owned(),
            });
        };
        Ok(Self {
            client_id: TidalClientId::new(client_id)?,
            client_secret: TidalClientSecret::new(client_secret)?,
        })
    }

    fn from_default_fallback() -> TidalResult<Self> {
        let bytes =
            STANDARD
                .decode(DEFAULT_CREDENTIALS_B64)
                .map_err(|error| TidalError::InvalidValue {
                    field: "default_credentials",
                    message: error.to_string(),
                })?;
        let decoded = String::from_utf8(bytes).map_err(|error| TidalError::InvalidValue {
            field: "default_credentials",
            message: error.to_string(),
        })?;
        Self::from_semicolon_pair("default_credentials", &decoded)
    }
}

/// Device authorization response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceAuthorization {
    /// Device code for polling.
    pub device_code: DeviceCode,
    /// User code.
    pub user_code: UserCode,
    /// Verification URL.
    pub verification_uri: Url,
    /// Complete verification URL.
    pub verification_uri_complete: Url,
    /// Expiry timestamp.
    pub expires_at: time::OffsetDateTime,
    /// Polling interval.
    pub interval: PollingInterval,
}

/// Persisted TIDAL token state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TidalTokenState {
    /// Access token.
    pub access_token: AccessToken,
    /// Refresh token.
    pub refresh_token: RefreshToken,
    /// Expiry.
    pub expires_at: TokenExpiry,
    /// User ID.
    pub user_id: TidalUserId,
    /// Country code.
    pub country_code: CountryCode,
}

/// Token persistence abstraction.
#[async_trait::async_trait]
pub trait TokenStore: Send + Sync + std::fmt::Debug {
    /// Loads token state, returning `None` when no token is stored.
    async fn load(&self) -> TidalResult<Option<TidalTokenState>>;
    /// Stores token state.
    async fn save(&self, state: &TidalTokenState) -> TidalResult<()>;
    /// Removes token state.
    async fn clear(&self) -> TidalResult<()>;
}

/// JSON file-backed token store.
#[derive(Debug, Clone)]
pub struct FileTokenStore {
    path: PathBuf,
}

impl FileTokenStore {
    /// Creates a token store for a path.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Returns the token path.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[async_trait::async_trait]
impl TokenStore for FileTokenStore {
    async fn load(&self) -> TidalResult<Option<TidalTokenState>> {
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || {
            if !path.exists() {
                return Ok(None);
            }
            let contents = fs::read_to_string(&path).map_err(|error| token_error(&path, &error))?;
            serde_json::from_str(&contents)
                .map(Some)
                .map_err(|error| TidalError::TokenStore {
                    path: path.display().to_string(),
                    message: error.to_string(),
                })
        })
        .await
        .map_err(|error| TidalError::TokenStore {
            path: self.path.display().to_string(),
            message: error.to_string(),
        })?
    }

    async fn save(&self, state: &TidalTokenState) -> TidalResult<()> {
        let path = self.path.clone();
        let state = state.clone();
        tokio::task::spawn_blocking(move || {
            let parent = path.parent().ok_or_else(|| TidalError::TokenStore {
                path: path.display().to_string(),
                message: "token path must have a parent directory".to_owned(),
            })?;
            fs::create_dir_all(parent).map_err(|error| token_error(parent, &error))?;
            let mut tmp = tempfile::NamedTempFile::new_in(parent).map_err(|error| {
                TidalError::TokenStore {
                    path: parent.display().to_string(),
                    message: error.to_string(),
                }
            })?;
            serde_json::to_writer(&mut tmp, &state).map_err(|error| TidalError::TokenStore {
                path: path.display().to_string(),
                message: error.to_string(),
            })?;
            tmp.persist(&path).map_err(|error| TidalError::TokenStore {
                path: path.display().to_string(),
                message: error.error.to_string(),
            })?;
            Ok(())
        })
        .await
        .map_err(|error| TidalError::TokenStore {
            path: self.path.display().to_string(),
            message: error.to_string(),
        })?
    }

    async fn clear(&self) -> TidalResult<()> {
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(token_error(&path, &error)),
        })
        .await
        .map_err(|error| TidalError::TokenStore {
            path: self.path.display().to_string(),
            message: error.to_string(),
        })?
    }
}

fn token_error(path: &Path, error: &std::io::Error) -> TidalError {
    TidalError::TokenStore {
        path: path.display().to_string(),
        message: error.to_string(),
    }
}

/// HTTP OAuth client.
#[derive(Debug, Clone)]
pub struct TidalAuthClient {
    client: reqwest::Client,
    base_url: Url,
    credentials: TidalCredentials,
}

impl TidalAuthClient {
    /// Creates an auth client.
    pub const fn new(client: reqwest::Client, base_url: Url, credentials: TidalCredentials) -> Self {
        Self {
            client,
            base_url,
            credentials,
        }
    }

    /// Requests a device code.
    pub async fn device_authorization(&self) -> TidalResult<DeviceAuthorization> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Response {
            device_code: String,
            user_code: String,
            verification_uri: String,
            verification_uri_complete: String,
            expires_in: u64,
            interval: u64,
        }

        let url = self.join("device_authorization")?;
        let response = self
            .client
            .post(url)
            .form(&[
                ("client_id", self.credentials.client_id.as_str()),
                ("scope", SCOPE),
            ])
            .send()
            .await
            .map_err(|source| TidalError::Http {
                operation: "device authorization",
                source,
            })?;
        let status = response.status();
        if !status.is_success() {
            return Err(api_error("device authorization", status.as_u16(), response).await);
        }
        let response = response
            .json::<Response>()
            .await
            .map_err(|source| TidalError::Http {
                operation: "device authorization json",
                source,
            })?;
        Ok(DeviceAuthorization {
            device_code: DeviceCode::new(response.device_code)?,
            user_code: UserCode::new(response.user_code)?,
            verification_uri: parse_url(&self.base_url, &response.verification_uri)?,
            verification_uri_complete: parse_url(
                &self.base_url,
                &response.verification_uri_complete,
            )?,
            expires_at: time::OffsetDateTime::now_utc()
                + time::Duration::seconds(i64::try_from(response.expires_in).unwrap_or(0)),
            interval: PollingInterval::new(Duration::from_secs(response.interval))?,
        })
    }

    /// Refreshes a token.
    pub async fn refresh(&self, refresh_token: &RefreshToken) -> TidalResult<TidalTokenState> {
        self.token(&[
            ("client_id", self.credentials.client_id.as_str()),
            ("refresh_token", refresh_token.expose_secret()),
            ("grant_type", "refresh_token"),
            ("scope", SCOPE),
        ])
        .await
    }

    /// Exchanges an authorized device code for token state.
    pub async fn exchange_device_code(
        &self,
        device_code: &DeviceCode,
    ) -> TidalResult<TidalTokenState> {
        self.token(&[
            ("client_id", self.credentials.client_id.as_str()),
            ("device_code", device_code.as_str()),
            ("grant_type", DEVICE_GRANT),
            ("scope", SCOPE),
        ])
        .await
    }

    async fn token(&self, form: &[(&str, &str)]) -> TidalResult<TidalTokenState> {
        #[derive(Deserialize)]
        struct User {
            #[serde(rename = "countryCode")]
            country_code: String,
        }

        #[derive(Deserialize)]
        struct Response {
            access_token: String,
            refresh_token: Option<String>,
            expires_in: u64,
            user_id: i64,
            user: User,
        }

        let response = self
            .client
            .post(self.join("token")?)
            .basic_auth(
                self.credentials.client_id.as_str(),
                Some(self.credentials.client_secret.expose_secret()),
            )
            .form(form)
            .send()
            .await
            .map_err(|source| TidalError::Http {
                operation: "token",
                source,
            })?;
        let status = response.status();
        if !status.is_success() {
            return Err(api_error("token", status.as_u16(), response).await);
        }
        let response = response
            .json::<Response>()
            .await
            .map_err(|source| TidalError::Http {
                operation: "token json",
                source,
            })?;
        let refresh_token = response
            .refresh_token
            .or_else(|| {
                form.iter()
                    .find(|(key, _)| *key == "refresh_token")
                    .map(|(_, value)| (*value).to_owned())
            })
            .ok_or(TidalError::InvalidResponse {
                field: "refresh_token".to_owned(),
                message: "server did not return a refresh token".to_owned(),
            })?;
        Ok(TidalTokenState {
            access_token: AccessToken::new(response.access_token)?,
            refresh_token: RefreshToken::new(refresh_token)?,
            expires_at: TokenExpiry::from_now(response.expires_in, time::OffsetDateTime::now_utc()),
            user_id: TidalUserId::new(response.user_id.to_string())?,
            country_code: CountryCode::new(response.user.country_code)?,
        })
    }

    /// Logs out an access token.
    pub async fn logout(&self, access_token: &AccessToken) -> TidalResult<()> {
        let url = Url::parse("https://api.tidal.com/v1/logout").map_err(|error| {
            TidalError::InvalidValue {
                field: "logout_url",
                message: error.to_string(),
            }
        })?;
        let response = self
            .client
            .post(url)
            .bearer_auth(access_token.expose_secret())
            .send()
            .await
            .map_err(|source| TidalError::Http {
                operation: "logout",
                source,
            })?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(api_error("logout", response.status().as_u16(), response).await)
        }
    }

    fn join(&self, path: &str) -> TidalResult<Url> {
        self.base_url
            .join(path)
            .map_err(|error| TidalError::InvalidValue {
                field: "auth_base_url",
                message: error.to_string(),
            })
    }
}

async fn api_error(
    operation: impl Into<String>,
    status: u16,
    response: reqwest::Response,
) -> TidalError {
    let message = response
        .text()
        .await
        .unwrap_or_else(|e| format!("<body read failed: {e}>"));

    if let Some(oauth_error) = parse_oauth_error(&message) {
        return oauth_error;
    }

    TidalError::Api {
        operation: operation.into(),
        status,
        message,
    }
}

/// Attempts to extract a structured `OAuth2` error from a JSON response body.
fn parse_oauth_error(body: &str) -> Option<TidalError> {
    #[derive(serde::Deserialize)]
    struct OAuthError {
        #[serde(default)]
        error: String,
    }

    let parsed: OAuthError = serde_json::from_str(body).ok()?;
    match parsed.error.as_str() {
        "authorization_pending" => Some(TidalError::AuthorizationPending),
        "slow_down" => Some(TidalError::SlowDown),
        "expired_token" => Some(TidalError::ExpiredToken),
        _ => None,
    }
}

fn parse_url(base_url: &Url, value: &str) -> TidalResult<Url> {
    match Url::parse(value) {
        Ok(url) => Ok(url),
        Err(url::ParseError::RelativeUrlWithoutBase) => {
            if value.contains('.') && !value.starts_with('/') {
                return Url::parse(&format!("https://{value}")).map_err(|error| {
                    TidalError::InvalidValue {
                        field: "url",
                        message: error.to_string(),
                    }
                });
            }
            base_url
                .join(value)
                .map_err(|error| TidalError::InvalidValue {
                    field: "url",
                    message: error.to_string(),
                })
        }
        Err(error) => Err(TidalError::InvalidValue {
            field: "url",
            message: error.to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn country_code_validates_shape() -> TidalResult<()> {
        assert!(CountryCode::new("us").is_err());
        assert!(CountryCode::new("USA").is_err());
        assert_eq!(CountryCode::new("US")?.as_str(), "US");
        Ok(())
    }

    #[test]
    fn token_debug_redacts_secret() -> TidalResult<()> {
        let token = AccessToken::new("secret")?;
        assert_eq!(format!("{token:?}"), "AccessToken(\"***\")");
        Ok(())
    }

    #[test]
    fn slow_down_adds_five_seconds() -> TidalResult<()> {
        let interval = PollingInterval::new(Duration::from_secs(1))?.slow_down();
        assert_eq!(interval.as_duration(), Duration::from_secs(6));
        Ok(())
    }

    #[test]
    fn credentials_use_env_pair_when_present() -> TidalResult<()> {
        let credentials = TidalCredentials::from_sources(
            Some("new-id".to_owned()),
            Some("new-secret".to_owned()),
        )?;
        assert_eq!(credentials.client_id.as_str(), "new-id");
        assert_eq!(credentials.client_secret.expose_secret(), "new-secret");
        Ok(())
    }

    #[test]
    fn credentials_reject_partial_env_pair() {
        let error = TidalCredentials::from_sources(Some("new-id".to_owned()), None);
        assert!(
            matches!(error, Err(TidalError::InvalidValue { field, .. }) if field == "TIDAL credentials")
        );
    }

    #[test]
    fn credentials_fall_back_to_embedded_default() -> TidalResult<()> {
        let credentials = TidalCredentials::from_sources(None, None)?;
        assert_eq!(credentials.client_id.as_str(), "fX2JxdmntZWK0ixT");
        assert!(!credentials.client_secret.expose_secret().is_empty());
        Ok(())
    }

    #[test]
    fn verification_url_accepts_relative_response_values() -> TidalResult<()> {
        let base = Url::parse("https://auth.tidal.com/v1/oauth2/").map_err(|error| {
            TidalError::InvalidValue {
                field: "url",
                message: error.to_string(),
            }
        })?;
        let parsed = parse_url(&base, "device").map_err(|error| TidalError::InvalidValue {
            field: "url",
            message: error.to_string(),
        })?;
        assert_eq!(parsed.as_str(), "https://auth.tidal.com/v1/oauth2/device");
        Ok(())
    }

    #[test]
    fn verification_url_accepts_scheme_less_hosts() -> TidalResult<()> {
        let base = Url::parse("https://auth.tidal.com/v1/oauth2/").map_err(|error| {
            TidalError::InvalidValue {
                field: "url",
                message: error.to_string(),
            }
        })?;
        let parsed = parse_url(&base, "link.tidal.com/XKLLI")?;
        assert_eq!(parsed.as_str(), "https://link.tidal.com/XKLLI");
        Ok(())
    }
}
