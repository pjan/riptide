//! Configuration parsing and validation for riptide-rs.

pub mod error;
pub mod raw;
pub mod validated;

pub use error::{ConfigError, ConfigResult};
pub use validated::{
    Concurrency, DownloadConfig, DownloadRoot, ListenerConfig, ListenerPort, ListenerSecret,
    M3uConfig, MetadataConfig, SinglesMode, TemplateConfig, ValidatedConfig,
};
