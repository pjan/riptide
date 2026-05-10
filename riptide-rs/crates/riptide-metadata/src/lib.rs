//! Metadata, sidecar, cover, and playlist writing.

pub mod cover;
pub mod error;
pub mod lyrics;
pub mod m3u;
pub mod tags;

pub use cover::save_cover;
pub use error::{MetadataError, MetadataResult};
pub use lyrics::write_lyrics_sidecar;
pub use m3u::{write_m3u, M3uEntry};
pub use tags::{metadata_from_track, write_tags, FileMetadata};
