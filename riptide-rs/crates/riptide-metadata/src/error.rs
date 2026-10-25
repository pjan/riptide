//! Errors for metadata operations.

use riptide_core::ContainerExtension;
use thiserror::Error;

/// Convenient result alias for metadata operations.
pub type MetadataResult<T> = Result<T, MetadataError>;

/// Errors produced by metadata writers.
#[derive(Debug, Error)]
pub enum MetadataError {
    /// Container cannot be tagged.
    #[error("unsupported metadata container: {extension:?}")]
    UnsupportedContainer {
        /// Unsupported extension.
        extension: ContainerExtension,
    },

    /// Tag writing failed.
    #[error("failed to write tags")]
    TagWrite {
        /// Source lofty error.
        #[source]
        source: lofty::error::LoftyError,
    },

    /// File I/O failed.
    #[error("I/O error while {operation}")]
    Io {
        /// Operation context.
        operation: &'static str,
        /// Source error.
        #[source]
        source: std::io::Error,
    },
}

impl MetadataError {
    pub(crate) const fn io(operation: &'static str, source: std::io::Error) -> Self {
        Self::Io { operation, source }
    }
}
