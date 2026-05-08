//! M3U playlist writing.

use crate::{lyrics::atomic_write, MetadataResult};
use riptide_core::MediaDuration;
use std::{
    fmt::Write as _,
    path::{Path, PathBuf},
};

/// A single M3U playlist entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct M3uEntry {
    /// Path relative to the playlist file location.
    pub path: PathBuf,
    /// Display title.
    pub title: String,
    /// Media duration.
    pub duration: Option<MediaDuration>,
}

/// Writes an Extended M3U playlist.
pub fn write_m3u(entries: &[M3uEntry], output: &Path) -> MetadataResult<()> {
    let mut text = String::from("#EXTM3U\n");
    for entry in entries {
        let duration = entry.duration.map_or(-1_i64, |duration| {
            i64::try_from(duration.as_duration().as_secs()).unwrap_or(i64::MAX)
        });
        let _ = writeln!(text, "#EXTINF:{duration},{}", entry.title);
        text.push_str(&entry.path.to_string_lossy());
        text.push('\n');
    }
    atomic_write(output, text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn m3u_writer_preserves_order_and_duration() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let output = dir.path().join("playlist.m3u");
        write_m3u(
            &[
                M3uEntry {
                    path: PathBuf::from("01.flac"),
                    title: "One".to_owned(),
                    duration: Some(MediaDuration::from_secs(12)),
                },
                M3uEntry {
                    path: PathBuf::from("02.flac"),
                    title: "Two".to_owned(),
                    duration: None,
                },
            ],
            &output,
        )?;
        let text = std::fs::read_to_string(output)?;
        assert!(text.contains("#EXTINF:12,One\n01.flac"));
        assert!(text.contains("#EXTINF:-1,Two\n02.flac"));
        Ok(())
    }
}
