//! Quality request and resolved quality types.

use crate::{ProviderId, ProviderQualityLabel};
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

/// Audio quality requested by a caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioQualityRequest {
    /// Low quality.
    Low,
    /// High quality.
    High,
    /// Lossless quality.
    Lossless,
    /// High-resolution lossless quality.
    HiResLossless,
    /// Best available quality.
    Best,
}

impl FromStr for AudioQualityRequest {
    type Err = QualityParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "low" => Ok(Self::Low),
            "high" => Ok(Self::High),
            "lossless" => Ok(Self::Lossless),
            "hi_res_lossless" => Ok(Self::HiResLossless),
            "best" => Ok(Self::Best),
            _ => Err(QualityParseError {
                value: value.to_owned(),
                expected: "low, high, lossless, hi_res_lossless, best",
            }),
        }
    }
}

/// Quality request for a stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "quality", rename_all = "snake_case")]
pub enum QualityRequest {
    /// Audio quality request.
    Audio(AudioQualityRequest),
    /// Best available stream regardless of media type.
    BestAvailable,
}

/// Audio quality resolved by a provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioQuality {
    /// Low quality.
    Low,
    /// High quality.
    High,
    /// Lossless quality.
    Lossless,
    /// High-resolution lossless quality.
    HiResLossless,
    /// Provider-specific quality.
    ProviderSpecific(ProviderQualityLabel),
}

/// Quality actually returned by a provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolvedQuality {
    /// Audio quality.
    Audio(AudioQuality),
    /// Provider-specific quality label.
    ProviderSpecific {
        /// Provider that produced the label.
        provider: ProviderId,
        /// Provider label.
        label: ProviderQualityLabel,
    },
}

/// Error produced when parsing a quality string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QualityParseError {
    value: String,
    expected: &'static str,
}

impl QualityParseError {
    /// Returns the invalid value.
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Returns a human-readable list of expected values.
    pub const fn expected(&self) -> &'static str {
        self.expected
    }
}

impl fmt::Display for QualityParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid quality `{}`; expected one of: {}",
            self.value, self.expected
        )
    }
}

impl std::error::Error for QualityParseError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_quality_request_uses_documented_strings() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            "hi_res_lossless".parse::<AudioQualityRequest>()?,
            AudioQualityRequest::HiResLossless
        );
        assert!("hires_lossless".parse::<AudioQualityRequest>().is_err());
        Ok(())
    }

    #[test]
    fn resolved_quality_allows_provider_specific_label() -> Result<(), Box<dyn std::error::Error>> {
        let quality = ResolvedQuality::ProviderSpecific {
            provider: ProviderId::new("tidal")?,
            label: ProviderQualityLabel::new("MAX")?,
        };
        assert!(matches!(quality, ResolvedQuality::ProviderSpecific { .. }));
        Ok(())
    }
}
