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
    /// Raw metadata configuration.
    pub metadata: RawMetadataConfig,
    /// Raw M3U configuration.
    pub m3u: RawM3uConfig,
}

/// Raw download configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RawDownloadConfig {
    /// Requested audio quality.
    pub audio_quality: String,
    /// Skip existing files.
    pub skip_existing: bool,
    /// Download concurrency.
    pub concurrency: u8,
    /// Optional download root.
    pub download_root: Option<String>,
    /// Singles handling mode.
    pub singles_mode: String,
    /// Template configuration.
    pub templates: RawTemplateConfig,
}

impl Default for RawDownloadConfig {
    fn default() -> Self {
        Self {
            audio_quality: "high".to_owned(),
            skip_existing: true,
            concurrency: 4,
            download_root: None,
            singles_mode: "include".to_owned(),
            templates: RawTemplateConfig::default(),
        }
    }
}

/// Raw template configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RawTemplateConfig {
    /// Default template.
    pub default: String,
    /// Optional track override.
    pub track: Option<String>,
    /// Optional album override.
    pub album: Option<String>,
    /// Optional playlist override.
    pub playlist: Option<String>,
}

impl Default for RawTemplateConfig {
    fn default() -> Self {
        Self {
            default:
                "{album.artists}/{album.date:%Y} - {album.title}/{item.number:02d} {item.title}"
                    .to_owned(),
            track: None,
            album: None,
            playlist: None,
        }
    }
}

/// Raw metadata configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RawMetadataConfig {
    /// Enable metadata writing.
    pub enable: bool,
    /// Fetch and write lyrics.
    pub lyrics: bool,
    /// Fetch and write cover art.
    pub cover: bool,
}

impl Default for RawMetadataConfig {
    fn default() -> Self {
        Self {
            enable: true,
            lyrics: true,
            cover: true,
        }
    }
}

/// Raw M3U configuration.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RawM3uConfig {
    /// Save M3U playlist.
    pub save: bool,
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
