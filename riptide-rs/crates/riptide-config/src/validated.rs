//! Validated runtime configuration.

use crate::{
    error::{ConfigError, ConfigResult},
    raw::RawConfig,
};
use riptide_core::{AudioQualityRequest, VideoQualityRequest};
use serde::Deserialize;
use std::{fmt, num::NonZeroU16, str::FromStr};

/// Runtime configuration after validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedConfig {
    /// Download configuration.
    pub download: DownloadConfig,
    /// Listener configuration.
    pub listener: ListenerConfig,
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
        })
    }
}

/// Runtime download configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadConfig {
    /// Requested audio quality.
    pub track_quality: AudioQualityRequest,
    /// Requested video quality.
    pub video_quality: VideoQualityRequest,
    /// Skip existing files.
    pub skip_existing: bool,
}

impl TryFrom<crate::raw::RawDownloadConfig> for DownloadConfig {
    type Error = ConfigError;

    fn try_from(raw: crate::raw::RawDownloadConfig) -> Result<Self, Self::Error> {
        let track_quality = AudioQualityRequest::from_str(&raw.track_quality).map_err(|error| {
            ConfigError::InvalidValue {
                path: "download.track_quality".to_owned(),
                message: error.to_string(),
            }
        })?;

        let video_quality = VideoQualityRequest::from_str(&raw.video_quality).map_err(|error| {
            ConfigError::InvalidValue {
                path: "download.video_quality".to_owned(),
                message: error.to_string(),
            }
        })?;

        Ok(Self {
            track_quality,
            video_quality,
            skip_existing: raw.skip_existing,
        })
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
        match NonZeroU16::new(value) {
            Some(value) => Ok(Self(value)),
            None => Err(ConfigError::InvalidValue {
                path: "listener.port".to_owned(),
                message: "must be greater than zero".to_owned(),
            }),
        }
    }

    /// Returns the port value.
    pub fn get(self) -> u16 {
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
            "download" | "listener" => {}
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
        &["track_quality", "video_quality", "skip_existing"],
    )?;
    inspect_table(root, "listener", &["port", "secret"])?;

    Ok(())
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
        assert_eq!(config.download.track_quality, AudioQualityRequest::High);
        assert_eq!(config.download.video_quality, VideoQualityRequest::High);
        assert!(config.download.skip_existing);
        assert_eq!(config.listener.port.get(), 8123);
        assert!(config.listener.secret.is_none());
        Ok(())
    }

    #[test]
    fn valid_config_parses_to_typed_values() -> Result<(), Box<dyn std::error::Error>> {
        let config = ValidatedConfig::from_toml_str(
            r#"
            [download]
            track_quality = "lossless"
            video_quality = "best"
            skip_existing = false

            [listener]
            port = 9000
            secret = "secret"
            "#,
        )?;

        assert_eq!(config.download.track_quality, AudioQualityRequest::Lossless);
        assert_eq!(config.download.video_quality, VideoQualityRequest::Best);
        assert!(!config.download.skip_existing);
        assert_eq!(config.listener.port.get(), 9000);
        let Some(secret) = config.listener.secret else {
            panic!("expected listener secret");
        };
        assert_eq!(secret.expose_secret(), "secret");
        assert_eq!(format!("{secret:?}"), "ListenerSecret(\"***\")");
        Ok(())
    }

    #[test]
    fn invalid_track_quality_fails_with_path() {
        let error = ValidatedConfig::from_toml_str(
            r#"
            [download]
            track_quality = "maximum"
            "#,
        );
        assert!(matches!(
            error,
            Err(ConfigError::InvalidValue { ref path, .. }) if path == "download.track_quality"
        ));
    }

    #[test]
    fn invalid_video_quality_fails_with_path() {
        let error = ValidatedConfig::from_toml_str(
            r#"
            [download]
            video_quality = "maximum"
            "#,
        );
        assert!(matches!(
            error,
            Err(ConfigError::InvalidValue { ref path, .. }) if path == "download.video_quality"
        ));
    }

    #[test]
    fn unsupported_quality_values_are_invalid_values() {
        for (key, value, path) in [
            ("track_quality", "normal", "download.track_quality"),
            ("track_quality", "max", "download.track_quality"),
            ("video_quality", "sd", "download.video_quality"),
            ("video_quality", "hd", "download.video_quality"),
            ("video_quality", "fhd", "download.video_quality"),
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
    fn port_zero_fails() {
        let error = ValidatedConfig::from_toml_str(
            r#"
            [listener]
            port = 0
            "#,
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
            r#"
            [download]
            threads_count = 4
            "#,
        );
        assert!(matches!(
            error,
            Err(ConfigError::UnknownKey { ref path }) if path == "download.threads_count"
        ));
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
            r#"
            [listener]
            unknown = true
            "#,
        );
        assert!(matches!(
            error,
            Err(ConfigError::UnknownKey { ref path }) if path == "listener.unknown"
        ));
    }
}
