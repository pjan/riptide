//! Provider-neutral manifest resolution.

use crate::{MediaError, MediaResult};
use riptide_core::{Codec, Manifest};
use url::Url;

/// Resolved manifest segments ready for download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSegments {
    /// Concrete segment URLs.
    pub urls: Vec<Url>,
    /// Codec label discovered while parsing.
    pub codec: Option<Codec>,
}

/// Resolves manifest variants that require no I/O.
pub fn resolve_manifest_sync(manifest: &Manifest) -> MediaResult<Option<ResolvedSegments>> {
    match manifest {
        Manifest::DirectSegments(urls) => Ok(Some(ResolvedSegments {
            urls: urls.clone(),
            codec: None,
        })),
        Manifest::DashXml(xml) => parse_dash_mpd(xml).map(Some),
        Manifest::HlsMediaPlaylist(content) => {
            let base = Url::parse("https://media.invalid/").map_err(|error| {
                MediaError::ManifestParse {
                    format: "hls",
                    message: error.to_string(),
                }
            })?;
            Ok(Some(ResolvedSegments {
                urls: parse_hls_media_playlist(content, &base)?,
                codec: None,
            }))
        }
        Manifest::HlsMasterPlaylistUrl(_) => Ok(None),
    }
}

/// Resolves manifest variants that require async HTTP.
pub async fn resolve_manifest_async(
    manifest: &Manifest,
    http: &reqwest::Client,
) -> MediaResult<ResolvedSegments> {
    if let Some(resolved) = resolve_manifest_sync(manifest)? {
        return Ok(resolved);
    }

    let Manifest::HlsMasterPlaylistUrl(master_url) = manifest else {
        unreachable!("sync resolver returned None only for HLS master playlists");
    };
    let master = http
        .get(master_url.clone())
        .send()
        .await
        .map_err(|source| MediaError::SegmentDownload {
            segment_index: 0,
            source,
        })?
        .text()
        .await
        .map_err(|source| MediaError::SegmentDownload {
            segment_index: 0,
            source,
        })?;
    let variant = select_hls_variant(&master, master_url)?;
    let media = http
        .get(variant.clone())
        .send()
        .await
        .map_err(|source| MediaError::SegmentDownload {
            segment_index: 0,
            source,
        })?
        .text()
        .await
        .map_err(|source| MediaError::SegmentDownload {
            segment_index: 0,
            source,
        })?;
    Ok(ResolvedSegments {
        urls: parse_hls_media_playlist(&media, &variant)?,
        codec: None,
    })
}

/// Parses a DASH MPD XML document into segment URLs and codec info.
pub fn parse_dash_mpd(xml: &str) -> MediaResult<ResolvedSegments> {
    let base_url = first_tag_text(xml, "BaseURL").and_then(|value| Url::parse(&value).ok());
    let codec = attr_value(xml, "codecs")
        .or_else(|| attr_value(xml, "codec"))
        .map(Codec::new)
        .transpose()
        .map_err(|error| MediaError::ManifestParse {
            format: "dash",
            message: error.to_string(),
        })?;

    let mut urls = Vec::new();
    for media in attr_values(xml, "media") {
        let url = match &base_url {
            Some(base) => base
                .join(&media)
                .map_err(|error| MediaError::ManifestParse {
                    format: "dash",
                    message: error.to_string(),
                })?,
            None => Url::parse(&media).map_err(|error| MediaError::ManifestParse {
                format: "dash",
                message: error.to_string(),
            })?,
        };
        urls.push(url);
    }

    if urls.is_empty() {
        if let Some(base) = base_url {
            urls.push(base);
        }
    }

    if urls.is_empty() {
        return Err(MediaError::ManifestParse {
            format: "dash",
            message: "no segment URLs found".to_owned(),
        });
    }

    Ok(ResolvedSegments { urls, codec })
}

/// Parses an HLS media playlist into segment URLs.
pub fn parse_hls_media_playlist(content: &str, base_url: &Url) -> MediaResult<Vec<Url>> {
    let mut urls = Vec::new();
    for line in content.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        urls.push(
            base_url
                .join(line)
                .map_err(|error| MediaError::ManifestParse {
                    format: "hls",
                    message: error.to_string(),
                })?,
        );
    }
    if urls.is_empty() {
        return Err(MediaError::ManifestParse {
            format: "hls",
            message: "no segment URLs found".to_owned(),
        });
    }
    Ok(urls)
}

fn select_hls_variant(content: &str, base_url: &Url) -> MediaResult<Url> {
    let mut best: Option<(u64, Url)> = None;
    let mut pending_bandwidth = None;
    for line in content.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("#EXT-X-STREAM-INF:") {
            pending_bandwidth = parse_bandwidth(rest);
            continue;
        }
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let bandwidth = pending_bandwidth.take().unwrap_or(0);
        let url = base_url
            .join(line)
            .map_err(|error| MediaError::ManifestParse {
                format: "hls",
                message: error.to_string(),
            })?;
        if best
            .as_ref()
            .is_none_or(|(current, _)| bandwidth > *current)
        {
            best = Some((bandwidth, url));
        }
    }
    best.map(|(_, url)| url)
        .ok_or_else(|| MediaError::ManifestParse {
            format: "hls",
            message: "no variants found".to_owned(),
        })
}

fn parse_bandwidth(attributes: &str) -> Option<u64> {
    attributes.split(',').find_map(|part| {
        let (key, value) = part.split_once('=')?;
        (key == "BANDWIDTH").then(|| value.parse().ok()).flatten()
    })
}

fn first_tag_text(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    Some(xml[start..end].trim().to_owned())
}

fn attr_value(xml: &str, attr: &str) -> Option<String> {
    attr_values(xml, attr).into_iter().next()
}

fn attr_values(xml: &str, attr: &str) -> Vec<String> {
    let mut values = Vec::new();
    let needle = format!("{attr}=\"");
    let mut rest = xml;
    while let Some(index) = rest.find(&needle) {
        let start = index + needle.len();
        let Some(end) = rest[start..].find('"') else {
            break;
        };
        values.push(rest[start..start + end].to_owned());
        rest = &rest[start + end + 1..];
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dash_parser_resolves_segment_urls() -> Result<(), Box<dyn std::error::Error>> {
        let xml = r#"
        <MPD>
          <BaseURL>https://cdn.example.test/audio/</BaseURL>
          <Representation codecs="flac">
            <SegmentList>
              <SegmentURL media="one.flac" />
              <SegmentURL media="two.flac" />
            </SegmentList>
          </Representation>
        </MPD>
        "#;
        let parsed = parse_dash_mpd(xml)?;
        assert_eq!(parsed.urls.len(), 2);
        assert_eq!(
            parsed.urls[0].as_str(),
            "https://cdn.example.test/audio/one.flac"
        );
        assert_eq!(parsed.codec.as_ref().map(Codec::as_str), Some("flac"));
        Ok(())
    }

    #[test]
    fn hls_parser_resolves_relative_segments() -> Result<(), Box<dyn std::error::Error>> {
        let base = Url::parse("https://cdn.example.test/video/playlist.m3u8")?;
        let urls = parse_hls_media_playlist("#EXTM3U\nseg-1.ts\nseg-2.ts\n", &base)?;
        assert_eq!(urls[1].as_str(), "https://cdn.example.test/video/seg-2.ts");
        Ok(())
    }
}
