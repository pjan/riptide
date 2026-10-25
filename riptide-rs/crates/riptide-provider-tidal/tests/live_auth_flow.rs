//! Gated live TIDAL auth flow verification.

use riptide_provider_tidal::{FileTokenStore, TidalAuthClient, TidalCredentials, TokenStore};
use std::time::Duration;
use url::Url;

#[tokio::test]
#[ignore = "requires real TIDAL credentials and manual device-code authorization"]
async fn live_tidal_device_auth_flow_completes() -> Result<(), Box<dyn std::error::Error>> {
    let credentials = TidalCredentials::from_env()?;
    let http = riptide_provider_tidal::http_client()?;
    let auth = TidalAuthClient::new(
        http,
        Url::parse("https://auth.tidal.com/v1/oauth2/")?,
        credentials,
    );
    let authorization = auth.device_authorization().await?;
    eprintln!("Open {}", authorization.verification_uri_complete);
    eprintln!("Code {}", authorization.user_code.as_str());

    let mut interval = authorization.interval;
    let deadline = authorization.expires_at;
    loop {
        tokio::time::sleep(interval.as_duration()).await;
        match auth.exchange_device_code(&authorization.device_code).await {
            Ok(token) => {
                let dir = tempfile::tempdir()?;
                let store = FileTokenStore::new(dir.path().join("tidal-token.json"));
                store.save(&token).await?;
                let loaded = store.load().await?;
                assert!(loaded.is_some());
                return Ok(());
            }
            Err(riptide_provider_tidal::TidalError::AuthorizationPending) => {}
            Err(riptide_provider_tidal::TidalError::SlowDown) => {
                interval = interval.slow_down();
            }
            Err(error) => return Err(Box::new(error) as Box<dyn std::error::Error>),
        }
        if time::OffsetDateTime::now_utc() >= deadline {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "device authorization expired",
            )
            .into());
        }
        if interval.as_duration() > Duration::from_secs(30) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "polling interval grew unexpectedly",
            )
            .into());
        }
    }
}
