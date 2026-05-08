//! Lyrics sidecar writing.

use crate::{MetadataError, MetadataResult};
use std::{io::Write, path::Path};

/// Writes synchronized lyrics to a `.lrc` sidecar using atomic temp-then-persist.
pub fn write_lyrics_sidecar(media_path: &Path, lrc_content: &str) -> MetadataResult<()> {
    let path = media_path.with_extension("lrc");
    atomic_write(&path, lrc_content.as_bytes())
}

pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> MetadataResult<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)
        .map_err(|source| MetadataError::io("creating sidecar directory", source))?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)
        .map_err(|source| MetadataError::io("creating sidecar temp file", source))?;
    temp.write_all(bytes)
        .map_err(|source| MetadataError::io("writing sidecar temp file", source))?;
    temp.persist(path)
        .map_err(|error| MetadataError::io("persisting sidecar", error.error))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lyrics_sidecar_uses_lrc_extension() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let media = dir.path().join("track.flac");
        write_lyrics_sidecar(&media, "[00:00.00]line")?;
        assert_eq!(
            std::fs::read_to_string(dir.path().join("track.lrc"))?,
            "[00:00.00]line"
        );
        Ok(())
    }
}
