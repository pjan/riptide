//! Structured configuration errors.

use thiserror::Error;

/// Result alias for configuration operations.
pub type ConfigResult<T> = Result<T, ConfigError>;

/// Errors produced while parsing or validating configuration.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ConfigError {
    /// TOML parsing failed.
    #[error("failed to parse TOML: {message}")]
    TomlParse {
        /// Parser message.
        message: String,
    },

    /// Strict deserialization failed.
    #[error("failed to decode config: {message}")]
    Decode {
        /// Decode message.
        message: String,
    },

    /// Unknown key was found.
    #[error("unknown config key `{path}`")]
    UnknownKey {
        /// Key path.
        path: String,
    },

    /// Known legacy key or value needs migration.
    #[error("config `{path}` changed: {message}")]
    Migration {
        /// Key path.
        path: String,
        /// Migration guidance.
        message: String,
    },

    /// A value failed validation.
    #[error("invalid config value at `{path}`: {message}")]
    InvalidValue {
        /// Key path.
        path: String,
        /// Validation message.
        message: String,
    },
}
