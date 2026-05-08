//! Raw serde configuration shapes.

use serde::Deserialize;

/// Raw root configuration parsed from TOML.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RawConfig {
    /// Raw download configuration.
    pub download: RawDownloadConfig,
    /// Raw listener configuration.
    pub listener: RawListenerConfig,
}

/// Raw download configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RawDownloadConfig {
    /// Requested audio quality.
    pub track_quality: String,
    /// Requested video quality.
    pub video_quality: String,
    /// Skip existing files.
    pub skip_existing: bool,
}

impl Default for RawDownloadConfig {
    fn default() -> Self {
        Self {
            track_quality: "high".to_owned(),
            video_quality: "high".to_owned(),
            skip_existing: true,
        }
    }
}

/// Raw listener configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RawListenerConfig {
    /// Listener port.
    pub port: u16,
    /// Optional listener secret.
    pub secret: String,
}

impl Default for RawListenerConfig {
    fn default() -> Self {
        Self {
            port: 8123,
            secret: String::new(),
        }
    }
}
