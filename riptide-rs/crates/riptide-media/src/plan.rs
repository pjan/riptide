//! Pure download plan construction.

use riptide_core::{ContainerExtension, ContainerKind, Manifest, PostProcessStep, StreamVariant};
use std::path::PathBuf;
use url::Url;

/// Pure plan for downloading one media item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadPlan {
    /// Where to write the final file without extension.
    pub output_stem: PathBuf,
    /// Final file extension after post-processing.
    pub final_extension: ContainerExtension,
    /// Provider-neutral manifest.
    pub manifest: Manifest,
    /// Post-processing steps.
    pub post_process: Vec<PostProcessStep>,
    /// Intermediate extension used while downloading.
    pub download_extension: ContainerExtension,
}

/// Plan after manifest resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedDownloadPlan {
    /// Original pure plan.
    pub plan: DownloadPlan,
    /// Concrete segment URLs.
    pub segments: Vec<Url>,
}

impl DownloadPlan {
    /// Final output path.
    pub fn final_path(&self) -> PathBuf {
        with_extension(&self.output_stem, self.final_extension)
    }

    /// Intermediate download path.
    pub fn download_path(&self) -> PathBuf {
        with_extension(&self.output_stem, self.download_extension)
    }
}

/// Constructs a pure download plan for a stream variant.
pub fn plan_download(variant: &StreamVariant, output_stem: PathBuf) -> DownloadPlan {
    let download_extension = extension_for_container(&variant.container);
    let final_extension = final_extension(download_extension, &variant.post_process);
    DownloadPlan {
        output_stem,
        final_extension,
        manifest: variant.manifest.clone(),
        post_process: variant.post_process.clone(),
        download_extension,
    }
}

const fn extension_for_container(container: &ContainerKind) -> ContainerExtension {
    match container {
        ContainerKind::Flac => ContainerExtension::Flac,
        ContainerKind::M4a | ContainerKind::ProviderSpecific(_) => ContainerExtension::M4a,
        ContainerKind::Mp4 => ContainerExtension::Mp4,
        ContainerKind::TransportStream => ContainerExtension::Ts,
    }
}

fn final_extension(
    download_extension: ContainerExtension,
    steps: &[PostProcessStep],
) -> ContainerExtension {
    if steps.contains(&PostProcessStep::ExtractFlac) {
        ContainerExtension::Flac
    } else if steps.contains(&PostProcessStep::RemuxToMp4) {
        ContainerExtension::Mp4
    } else {
        download_extension
    }
}

fn with_extension(stem: &std::path::Path, extension: ContainerExtension) -> PathBuf {
    let mut path = stem.to_path_buf();
    let without_dot = extension.as_str().trim_start_matches('.');
    path.set_extension(without_dot);
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use riptide_core::{
        AudioQuality, MediaRef, ProviderId, ResolvedQuality, ResourceId, ResourceKind,
    };

    #[test]
    fn download_plan_maps_postprocess_extension() -> Result<(), Box<dyn std::error::Error>> {
        let variant = StreamVariant {
            source: MediaRef::new(
                ProviderId::new("tidal")?,
                ResourceKind::Track,
                ResourceId::new("1")?,
            ),
            quality: ResolvedQuality::Audio(AudioQuality::HiResLossless),
            container: ContainerKind::M4a,
            codec: None,
            bit_depth: None,
            sample_rate: None,
            manifest: Manifest::DirectSegments(vec![Url::parse("https://example.test/a")?]),
            post_process: vec![PostProcessStep::ExtractFlac],
        };
        let plan = plan_download(&variant, PathBuf::from("Artist/Track"));
        assert_eq!(plan.download_path(), PathBuf::from("Artist/Track.m4a"));
        assert_eq!(plan.final_path(), PathBuf::from("Artist/Track.flac"));
        Ok(())
    }
}
