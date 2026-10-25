//! Provider-neutral media planning, manifest resolution, downloads, and post-processing.

pub mod download;
pub mod error;
pub mod ffmpeg;
pub mod manifest;
pub mod plan;

pub use download::{download_segments, DownloadOptions, DownloadProgress};
pub use error::{MediaError, MediaResult};
pub use ffmpeg::{
    extract_flac, ffmpeg_available, ffmpeg_extract_flac_command, ffmpeg_remux_to_mp4_command,
    post_process, remux_to_mp4,
};
pub use manifest::{
    parse_dash_mpd, parse_hls_media_playlist, resolve_manifest_async, resolve_manifest_sync,
    ResolvedSegments,
};
pub use plan::{plan_download, DownloadPlan, ResolvedDownloadPlan};
