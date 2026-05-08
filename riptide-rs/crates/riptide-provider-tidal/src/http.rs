//! HTTP client construction helpers.

use crate::{TidalError, TidalResult};
use std::{env, fs, path::PathBuf};

const CERT_ENV_VARS: &[&str] = &["SSL_CERT_FILE", "CARGO_HTTP_CAINFO", "CURL_CA_BUNDLE"];

/// Builds a reqwest client with optional environment-provided PEM roots.
pub fn client() -> TidalResult<reqwest::Client> {
    let mut builder = reqwest::Client::builder();
    if let Some(path) = certificate_bundle_path() {
        let pem = fs::read(&path).map_err(|error| TidalError::HttpClient {
            message: format!(
                "failed to read certificate bundle {}: {error}",
                path.display()
            ),
        })?;
        for certificate in
            reqwest::Certificate::from_pem_bundle(&pem).map_err(|error| TidalError::HttpClient {
                message: format!(
                    "failed to parse certificate bundle {}: {error}",
                    path.display()
                ),
            })?
        {
            builder = builder.add_root_certificate(certificate);
        }
    }
    builder.build().map_err(|error| TidalError::HttpClient {
        message: error.to_string(),
    })
}

fn certificate_bundle_path() -> Option<PathBuf> {
    CERT_ENV_VARS
        .iter()
        .filter_map(env::var_os)
        .map(PathBuf::from)
        .find(|path| path.is_file())
}
