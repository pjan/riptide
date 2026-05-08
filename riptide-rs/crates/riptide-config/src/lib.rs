//! Configuration parsing and validation for riptide-rs.

pub mod error;
pub mod raw;
pub mod validated;

pub use error::{ConfigError, ConfigResult};
pub use validated::{
    DownloadConfig, ListenerConfig, ListenerPort, ListenerSecret, ValidatedConfig,
};
