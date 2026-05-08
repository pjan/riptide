//! Provider-neutral stream and lyrics types.

use crate::{
    BitDepth, Codec, MediaRef, ProviderContainerLabel, ProviderId, ResolvedQuality, SampleRateHz,
};
use serde::{Deserialize, Serialize};
use url::Url;

/// Media container kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContainerKind {
    /// FLAC audio.
    Flac,
    /// M4A/MP4 audio.
    M4a,
    /// MP4 video.
    Mp4,
    /// MPEG transport stream.
    TransportStream,
    /// Provider-specific container label.
    ProviderSpecific(ProviderContainerLabel),
}

/// Typed file extension derived from a media container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContainerExtension {
    /// FLAC file extension.
    Flac,
    /// M4A file extension.
    M4a,
    /// MP4 file extension.
    Mp4,
    /// MPEG transport stream extension.
    Ts,
}

impl ContainerExtension {
    /// Returns the extension including the leading dot.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Flac => ".flac",
            Self::M4a => ".m4a",
            Self::Mp4 => ".mp4",
            Self::Ts => ".ts",
        }
    }
}

/// Provider-neutral manifest input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Manifest {
    /// Direct segment URLs.
    DirectSegments(Vec<Url>),
    /// DASH XML document.
    DashXml(String),
    /// HLS master playlist URL.
    HlsMasterPlaylistUrl(Url),
    /// HLS media playlist text.
    HlsMediaPlaylist(String),
}

/// Stream variant returned by a provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamVariant {
    /// Source media reference.
    pub source: MediaRef,
    /// Resolved quality.
    pub quality: ResolvedQuality,
    /// Container kind.
    pub container: ContainerKind,
    /// Codec label.
    pub codec: Option<Codec>,
    /// Audio bit depth.
    pub bit_depth: Option<BitDepth>,
    /// Audio sample rate.
    pub sample_rate: Option<SampleRateHz>,
    /// Manifest input.
    pub manifest: Manifest,
    /// Provider-neutral post-processing intent.
    #[serde(default)]
    pub post_process: Vec<PostProcessStep>,
}

/// Lyrics source attribution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LyricsSource {
    /// Registered provider source.
    Provider(ProviderId),
    /// External display-only source.
    External(LyricsSourceName),
}

/// Display-only lyrics source name.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct LyricsSourceName(String);

impl LyricsSourceName {
    /// Creates a new lyrics source name after rejecting empty input.
    pub fn new(value: impl Into<String>) -> crate::CoreResult<Self> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(crate::CoreError::EmptyValue {
                type_name: "LyricsSourceName",
            });
        }
        Ok(Self(value))
    }

    /// Returns the source name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for LyricsSourceName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("LyricsSourceName").field(&self.0).finish()
    }
}

/// Lyrics text and optional synchronized lyrics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lyrics {
    /// Plain text lyrics.
    pub text: Option<String>,
    /// Synchronized LRC/subtitle lyrics.
    pub synchronized_lrc: Option<String>,
    /// Source attribution.
    pub source: Option<LyricsSource>,
}

/// Provider-neutral post-processing intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PostProcessStep {
    /// Extract FLAC from a container.
    ExtractFlac,
    /// Remux media to MP4.
    RemuxToMp4,
    /// Remove the source file after post-processing.
    RemoveSource,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AudioQuality, ProviderQualityLabel, ResourceId, ResourceKind};

    #[test]
    fn codec_rejects_empty() {
        assert!(Codec::new("").is_err());
    }

    #[test]
    fn lyrics_source_external_holds_attribution() -> Result<(), Box<dyn std::error::Error>> {
        let source = LyricsSource::External(LyricsSourceName::new("Musixmatch")?);
        match source {
            LyricsSource::External(name) => assert_eq!(name.as_str(), "Musixmatch"),
            LyricsSource::Provider(_) => panic!("unexpected provider source"),
        }
        Ok(())
    }

    #[test]
    fn lyrics_source_provider_wraps_provider_id() -> Result<(), Box<dyn std::error::Error>> {
        let provider = ProviderId::new("tidal")?;
        let source = LyricsSource::Provider(provider.clone());
        assert_eq!(source, LyricsSource::Provider(provider));
        Ok(())
    }

    #[test]
    fn stream_variant_represents_manifest_inputs() -> Result<(), Box<dyn std::error::Error>> {
        let variant = StreamVariant {
            source: MediaRef::new(
                ProviderId::new("tidal")?,
                ResourceKind::Track,
                ResourceId::new("123")?,
            ),
            quality: ResolvedQuality::Audio(AudioQuality::ProviderSpecific(
                ProviderQualityLabel::new("MAX")?,
            )),
            container: ContainerKind::M4a,
            codec: Some(Codec::new("flac")?),
            bit_depth: None,
            sample_rate: None,
            manifest: Manifest::DashXml("<MPD />".to_owned()),
            post_process: Vec::new(),
        };
        assert!(matches!(variant.manifest, Manifest::DashXml(_)));
        Ok(())
    }

    #[test]
    fn container_extension_uses_closed_labels() {
        assert_eq!(ContainerExtension::Flac.as_str(), ".flac");
        assert_eq!(ContainerExtension::M4a.as_str(), ".m4a");
        assert_eq!(ContainerExtension::Mp4.as_str(), ".mp4");
        assert_eq!(ContainerExtension::Ts.as_str(), ".ts");
    }
}
