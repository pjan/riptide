//! Cover sidecar writing.

use crate::{lyrics::atomic_write, MetadataResult};
use std::path::Path;

/// Saves cover art bytes using atomic temp-then-persist.
pub fn save_cover(data: &[u8], path: &Path) -> MetadataResult<()> {
    atomic_write(path, data)
}
