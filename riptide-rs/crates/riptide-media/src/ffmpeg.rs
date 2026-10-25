//! `FFmpeg` subprocess wrapper.

use crate::{MediaError, MediaResult};
use riptide_core::PostProcessStep;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

/// Checks whether ffmpeg is available on PATH.
pub async fn ffmpeg_available() -> bool {
    tokio::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
        .await
        .is_ok_and(|output| output.status.success())
}

/// Builds an ffmpeg command that extracts FLAC from an M4A container.
pub fn ffmpeg_extract_flac_command(source: &Path, output: &Path) -> Command {
    let mut command = Command::new("ffmpeg");
    command
        .arg("-y")
        .arg("-i")
        .arg(source)
        .arg("-c")
        .arg("copy")
        .arg(output);
    command
}

/// Builds an ffmpeg command that remuxes a transport stream to MP4.
pub fn ffmpeg_remux_to_mp4_command(source: &Path, output: &Path) -> Command {
    let mut command = Command::new("ffmpeg");
    command
        .arg("-y")
        .arg("-i")
        .arg(source)
        .arg("-c")
        .arg("copy")
        .arg(output);
    command
}

/// Extracts FLAC from an M4A container.
pub async fn extract_flac(source: &Path) -> MediaResult<PathBuf> {
    let output = source.with_extension("flac");
    run_ffmpeg("extract_flac", ffmpeg_extract_flac_command(source, &output)).await?;
    Ok(output)
}

/// Remuxes a transport stream to MP4.
pub async fn remux_to_mp4(source: &Path) -> MediaResult<PathBuf> {
    let output = source.with_extension("mp4");
    run_ffmpeg("remux_to_mp4", ffmpeg_remux_to_mp4_command(source, &output)).await?;
    Ok(output)
}

/// Applies post-processing steps and returns the current output path.
pub async fn post_process(path: PathBuf, steps: &[PostProcessStep]) -> MediaResult<PathBuf> {
    let mut current = path;
    let mut previous_source = None;
    for step in steps {
        match step {
            PostProcessStep::ExtractFlac => {
                let source = current;
                let next = extract_flac(&source).await?;
                previous_source = Some(source);
                current = next;
            }
            PostProcessStep::RemuxToMp4 => {
                let source = current;
                let next = remux_to_mp4(&source).await?;
                previous_source = Some(source);
                current = next;
            }
            PostProcessStep::RemoveSource => {
                if let Some(source) = previous_source.take() {
                    let _ = tokio::fs::remove_file(source).await;
                }
            }
            _ => unreachable!("new post-process step not handled"),
        }
    }
    Ok(current)
}

async fn run_ffmpeg(operation: &'static str, command: Command) -> MediaResult<()> {
    let output = tokio::process::Command::from(command)
        .output()
        .await
        .map_err(|source| {
            if source.kind() == std::io::ErrorKind::NotFound {
                MediaError::FfmpegNotFound
            } else {
                MediaError::io("running ffmpeg", source)
            }
        })?;
    if output.status.success() {
        return Ok(());
    }
    Err(MediaError::FfmpegFailed {
        operation,
        exit_code: output.status.code(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ffmpeg_commands_use_stream_copy() {
        let command = ffmpeg_extract_flac_command(Path::new("input.m4a"), Path::new("out.flac"));
        let args: Vec<_> = command.get_args().collect();
        assert!(args.iter().any(|arg| *arg == "-c"));
        assert!(args.iter().any(|arg| *arg == "copy"));
    }
}
