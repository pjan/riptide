//! TIDAL stream response decoding.

use crate::{model, TidalError, TidalResult};
use base64::{engine::general_purpose::STANDARD, Engine};
use riptide_core::{
    AudioQuality, AudioQualityRequest, Codec, ContainerKind, Manifest, MediaRef, PostProcessStep,
    ProviderQualityLabel, ResolvedQuality, StreamVariant,
};
use url::Url;

/// TIDAL API label for an audio quality request.
pub const fn audio_quality_request_label(request: AudioQualityRequest) -> &'static str {
    match request {
        AudioQualityRequest::Low => "LOW",
        AudioQualityRequest::High => "HIGH",
        AudioQualityRequest::Lossless => "LOSSLESS",
        AudioQualityRequest::HiResLossless | AudioQualityRequest::Best => "HI_RES_LOSSLESS",
    }
}

/// Decodes a TIDAL track stream response.
pub fn decode_track_manifest(
    response: &model::TidalStreamResponse,
    source: &MediaRef,
    requested_quality: AudioQualityRequest,
) -> TidalResult<StreamVariant> {
    let decoded = decode_manifest_text(&response.manifest)?;
    let quality_label = response
        .audio_quality
        .as_deref()
        .unwrap_or_else(|| audio_quality_request_label(requested_quality));
    let resolved_quality = audio_quality(quality_label)?;
    let (manifest, codec) = manifest_and_codec(response.manifest_mime_type, &decoded)?;
    let high_res = matches!(
        requested_quality,
        AudioQualityRequest::HiResLossless | AudioQualityRequest::Best
    ) || matches!(quality_label, "HI_RES" | "HI_RES_LOSSLESS" | "MAX");
    let container = audio_container(codec.as_ref(), high_res);
    let post_process = audio_post_process(codec.as_ref(), high_res);

    Ok(StreamVariant {
        source: source.clone(),
        quality: ResolvedQuality::Audio(resolved_quality),
        container,
        codec,
        bit_depth: None,
        sample_rate: None,
        manifest,
        post_process,
    })
}

fn decode_manifest_text(encoded: &str) -> TidalResult<String> {
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|error| TidalError::InvalidResponse {
            field: "stream.manifest".to_owned(),
            message: error.to_string(),
        })?;
    String::from_utf8(bytes).map_err(|error| TidalError::InvalidResponse {
        field: "stream.manifest".to_owned(),
        message: error.to_string(),
    })
}

fn manifest_and_codec(
    mime: model::TidalManifestMime,
    decoded: &str,
) -> TidalResult<(Manifest, Option<Codec>)> {
    match mime {
        model::TidalManifestMime::Bts | model::TidalManifestMime::Emu => {
            let bts: model::BtsManifest =
                serde_json::from_str(decoded).map_err(|error| TidalError::InvalidResponse {
                    field: "stream.manifest".to_owned(),
                    message: error.to_string(),
                })?;
            let codec = bts.codec.map(Codec::new).transpose().map_err(|error| {
                TidalError::InvalidResponse {
                    field: "stream.codec".to_owned(),
                    message: error.to_string(),
                }
            })?;
            Ok((Manifest::DirectSegments(bts.urls), codec))
        }
        model::TidalManifestMime::Dash => Ok((Manifest::DashXml(decoded.to_owned()), None)),
        model::TidalManifestMime::Hls => Ok((
            Manifest::HlsMasterPlaylistUrl(Url::parse(decoded.trim()).map_err(|error| {
                TidalError::InvalidResponse {
                    field: "stream.manifest".to_owned(),
                    message: error.to_string(),
                }
            })?),
            None,
        )),
        model::TidalManifestMime::Other => Err(TidalError::InvalidResponse {
            field: "stream.manifestMimeType".to_owned(),
            message: "unsupported manifest MIME type".to_owned(),
        }),
    }
}

fn audio_container(codec: Option<&Codec>, high_res: bool) -> ContainerKind {
    let codec = codec.map(Codec::as_str).unwrap_or_default();
    if codec.eq_ignore_ascii_case("flac") && !high_res {
        ContainerKind::Flac
    } else {
        ContainerKind::M4a
    }
}

fn audio_post_process(codec: Option<&Codec>, high_res: bool) -> Vec<PostProcessStep> {
    let codec = codec.map(Codec::as_str).unwrap_or_default();
    if codec.eq_ignore_ascii_case("flac") && high_res {
        vec![PostProcessStep::ExtractFlac, PostProcessStep::RemoveSource]
    } else {
        Vec::new()
    }
}

fn audio_quality(label: &str) -> TidalResult<AudioQuality> {
    Ok(match label {
        "LOW" => AudioQuality::Low,
        "HIGH" => AudioQuality::High,
        "LOSSLESS" => AudioQuality::Lossless,
        "HI_RES" | "HI_RES_LOSSLESS" | "MAX" => AudioQuality::HiResLossless,
        other => {
            AudioQuality::ProviderSpecific(ProviderQualityLabel::new(other).map_err(|error| {
                TidalError::InvalidResponse {
                    field: "stream.audioQuality".to_owned(),
                    message: error.to_string(),
                }
            })?)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use riptide_core::{ProviderId, ResourceId, ResourceKind};

    #[test]
    fn bts_manifest_decodes_to_direct_segments() -> Result<(), Box<dyn std::error::Error>> {
        let payload = r#"{"urls":["https://cdn.example.test/a.flac"],"codecs":"flac"}"#;
        let response = model::TidalStreamResponse {
            manifest: STANDARD.encode(payload),
            manifest_mime_type: model::TidalManifestMime::Bts,
            audio_quality: Some("LOSSLESS".to_owned()),
        };
        let stream = decode_track_manifest(
            &response,
            &MediaRef::new(
                ProviderId::new(crate::TIDAL_PROVIDER_ID.as_str())?,
                ResourceKind::Track,
                ResourceId::new("1")?,
            ),
            AudioQualityRequest::Lossless,
        )?;
        assert!(matches!(stream.manifest, Manifest::DirectSegments(_)));
        assert_eq!(stream.container, ContainerKind::Flac);
        Ok(())
    }

    #[test]
    fn high_res_flac_requests_extract_postprocess() -> Result<(), Box<dyn std::error::Error>> {
        let payload = r#"{"urls":["https://cdn.example.test/a.m4a"],"codecs":"flac"}"#;
        let response = model::TidalStreamResponse {
            manifest: STANDARD.encode(payload),
            manifest_mime_type: model::TidalManifestMime::Bts,
            audio_quality: Some("HI_RES_LOSSLESS".to_owned()),
        };
        let stream = decode_track_manifest(
            &response,
            &MediaRef::new(
                ProviderId::new(crate::TIDAL_PROVIDER_ID.as_str())?,
                ResourceKind::Track,
                ResourceId::new("1")?,
            ),
            AudioQualityRequest::HiResLossless,
        )?;
        assert_eq!(
            stream.post_process,
            vec![PostProcessStep::ExtractFlac, PostProcessStep::RemoveSource]
        );
        Ok(())
    }
}
