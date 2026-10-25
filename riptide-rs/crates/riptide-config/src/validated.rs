//! Validated runtime configuration.

use crate::{
    error::{ConfigError, ConfigResult},
    raw::RawConfig,
};
use riptide_core::AudioQualityRequest;
use riptide_template::CompiledTemplate;
use serde::Deserialize;
use std::{
    fmt,
    num::{NonZeroU16, NonZeroU8},
    path::{Path, PathBuf},
    str::FromStr,
};

/// Runtime configuration after validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedConfig {
    /// Download configuration.
    pub download: DownloadConfig,
    /// Listener configuration.
    pub listener: ListenerConfig,
    /// Metadata configuration.
    pub metadata: MetadataConfig,
    /// M3U configuration.
    pub m3u: M3uConfig,
}

impl ValidatedConfig {
    /// Parses TOML and validates the runtime configuration.
    pub fn from_toml_str(input: &str) -> ConfigResult<Self> {
        let value =
            toml::from_str::<toml::Value>(input).map_err(|error| ConfigError::TomlParse {
                message: error.to_string(),
            })?;

        inspect_unknown_keys(&value)?;

        let raw = RawConfig::deserialize(value).map_err(|error| ConfigError::Decode {
            message: error.to_string(),
        })?;

        Self::try_from(raw)
    }
}

impl TryFrom<RawConfig> for ValidatedConfig {
    type Error = ConfigError;

    fn try_from(raw: RawConfig) -> Result<Self, Self::Error> {
        Ok(Self {
            download: DownloadConfig::try_from(raw.download)?,
            listener: ListenerConfig::try_from(raw.listener)?,
            metadata: MetadataConfig::from(raw.metadata),
            m3u: M3uConfig::from(raw.m3u),
        })
    }
}

impl Default for ValidatedConfig {
    fn default() -> Self {
        Self::try_from(RawConfig::default())
            .unwrap_or_else(|_| unreachable!("raw defaults are valid"))
    }
}

/// Runtime download configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadConfig {
    /// Requested audio quality.
    pub audio_quality: AudioQualityRequest,
    /// Skip existing files.
    pub skip_existing: bool,
    /// Download concurrency.
    pub concurrency: Concurrency,
    /// Optional download root.
    pub download_root: Option<DownloadRoot>,
    /// Singles handling mode.
    pub singles_mode: SinglesMode,
    /// Template configuration.
    pub templates: TemplateConfig,
}

impl TryFrom<crate::raw::RawDownloadConfig> for DownloadConfig {
    type Error = ConfigError;

    fn try_from(raw: crate::raw::RawDownloadConfig) -> Result<Self, Self::Error> {
        let audio_quality = AudioQualityRequest::from_str(&raw.audio_quality).map_err(|error| {
            ConfigError::InvalidValue {
                path: "download.audio_quality".to_owned(),
                message: error.to_string(),
            }
        })?;

        Ok(Self {
            audio_quality,
            skip_existing: raw.skip_existing,
            concurrency: Concurrency::new(raw.concurrency)?,
            download_root: raw.download_root.map(DownloadRoot::new).transpose()?,
            singles_mode: SinglesMode::from_str(&raw.singles_mode)?,
            templates: TemplateConfig::try_from(raw.templates)?,
        })
    }
}

/// Download concurrency limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Concurrency(NonZeroU8);

impl Concurrency {
    /// Creates a concurrency limit in the supported range 1-32.
    pub fn new(value: u8) -> ConfigResult<Self> {
        let Some(value) = NonZeroU8::new(value) else {
            return Err(ConfigError::InvalidValue {
                path: "download.concurrency".to_owned(),
                message: "must be between 1 and 32".to_owned(),
            });
        };
        if value.get() > 32 {
            return Err(ConfigError::InvalidValue {
                path: "download.concurrency".to_owned(),
                message: "must be between 1 and 32".to_owned(),
            });
        }
        Ok(Self(value))
    }

    /// Returns the concurrency limit.
    pub const fn get(self) -> u8 {
        self.0.get()
    }
}

/// Validated download root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadRoot(PathBuf);

impl DownloadRoot {
    /// Creates a download root after rejecting empty paths.
    pub fn new(path: impl Into<PathBuf>) -> ConfigResult<Self> {
        let path = path.into();
        if path.as_os_str().is_empty() {
            return Err(ConfigError::InvalidValue {
                path: "download.download_root".to_owned(),
                message: "cannot be empty".to_owned(),
            });
        }
        Ok(Self(path))
    }

    /// Returns the path.
    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

/// How artist singles are handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SinglesMode {
    /// Exclude singles and EPs.
    Exclude,
    /// Include singles and EPs.
    Include,
    /// Download only singles and EPs.
    Only,
}

impl FromStr for SinglesMode {
    type Err = ConfigError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "exclude" => Ok(Self::Exclude),
            "include" => Ok(Self::Include),
            "only" => Ok(Self::Only),
            _ => Err(ConfigError::InvalidValue {
                path: "download.singles_mode".to_owned(),
                message: "expected exclude, include, or only".to_owned(),
            }),
        }
    }
}

/// Validated template configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateConfig {
    /// Default template.
    pub default: CompiledTemplate,
    /// Track override.
    pub track: Option<CompiledTemplate>,
    /// Album override.
    pub album: Option<CompiledTemplate>,
    /// Playlist override.
    pub playlist: Option<CompiledTemplate>,
}

impl TryFrom<crate::raw::RawTemplateConfig> for TemplateConfig {
    type Error = ConfigError;

    fn try_from(raw: crate::raw::RawTemplateConfig) -> Result<Self, Self::Error> {
        let Some(default) = parse_template("download.templates.default", Some(raw.default))? else {
            unreachable!("default template is always provided by raw defaults");
        };
        Ok(Self {
            default,
            track: parse_template("download.templates.track", raw.track)?,
            album: parse_template("download.templates.album", raw.album)?,
            playlist: parse_template("download.templates.playlist", raw.playlist)?,
        })
    }
}

fn parse_template(
    path: &'static str,
    value: Option<String>,
) -> ConfigResult<Option<CompiledTemplate>> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_empty() {
        return Err(ConfigError::InvalidValue {
            path: path.to_owned(),
            message: "cannot be empty; omit the key to inherit the default".to_owned(),
        });
    }
    CompiledTemplate::parse(&value)
        .map(Some)
        .map_err(|error| ConfigError::InvalidValue {
            path: path.to_owned(),
            message: error.to_string(),
        })
}

/// Metadata configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetadataConfig {
    /// Enable metadata writes.
    pub enable: bool,
    /// Enable lyrics sidecars.
    pub lyrics: bool,
    /// Enable cover sidecars.
    pub cover: bool,
}

impl From<crate::raw::RawMetadataConfig> for MetadataConfig {
    fn from(raw: crate::raw::RawMetadataConfig) -> Self {
        Self {
            enable: raw.enable,
            lyrics: raw.lyrics,
            cover: raw.cover,
        }
    }
}

/// M3U configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct M3uConfig {
    /// Save M3U playlists.
    pub save: bool,
}

impl From<crate::raw::RawM3uConfig> for M3uConfig {
    fn from(raw: crate::raw::RawM3uConfig) -> Self {
        Self { save: raw.save }
    }
}

/// Runtime listener configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListenerConfig {
    /// Listener port.
    pub port: ListenerPort,
    /// Optional listener secret.
    pub secret: Option<ListenerSecret>,
}

impl TryFrom<crate::raw::RawListenerConfig> for ListenerConfig {
    type Error = ConfigError;

    fn try_from(raw: crate::raw::RawListenerConfig) -> Result<Self, Self::Error> {
        Ok(Self {
            port: ListenerPort::new(raw.port)?,
            secret: ListenerSecret::optional(raw.secret)?,
        })
    }
}

/// Non-zero listener port.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListenerPort(NonZeroU16);

impl ListenerPort {
    /// Creates a non-zero listener port.
    pub fn new(value: u16) -> ConfigResult<Self> {
        NonZeroU16::new(value).map_or_else(
            || {
                Err(ConfigError::InvalidValue {
                    path: "listener.port".to_owned(),
                    message: "must be greater than zero".to_owned(),
                })
            },
            |value| Ok(Self(value)),
        )
    }

    /// Returns the port value.
    pub const fn get(self) -> u16 {
        self.0.get()
    }
}

/// Listener authentication secret.
#[derive(Clone, PartialEq, Eq)]
pub struct ListenerSecret(String);

impl ListenerSecret {
    /// Creates a listener secret, rejecting empty input.
    pub fn new(value: impl Into<String>) -> ConfigResult<Self> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(ConfigError::InvalidValue {
                path: "listener.secret".to_owned(),
                message: "cannot be empty".to_owned(),
            });
        }
        Ok(Self(value))
    }

    /// Converts an empty secret into `None`.
    pub fn optional(value: String) -> ConfigResult<Option<Self>> {
        if value.trim().is_empty() {
            Ok(None)
        } else {
            Self::new(value).map(Some)
        }
    }

    /// Returns the secret string.
    pub fn expose_secret(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ListenerSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ListenerSecret").field(&"***").finish()
    }
}

fn inspect_unknown_keys(value: &toml::Value) -> ConfigResult<()> {
    let Some(root) = value.as_table() else {
        return Err(ConfigError::InvalidValue {
            path: "<root>".to_owned(),
            message: "expected a TOML table".to_owned(),
        });
    };

    for key in root.keys() {
        match key.as_str() {
            "download" | "listener" | "metadata" | "m3u" => {}
            other => {
                return Err(ConfigError::UnknownKey {
                    path: other.to_owned(),
                });
            }
        }
    }

    inspect_table(
        root,
        "download",
        &[
            "audio_quality",
            "skip_existing",
            "concurrency",
            "download_root",
            "singles_mode",
            "templates",
        ],
    )?;
    inspect_table(
        section_table(root, "download"),
        "templates",
        &["default", "track", "album", "playlist"],
    )?;
    inspect_table(root, "listener", &["port", "secret"])?;
    inspect_table(root, "metadata", &["enable", "lyrics", "cover"])?;
    inspect_table(root, "m3u", &["save"])?;

    Ok(())
}

fn section_table<'a>(
    root: &'a toml::map::Map<String, toml::Value>,
    section: &str,
) -> &'a toml::map::Map<String, toml::Value> {
    root.get(section)
        .and_then(toml::Value::as_table)
        .unwrap_or(root)
}

fn inspect_table(
    root: &toml::map::Map<String, toml::Value>,
    section: &'static str,
    allowed: &[&str],
) -> ConfigResult<()> {
    let Some(section_value) = root.get(section) else {
        return Ok(());
    };

    let Some(table) = section_value.as_table() else {
        return Err(ConfigError::InvalidValue {
            path: section.to_owned(),
            message: "expected a TOML table".to_owned(),
        });
    };

    for key in table.keys() {
        if allowed.contains(&key.as_str()) {
            continue;
        }

        return Err(ConfigError::UnknownKey {
            path: format!("{section}.{key}"),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config_uses_defaults() -> Result<(), Box<dyn std::error::Error>> {
        let config = ValidatedConfig::from_toml_str("")?;
        assert_eq!(config.download.audio_quality, AudioQualityRequest::High);
        assert!(config.download.skip_existing);
        assert_eq!(config.download.concurrency.get(), 4);
        assert_eq!(config.download.singles_mode, SinglesMode::Include);
        assert!(config.download.download_root.is_none());
        assert!(config.metadata.enable);
        assert!(config.metadata.lyrics);
        assert!(config.metadata.cover);
        assert!(!config.m3u.save);
        assert_eq!(config.listener.port.get(), 8123);
        assert!(config.listener.secret.is_none());
        Ok(())
    }

    #[test]
    fn valid_config_parses_to_typed_values() -> Result<(), Box<dyn std::error::Error>> {
        let config = ValidatedConfig::from_toml_str(
            r#"
            [download]
            audio_quality = "lossless"
            skip_existing = false
            concurrency = 2
            download_root = "Music/Riptide"
            singles_mode = "only"

            [download.templates]
            default = "{item.title}"

            [listener]
            port = 9000
            secret = "secret"

            [metadata]
            enable = false
            lyrics = false
            cover = false

            [m3u]
            save = true
            "#,
        )?;

        assert_eq!(config.download.audio_quality, AudioQualityRequest::Lossless);
        assert!(!config.download.skip_existing);
        assert_eq!(config.download.concurrency.get(), 2);
        assert_eq!(config.download.singles_mode, SinglesMode::Only);
        assert_eq!(
            config
                .download
                .download_root
                .as_ref()
                .map(DownloadRoot::as_path),
            Some(Path::new("Music/Riptide"))
        );
        assert_eq!(config.listener.port.get(), 9000);
        assert!(!config.metadata.enable);
        assert!(!config.metadata.lyrics);
        assert!(!config.metadata.cover);
        assert!(config.m3u.save);
        let Some(secret) = config.listener.secret else {
            panic!("expected listener secret");
        };
        assert_eq!(secret.expose_secret(), "secret");
        assert_eq!(format!("{secret:?}"), "ListenerSecret(\"***\")");
        Ok(())
    }

    #[test]
    fn invalid_audio_quality_fails_with_path() {
        let error = ValidatedConfig::from_toml_str(
            r#"
            [download]
            audio_quality = "maximum"
            "#,
        );
        assert!(matches!(
            error,
            Err(ConfigError::InvalidValue { ref path, .. }) if path == "download.audio_quality"
        ));
    }

    #[test]
    fn video_quality_is_no_longer_supported() {
        let error = ValidatedConfig::from_toml_str(
            r#"
            [download]
            video_quality = "maximum"
            "#,
        );
        assert!(matches!(
            error,
            Err(ConfigError::UnknownKey { ref path }) if path == "download.video_quality"
        ));
    }

    #[test]
    fn unsupported_quality_values_are_invalid_values() {
        for (key, value, path) in [
            ("audio_quality", "normal", "download.audio_quality"),
            ("audio_quality", "max", "download.audio_quality"),
        ] {
            let error = ValidatedConfig::from_toml_str(&format!(
                r#"
                [download]
                {key} = "{value}"
                "#
            ));
            assert!(matches!(
                error,
                Err(ConfigError::InvalidValue { path: ref error_path, .. }) if error_path == path
            ));
        }
    }

    #[test]
    fn video_download_keys_are_unknown() {
        for (toml, path) in [
            (
                r#"
                [download]
                videos_mode = "skip"
                "#,
                "download.videos_mode",
            ),
            (
                r#"
                [download.templates]
                video = "{item.title}"
                "#,
                "templates.video",
            ),
        ] {
            let error = ValidatedConfig::from_toml_str(toml);
            assert!(
                matches!(error, Err(ConfigError::UnknownKey { path: ref error_path }) if error_path == path)
            );
        }
    }

    #[test]
    fn port_zero_fails() {
        let error = ValidatedConfig::from_toml_str(
            r"
            [listener]
            port = 0
            ",
        );
        assert!(matches!(
            error,
            Err(ConfigError::InvalidValue { ref path, .. }) if path == "listener.port"
        ));
    }

    #[test]
    fn empty_listener_secret_becomes_none() -> Result<(), Box<dyn std::error::Error>> {
        let config = ValidatedConfig::from_toml_str(
            r#"
            [listener]
            secret = ""
            "#,
        )?;
        assert!(config.listener.secret.is_none());
        Ok(())
    }

    #[test]
    fn unsupported_download_key_is_unknown() {
        let error = ValidatedConfig::from_toml_str(
            r"
            [download]
            threads_count = 4
            ",
        );
        assert!(matches!(
            error,
            Err(ConfigError::UnknownKey { ref path }) if path == "download.threads_count"
        ));
    }

    #[test]
    fn empty_template_override_is_invalid() {
        let error = ValidatedConfig::from_toml_str(
            r#"
            [download.templates]
            track = ""
            "#,
        );
        assert!(matches!(
            error,
            Err(ConfigError::InvalidValue { ref path, .. })
                if path == "download.templates.track"
        ));
    }

    #[test]
    fn concurrency_is_bounded() {
        for value in [0, 33] {
            let error = ValidatedConfig::from_toml_str(&format!(
                r"
                [download]
                concurrency = {value}
                "
            ));
            assert!(matches!(
                error,
                Err(ConfigError::InvalidValue { ref path, .. })
                    if path == "download.concurrency"
            ));
        }
    }

    #[test]
    fn unsupported_root_key_is_unknown() {
        let error = ValidatedConfig::from_toml_str("enable_cache = true");
        assert!(matches!(
            error,
            Err(ConfigError::UnknownKey { ref path }) if path == "enable_cache"
        ));
    }

    #[test]
    fn unknown_key_fails_with_path_context() {
        let error = ValidatedConfig::from_toml_str(
            r"
            [listener]
            unknown = true
            ",
        );
        assert!(matches!(
            error,
            Err(ConfigError::UnknownKey { ref path }) if path == "listener.unknown"
        ));
    }
}
