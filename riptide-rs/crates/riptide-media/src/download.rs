//! Streaming segment download.

use crate::{MediaError, MediaResult};
use futures_util::StreamExt;
use std::{path::Path, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufWriter};
use url::Url;

/// Segment download behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DownloadOptions {
    /// Per-attempt segment timeout.
    pub segment_timeout: Duration,
    /// Retry attempts after the first failed attempt. Keep this in the small range 0-5.
    pub max_retries_per_segment: u8,
}

impl Default for DownloadOptions {
    fn default() -> Self {
        Self {
            segment_timeout: Duration::from_secs(30),
            max_retries_per_segment: 3,
        }
    }
}

/// Progress callback for segment downloads.
pub trait DownloadProgress: Send + Sync {
    /// Called when a segment starts.
    fn on_segment_start(
        &self,
        segment_index: usize,
        total_segments: usize,
        content_length: Option<u64>,
    );

    /// Called as bytes are appended to the output.
    fn on_bytes(&self, bytes_downloaded: u64);
}

/// Downloads segments to a file using streaming I/O.
pub async fn download_segments(
    http: &reqwest::Client,
    segments: &[Url],
    output_path: &Path,
    options: DownloadOptions,
    progress: Option<&dyn DownloadProgress>,
) -> MediaResult<()> {
    let output_file = tokio::fs::File::create(output_path)
        .await
        .map_err(|source| MediaError::io("creating download output", source))?;
    let mut output = BufWriter::new(output_file);

    for (index, url) in segments.iter().enumerate() {
        let part_path = output_path.with_extension(format!("part-{index}"));
        download_segment_with_retries(
            http,
            url,
            index,
            &part_path,
            options,
            progress,
            segments.len(),
        )
        .await?;
        Box::pin(append_file(&mut output, &part_path)).await?;
        let _ = tokio::fs::remove_file(&part_path).await;
    }

    output
        .flush()
        .await
        .map_err(|source| MediaError::io("flushing download output", source))
}

async fn download_segment_with_retries(
    http: &reqwest::Client,
    url: &Url,
    index: usize,
    part_path: &Path,
    options: DownloadOptions,
    progress: Option<&dyn DownloadProgress>,
    total_segments: usize,
) -> MediaResult<()> {
    let mut last_error = None;
    for _attempt in 0..=options.max_retries_per_segment {
        let _ = tokio::fs::remove_file(part_path).await;
        match download_one_segment(
            http,
            url,
            index,
            part_path,
            options.segment_timeout,
            progress,
            total_segments,
        )
        .await
        {
            Ok(()) => return Ok(()),
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.unwrap_or(MediaError::ManifestParse {
        format: "download",
        message: "segment retry loop did not run".to_owned(),
    }))
}

async fn download_one_segment(
    http: &reqwest::Client,
    url: &Url,
    index: usize,
    part_path: &Path,
    timeout: Duration,
    progress: Option<&dyn DownloadProgress>,
    total_segments: usize,
) -> MediaResult<()> {
    let response = tokio::time::timeout(timeout, http.get(url.clone()).send())
        .await
        .map_err(|source| MediaError::Io {
            operation: "waiting for segment response",
            source: std::io::Error::new(std::io::ErrorKind::TimedOut, source),
        })?
        .map_err(|source| MediaError::SegmentDownload {
            segment_index: index,
            source,
        })?;
    if !response.status().is_success() {
        return Err(MediaError::SegmentStatus {
            segment_index: index,
            status: response.status().as_u16(),
        });
    }
    let content_length = response.content_length();
    if let Some(progress) = progress {
        progress.on_segment_start(index, total_segments, content_length);
    }

    let file = tokio::fs::File::create(part_path)
        .await
        .map_err(|source| MediaError::io("creating segment part", source))?;
    let mut writer = BufWriter::new(file);
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|source| MediaError::SegmentDownload {
            segment_index: index,
            source,
        })?;
        writer
            .write_all(&chunk)
            .await
            .map_err(|source| MediaError::io("writing segment part", source))?;
        if let Some(progress) = progress {
            progress.on_bytes(u64::try_from(chunk.len()).unwrap_or(u64::MAX));
        }
    }
    writer
        .flush()
        .await
        .map_err(|source| MediaError::io("flushing segment part", source))
}

async fn append_file(output: &mut BufWriter<tokio::fs::File>, path: &Path) -> MediaResult<()> {
    let mut input = tokio::fs::File::open(path)
        .await
        .map_err(|source| MediaError::io("opening segment part", source))?;
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let read = input
            .read(&mut buffer)
            .await
            .map_err(|source| MediaError::io("reading segment part", source))?;
        if read == 0 {
            break;
        }
        output
            .write_all(&buffer[..read])
            .await
            .map_err(|source| MediaError::io("appending segment part", source))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    #[tokio::test]
    async fn segment_download_streams_and_concatenates_in_order(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let server = tokio::spawn(async move {
            for _ in 0..2 {
                let (mut socket, _) = listener.accept().await?;
                tokio::spawn(async move {
                    let mut buffer = [0_u8; 1024];
                    let read = socket.read(&mut buffer).await?;
                    let request = String::from_utf8_lossy(&buffer[..read]);
                    let body = if request.starts_with("GET /a ") {
                        "aaa"
                    } else {
                        "bbb"
                    };
                    socket
                        .write_all(
                            format!(
                                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
                                body.len(),
                                body
                            )
                            .as_bytes(),
                        )
                        .await?;
                    Ok::<_, std::io::Error>(())
                });
            }
            Ok::<_, std::io::Error>(())
        });

        let segments = vec![
            Url::parse(&format!("http://{addr}/a"))?,
            Url::parse(&format!("http://{addr}/b"))?,
        ];
        let dir = tempfile::tempdir()?;
        let output = dir.path().join("out.bin");
        download_segments(
            &reqwest::Client::new(),
            &segments,
            &output,
            DownloadOptions::default(),
            None,
        )
        .await?;

        server.await??;
        assert_eq!(tokio::fs::read(output).await?, b"aaabbb");
        Ok(())
    }
}
