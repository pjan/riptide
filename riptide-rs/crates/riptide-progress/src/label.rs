//! Pure label and quality-badge builders.
//!
//! These functions convert `riptide-core` catalog types into the
//! [`DisplayLabel`] and [`QualityBadge`] used by the progress event model.
//! They are pure functions with no I/O.

use crate::event::{DisplayLabel, QualityBadge, QualitySymbol};
use riptide_core::{
    Album, AudioQuality, ResolvedQuality, StreamVariant, Track, TrackNumber, Video,
};

/// Builds a display label for a track.
///
/// The primary artist is taken from the first [`riptide_core::ArtistCredit`];
/// the group is the album title when available.
pub fn label_for_track(track: &Track) -> DisplayLabel {
    DisplayLabel {
        title: track.title.clone(),
        artist: track.artists.first().map(|c| c.artist.name.clone()),
        group: track.album.as_ref().map(|a| a.title.clone()),
        track_number: track.track_number,
    }
}

/// Builds a display label for a video.
pub fn label_for_video(video: &Video) -> DisplayLabel {
    DisplayLabel {
        title: video.title.clone(),
        artist: video.artists.first().map(|c| c.artist.name.clone()),
        group: video.album.as_ref().map(|a| a.title.clone()),
        track_number: video.track_number,
    }
}

/// Builds a display label for an album.
pub fn label_for_album(album: &Album) -> DisplayLabel {
    DisplayLabel {
        title: album.title.clone(),
        artist: album.artists.first().map(|c| c.artist.name.clone()),
        group: None,
        track_number: None,
    }
}

/// Derives a quality badge from a resolved stream variant.
///
/// Returns `None` only when a new [`ResolvedQuality`] variant is encountered
/// that this function has no mapping for (guarded by `#[non_exhaustive]` on
/// the core enum).
pub fn quality_badge_for_stream(stream: &StreamVariant) -> Option<QualityBadge> {
    let (symbol_kind, text, detail) = match &stream.quality {
        ResolvedQuality::Audio(audio) => audio_badge(stream, audio),
        ResolvedQuality::Video(video) => {
            let text = match video {
                riptide_core::VideoQuality::Low => "360p".to_owned(),
                riptide_core::VideoQuality::Medium => "720p".to_owned(),
                riptide_core::VideoQuality::High => "1080p".to_owned(),
                riptide_core::VideoQuality::ProviderSpecific(label) => label.as_str().to_owned(),
            };
            (QualitySymbol::Video, text, None)
        }
        ResolvedQuality::ProviderSpecific { label, .. } => {
            (QualitySymbol::Lossless, label.as_str().to_owned(), None)
        }
    };
    Some(QualityBadge {
        symbol_kind,
        text,
        detail,
    })
}

fn audio_badge(
    stream: &StreamVariant,
    audio: &AudioQuality,
) -> (QualitySymbol, String, Option<String>) {
    match audio {
        AudioQuality::Low => (QualitySymbol::Low, "Low".to_owned(), None),
        AudioQuality::High => (QualitySymbol::High, "High".to_owned(), None),
        AudioQuality::Lossless => (QualitySymbol::Lossless, "Lossless".to_owned(), None),
        AudioQuality::HiResLossless => {
            let detail = hires_detail(stream);
            (QualitySymbol::HiRes, "Hi-Res".to_owned(), detail)
        }
        AudioQuality::ProviderSpecific(label) => {
            (QualitySymbol::Lossless, label.as_str().to_owned(), None)
        }
    }
}

fn hires_detail(stream: &StreamVariant) -> Option<String> {
    match (stream.bit_depth, stream.sample_rate) {
        (Some(bd), Some(sr)) => Some(format!("{}/{}", bd.get(), sr.get() / 1000)),
        _ => None,
    }
}

/// Formats a track number as a zero-padded two-digit string.
///
/// Returns `None` when `number` is `None`.
///
/// ```
/// use riptide_progress::label::format_track_number;
/// use riptide_core::TrackNumber;
///
/// assert_eq!(format_track_number(TrackNumber::new(1).ok()), Some("01".to_owned()));
/// assert_eq!(format_track_number(TrackNumber::new(12).ok()), Some("12".to_owned()));
/// assert_eq!(format_track_number(None), None);
/// ```
pub fn format_track_number(number: Option<TrackNumber>) -> Option<String> {
    number.map(|n| format!("{:02}", n.get()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use riptide_core::{
        ArtistCredit, ArtistRole, AudioQuality, ResolvedQuality, ResourceId, ResourceKind,
        TrackNumber,
    };

    fn make_media_ref() -> Result<riptide_core::MediaRef, Box<dyn std::error::Error>> {
        Ok(riptide_core::MediaRef::new(
            riptide_core::ProviderId::new("tidal")?,
            ResourceKind::Track,
            ResourceId::new("1")?,
        ))
    }

    fn make_artist_credit(name: &str) -> ArtistCredit {
        ArtistCredit {
            artist: riptide_core::Artist {
                id: None,
                name: name.to_owned(),
            },
            role: ArtistRole::Main,
        }
    }

    #[test]
    fn track_label_includes_two_digit_number() -> Result<(), Box<dyn std::error::Error>> {
        let track = Track {
            id: make_media_ref()?,
            title: "Pull Me Under".to_owned(),
            artists: vec![make_artist_credit("Dream Theater")],
            album: None,
            duration: None,
            track_number: Some(TrackNumber::new(1)?),
            disc_number: None,
            explicit: None,
            external_ids: riptide_core::ExternalIds::default(),
            version: None,
            copyright: None,
            bpm: None,
            quality_label: None,
            dolby_atmos: false,
        };
        let label = label_for_track(&track);
        assert_eq!(label.title, "Pull Me Under");
        assert_eq!(label.artist.as_deref(), Some("Dream Theater"));
        assert_eq!(
            format_track_number(label.track_number),
            Some("01".to_owned())
        );
        Ok(())
    }

    #[test]
    fn track_label_omits_number_when_absent() -> Result<(), Box<dyn std::error::Error>> {
        let track = Track {
            id: make_media_ref()?,
            title: "Untitled".to_owned(),
            artists: vec![],
            album: None,
            duration: None,
            track_number: None,
            disc_number: None,
            explicit: None,
            external_ids: riptide_core::ExternalIds::default(),
            version: None,
            copyright: None,
            bpm: None,
            quality_label: None,
            dolby_atmos: false,
        };
        let label = label_for_track(&track);
        assert!(label.track_number.is_none());
        assert_eq!(format_track_number(label.track_number), None);
        Ok(())
    }

    #[test]
    fn album_label_includes_artist_and_title() -> Result<(), Box<dyn std::error::Error>> {
        use riptide_core::ResourceKind;
        let album = Album {
            id: riptide_core::MediaRef::new(
                riptide_core::ProviderId::new("tidal")?,
                ResourceKind::Album,
                ResourceId::new("2")?,
            ),
            title: "Images and Words".to_owned(),
            artists: vec![make_artist_credit("Dream Theater")],
            release_date: None,
            track_count: None,
            video_count: None,
            explicit: None,
            release_type: None,
            hires_lossless: true,
            cover: None,
        };
        let label = label_for_album(&album);
        assert_eq!(label.title, "Images and Words");
        assert_eq!(label.artist.as_deref(), Some("Dream Theater"));
        assert!(label.track_number.is_none());
        Ok(())
    }

    fn make_stream(quality: ResolvedQuality) -> Result<StreamVariant, Box<dyn std::error::Error>> {
        Ok(StreamVariant {
            source: make_media_ref()?,
            quality,
            container: riptide_core::ContainerKind::Flac,
            codec: None,
            bit_depth: None,
            sample_rate: None,
            manifest: riptide_core::Manifest::DirectSegments(vec![]),
            post_process: vec![],
        })
    }

    #[test]
    fn quality_badge_lossless() -> Result<(), Box<dyn std::error::Error>> {
        let stream = make_stream(ResolvedQuality::Audio(AudioQuality::Lossless))?;
        let badge = quality_badge_for_stream(&stream).ok_or("expected badge")?;
        assert_eq!(badge.symbol_kind, QualitySymbol::Lossless);
        assert_eq!(badge.text, "Lossless");
        assert!(badge.detail.is_none());
        Ok(())
    }

    #[test]
    fn quality_badge_hires_without_metrics() -> Result<(), Box<dyn std::error::Error>> {
        let stream = make_stream(ResolvedQuality::Audio(AudioQuality::HiResLossless))?;
        let badge = quality_badge_for_stream(&stream).ok_or("expected badge")?;
        assert_eq!(badge.symbol_kind, QualitySymbol::HiRes);
        assert_eq!(badge.text, "Hi-Res");
        assert!(badge.detail.is_none()); // no bit_depth/sample_rate
        Ok(())
    }

    #[test]
    fn quality_badge_hires_with_metrics() -> Result<(), Box<dyn std::error::Error>> {
        use riptide_core::{BitDepth, SampleRateHz};
        let mut stream = make_stream(ResolvedQuality::Audio(AudioQuality::HiResLossless))?;
        stream.bit_depth = Some(BitDepth::new(24)?);
        stream.sample_rate = Some(SampleRateHz::new(96_000)?);
        let badge = quality_badge_for_stream(&stream).ok_or("expected badge")?;
        assert_eq!(badge.detail.as_deref(), Some("24/96"));
        Ok(())
    }

    #[test]
    fn quality_badge_video() -> Result<(), Box<dyn std::error::Error>> {
        let stream = make_stream(ResolvedQuality::Video(riptide_core::VideoQuality::High))?;
        let badge = quality_badge_for_stream(&stream).ok_or("expected badge")?;
        assert_eq!(badge.symbol_kind, QualitySymbol::Video);
        assert_eq!(badge.text, "1080p");
        Ok(())
    }

    #[test]
    fn quality_badge_provider_specific() -> Result<(), Box<dyn std::error::Error>> {
        let stream = make_stream(ResolvedQuality::Audio(AudioQuality::ProviderSpecific(
            riptide_core::ProviderQualityLabel::new("MAX")?,
        )))?;
        let badge = quality_badge_for_stream(&stream).ok_or("expected badge")?;
        assert_eq!(badge.text, "MAX");
        Ok(())
    }

    #[test]
    fn format_track_number_pads_single_digit() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            format_track_number(Some(TrackNumber::new(3)?)),
            Some("03".to_owned())
        );
        Ok(())
    }

    #[test]
    fn format_track_number_does_not_truncate_two_digit() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            format_track_number(Some(TrackNumber::new(12)?)),
            Some("12".to_owned())
        );
        Ok(())
    }
}
