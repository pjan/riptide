//! Errors for provider-neutral media operations.

use thiserror::Error;

/// Convenient result alias for media operations.
pub type MediaResult<T> = Result<T, MediaError>;

/// Errors produced by manifest resolution, downloads, and post-processing.
#[derive(Debug, Error)]
pub enum MediaError {
    /// Manifest parsing failed.
    #[error("failed to parse {format} manifest: {message}")]
    ManifestParse {
        /// Manifest format.
        format: &'static str,
        /// Failure details.
        message: String,
    },

    /// Segment download failed.
    #[error("failed to download segment {segment_index}")]
    SegmentDownload {
        /// Zero-based segment index.
        segment_index: usize,
        /// Source HTTP error.
        #[source]
        source: reqwest::Error,
    },

    /// HTTP response status was not successful.
    #[error("failed to download segment {segment_index}: status {status}")]
    SegmentStatus {
        /// Zero-based segment index.
        segment_index: usize,
        /// HTTP status code.
        status: u16,
    },

    /// `FFmpeg` executable was not found.
    #[error("ffmpeg was not found on PATH")]
    FfmpegNotFound,

    /// `FFmpeg` process failed.
    #[error("ffmpeg {operation} failed with exit code {exit_code:?}: {stderr}")]
    FfmpegFailed {
        /// Operation name.
        operation: &'static str,
        /// Process exit code.
        exit_code: Option<i32>,
        /// Captured stderr.
        stderr: String,
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

impl MediaError {
    pub(crate) const fn io(operation: &'static str, source: std::io::Error) -> Self {
        Self::Io { operation, source }
    }
}
