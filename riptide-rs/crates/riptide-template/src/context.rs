//! Template render context builders.

use crate::{ExtraFieldName, TemplateError, TemplateResult, TemplateValue};
use riptide_core::{Album, ArtistCredit, ArtistRole, Playlist, ProviderQualityLabel, Track, Video};
use serde::{Deserialize, Deserializer};
use std::{borrow::Cow, collections::BTreeMap};

/// One-based playlist index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PlaylistIndex(u32);

impl PlaylistIndex {
    /// Creates a one-based playlist index.
    pub const fn new(value: u32) -> Option<Self> {
        if value == 0 {
            None
        } else {
            Some(Self(value))
        }
    }

    /// Returns the underlying one-based value.
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Render context for a compiled template.
#[derive(Debug, Clone)]
pub struct TemplateContext {
    now: time::OffsetDateTime,
    item: Option<ItemTemplateInput>,
    album: Option<AlbumTemplateInput>,
    playlist: Option<PlaylistTemplateInput>,
    effective_quality: Option<ProviderQualityLabel>,
    extras: BTreeMap<ExtraFieldName, TemplateValue<'static>>,
}

impl TemplateContext {
    /// Creates an empty context with a stable `now` value.
    pub const fn new(now: time::OffsetDateTime) -> Self {
        Self {
            now,
            item: None,
            album: None,
            playlist: None,
            effective_quality: None,
            extras: BTreeMap::new(),
        }
    }

    /// Adds track fields to the item namespace.
    pub fn with_track(mut self, track: &Track) -> Self {
        self.item = Some(ItemTemplateInput::from_track(track));
        self
    }

    /// Adds video fields to the item namespace.
    pub fn with_video(mut self, video: &Video) -> Self {
        self.item = Some(ItemTemplateInput::from_video(video));
        self
    }

    /// Adds album fields to the album namespace.
    pub fn with_album(mut self, album: &Album) -> Self {
        self.album = Some(AlbumTemplateInput::from_album(album));
        self
    }

    /// Adds already-normalized item fields, useful for fixture rendering.
    pub fn with_item_input(mut self, item: ItemTemplateInput) -> Self {
        self.item = Some(item);
        self
    }

    /// Adds already-normalized album fields, useful for fixture rendering.
    pub fn with_album_input(mut self, album: AlbumTemplateInput) -> Self {
        self.album = Some(album);
        self
    }

    /// Adds playlist fields to the playlist namespace.
    pub fn with_playlist(mut self, playlist: &Playlist, index: PlaylistIndex) -> Self {
        self.playlist = Some(PlaylistTemplateInput::from_playlist(playlist, index));
        self
    }

    /// Adds normalized playlist fields to the playlist namespace.
    pub fn with_playlist_input(mut self, playlist: PlaylistTemplateInput) -> Self {
        self.playlist = Some(playlist);
        self
    }

    /// Adds an effective provider quality label for template decisions.
    pub fn with_effective_quality_label(mut self, label: Option<ProviderQualityLabel>) -> Self {
        self.effective_quality = label;
        self
    }

    /// Adds a top-level extra field.
    pub fn with_extra(mut self, name: ExtraFieldName, value: TemplateValue<'static>) -> Self {
        self.extras.insert(name, value);
        self
    }

    /// Returns the `now` value.
    pub const fn now(&self) -> time::OffsetDateTime {
        self.now
    }

    pub(crate) const fn item(&self) -> Option<&ItemTemplateInput> {
        self.item.as_ref()
    }

    pub(crate) const fn album(&self) -> Option<&AlbumTemplateInput> {
        self.album.as_ref()
    }

    pub(crate) const fn playlist(&self) -> Option<&PlaylistTemplateInput> {
        self.playlist.as_ref()
    }

    pub(crate) const fn effective_quality(&self) -> Option<&ProviderQualityLabel> {
        self.effective_quality.as_ref()
    }

    pub(crate) fn extra(&self, name: &ExtraFieldName) -> TemplateValue<'_> {
        self.extras
            .get(name)
            .cloned()
            .unwrap_or(TemplateValue::None)
    }
}

/// Normalized item template fields.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ItemTemplateInput {
    /// Item kind.
    pub kind: String,
    /// Provider item ID.
    pub id: String,
    /// Title.
    pub title: String,
    /// Track number.
    pub number: i64,
    /// Disc/volume number.
    pub volume: i64,
    /// Version text.
    #[serde(default)]
    pub version: String,
    /// Copyright text.
    #[serde(default)]
    pub copyright: String,
    /// BPM, defaulting to `0` when the provider omits it.
    #[serde(default)]
    pub bpm: i64,
    /// ISRC.
    #[serde(default)]
    pub isrc: String,
    /// Provider quality label.
    #[serde(default)]
    pub quality: String,
    /// Primary artist display name.
    #[serde(default)]
    pub artist: String,
    /// Sorted main artists.
    #[serde(default)]
    pub artists: String,
    /// Sorted featured artists.
    #[serde(default)]
    pub features: String,
    /// Sorted main + featured artists.
    #[serde(default)]
    pub artists_with_features: String,
    /// Explicit flag.
    pub explicit: Option<bool>,
    /// Dolby Atmos flag.
    #[serde(default)]
    pub dolby: bool,
}

impl ItemTemplateInput {
    fn from_track(track: &Track) -> Self {
        let mut main = artist_names_by_role(&track.artists, ArtistRole::Main);
        let mut featured = artist_names_by_role(&track.artists, ArtistRole::Featured);
        main.sort();
        featured.sort();
        let artist = track
            .artists
            .iter()
            .find(|credit| credit.role == ArtistRole::Main)
            .or_else(|| track.artists.first())
            .map(|credit| credit.artist.name.clone())
            .unwrap_or_default();
        Self {
            kind: "track".to_owned(),
            id: track.id.id.as_str().to_owned(),
            title: track.title.clone(),
            number: track.track_number.map_or(0, |n| i64::from(n.get())),
            volume: track.disc_number.map_or(0, |n| i64::from(n.get())),
            version: track.version.clone().unwrap_or_default(),
            copyright: track.copyright.clone().unwrap_or_default(),
            bpm: track.bpm.unwrap_or(0),
            isrc: track
                .external_ids
                .0
                .get(&riptide_core::ExternalIdKind::Isrc)
                .cloned()
                .unwrap_or_default(),
            quality: track.quality_label.clone().unwrap_or_default(),
            artist,
            artists: main.join(", "),
            features: featured.join(", "),
            artists_with_features: main
                .iter()
                .chain(featured.iter())
                .cloned()
                .collect::<Vec<_>>()
                .join(", "),
            explicit: track.explicit,
            dolby: track.dolby_atmos,
        }
    }

    fn from_video(video: &Video) -> Self {
        let mut main = artist_names_by_role(&video.artists, ArtistRole::Main);
        let mut featured = artist_names_by_role(&video.artists, ArtistRole::Featured);
        main.sort();
        featured.sort();
        let artist = video
            .artists
            .iter()
            .find(|credit| credit.role == ArtistRole::Main)
            .or_else(|| video.artists.first())
            .map(|credit| credit.artist.name.clone())
            .unwrap_or_default();
        Self {
            kind: "video".to_owned(),
            id: video.id.id.as_str().to_owned(),
            title: video.title.clone(),
            number: video.track_number.map_or(0, |n| i64::from(n.get())),
            volume: video.disc_number.map_or(0, |n| i64::from(n.get())),
            version: String::new(),
            copyright: String::new(),
            bpm: 0,
            isrc: String::new(),
            quality: video.quality_label.clone().unwrap_or_default(),
            artist,
            artists: main.join(", "),
            features: featured.join(", "),
            artists_with_features: main
                .iter()
                .chain(featured.iter())
                .cloned()
                .collect::<Vec<_>>()
                .join(", "),
            explicit: video.explicit,
            dolby: false,
        }
    }

    pub(crate) fn field(&self, field: &str) -> TemplateValue<'_> {
        match field {
            "id" => TemplateValue::Text(Cow::Borrowed(&self.id)),
            "title" => TemplateValue::Text(Cow::Borrowed(&self.title)),
            "title_version" => {
                if self.version.is_empty() {
                    TemplateValue::Text(Cow::Borrowed(&self.title))
                } else {
                    TemplateValue::Text(Cow::Owned(format!("{} ({})", self.title, self.version)))
                }
            }
            "number" => TemplateValue::Integer(self.number),
            "volume" => TemplateValue::Integer(self.volume),
            "version" => TemplateValue::Text(Cow::Borrowed(&self.version)),
            "copyright" => TemplateValue::Text(Cow::Borrowed(&self.copyright)),
            "bpm" => TemplateValue::Integer(self.bpm),
            "isrc" => TemplateValue::Text(Cow::Borrowed(&self.isrc)),
            "quality" => TemplateValue::Text(Cow::Borrowed(&self.quality)),
            "artist" => TemplateValue::Text(Cow::Borrowed(&self.artist)),
            "artists" => TemplateValue::Text(Cow::Borrowed(&self.artists)),
            "features" => TemplateValue::Text(Cow::Borrowed(&self.features)),
            "artists_with_features" => {
                TemplateValue::Text(Cow::Borrowed(&self.artists_with_features))
            }
            "explicit" => TemplateValue::Explicit(self.explicit),
            "dolby" => TemplateValue::Conditional(self.dolby),
            _ => TemplateValue::None,
        }
    }
}

/// Normalized album template fields.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AlbumTemplateInput {
    /// Provider album ID.
    pub id: String,
    /// Album title.
    pub title: String,
    /// Primary artist.
    #[serde(default)]
    pub artist: String,
    /// Album artists.
    #[serde(default)]
    pub artists: String,
    /// Release date.
    #[serde(default, deserialize_with = "deserialize_optional_date")]
    pub date: Option<time::Date>,
    /// Explicit flag.
    pub explicit: Option<bool>,
    /// High-resolution/master flag from provider tags.
    #[serde(default)]
    pub master: bool,
    /// Release type.
    #[serde(default)]
    pub release: String,
}

impl AlbumTemplateInput {
    fn from_album(album: &Album) -> Self {
        let mut main = album
            .artists
            .iter()
            .filter(|credit| credit.role == ArtistRole::Main)
            .map(|credit| credit.artist.name.clone())
            .collect::<Vec<_>>();
        main.sort();
        let artist = album
            .artists
            .iter()
            .find(|credit| credit.role == ArtistRole::Main)
            .or_else(|| album.artists.first())
            .map(|credit| credit.artist.name.clone())
            .unwrap_or_default();
        Self {
            id: album.id.id.as_str().to_owned(),
            title: album.title.clone(),
            artist,
            artists: main.join(", "),
            date: album.release_date,
            explicit: album.explicit,
            master: album.hires_lossless,
            release: album.release_type.clone().unwrap_or_default(),
        }
    }

    pub(crate) fn field(
        &self,
        field: &str,
        effective_quality: Option<&ProviderQualityLabel>,
    ) -> TemplateValue<'_> {
        match field {
            "id" => TemplateValue::Text(Cow::Borrowed(&self.id)),
            "title" => TemplateValue::Text(Cow::Borrowed(&self.title)),
            "artist" => TemplateValue::Text(Cow::Borrowed(&self.artist)),
            "artists" => TemplateValue::Text(Cow::Borrowed(&self.artists)),
            "date" => self
                .date
                .map_or(TemplateValue::None, TemplateValue::Date),
            "explicit" => TemplateValue::Explicit(self.explicit),
            "master" => TemplateValue::Conditional(
                self.master
                    && effective_quality
                        .is_some_and(|label| label.as_str() == "MAX"),
            ),
            "release" => TemplateValue::Text(Cow::Borrowed(&self.release)),
            _ => TemplateValue::None,
        }
    }
}

/// Normalized playlist template fields.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PlaylistTemplateInput {
    /// Playlist UUID.
    pub uuid: String,
    /// Playlist title.
    pub title: String,
    /// One-based index.
    pub index: u32,
    /// Creation time.
    #[serde(default, deserialize_with = "deserialize_optional_offset_datetime")]
    pub created: Option<time::OffsetDateTime>,
    /// Last update time.
    #[serde(default, deserialize_with = "deserialize_optional_offset_datetime")]
    pub updated: Option<time::OffsetDateTime>,
}

impl PlaylistTemplateInput {
    fn from_playlist(playlist: &Playlist, index: PlaylistIndex) -> Self {
        Self {
            uuid: playlist.id.id.as_str().to_owned(),
            title: playlist.title.clone(),
            index: index.get(),
            created: playlist.created_at,
            updated: playlist.updated_at,
        }
    }

    pub(crate) fn field(&self, field: &str) -> TemplateValue<'_> {
        match field {
            "uuid" => TemplateValue::Text(Cow::Borrowed(&self.uuid)),
            "title" => TemplateValue::Text(Cow::Borrowed(&self.title)),
            "index" => TemplateValue::Integer(i64::from(self.index)),
            "created" => self
                .created
                .map_or(TemplateValue::None, TemplateValue::DateTime),
            "updated" => self
                .updated
                .map_or(TemplateValue::None, TemplateValue::DateTime),
            _ => TemplateValue::None,
        }
    }
}

fn artist_names_by_role(credits: &[ArtistCredit], role: ArtistRole) -> Vec<String> {
    credits
        .iter()
        .filter(|credit| credit.role == role)
        .map(|credit| credit.artist.name.clone())
        .collect()
}

/// JSON fixture schema for deterministic template rendering tests.
#[derive(Debug, Deserialize)]
pub struct FixtureContext {
    /// Stable timestamp used for `{now}`.
    #[serde(deserialize_with = "deserialize_offset_datetime")]
    pub now: time::OffsetDateTime,
    /// Optional normalized item namespace.
    pub item: Option<ItemTemplateInput>,
    /// Optional normalized album namespace.
    pub album: Option<AlbumTemplateInput>,
    /// Optional normalized playlist namespace.
    pub playlist: Option<PlaylistTemplateInput>,
    /// Top-level extra fields.
    #[serde(default)]
    pub extras: BTreeMap<ExtraFieldName, Option<serde_json::Value>>,
}

fn deserialize_offset_datetime<'de, D>(deserializer: D) -> Result<time::OffsetDateTime, D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    time::OffsetDateTime::parse(&value, &time::format_description::well_known::Rfc3339)
        .map_err(serde::de::Error::custom)
}

fn deserialize_optional_offset_datetime<'de, D>(
    deserializer: D,
) -> Result<Option<time::OffsetDateTime>, D::Error>
where
    D: Deserializer<'de>,
{
    let Some(value) = Option::<String>::deserialize(deserializer)? else {
        return Ok(None);
    };
    time::OffsetDateTime::parse(&value, &time::format_description::well_known::Rfc3339)
        .map(Some)
        .map_err(serde::de::Error::custom)
}

fn deserialize_optional_date<'de, D>(deserializer: D) -> Result<Option<time::Date>, D::Error>
where
    D: Deserializer<'de>,
{
    let Some(value) = Option::<String>::deserialize(deserializer)? else {
        return Ok(None);
    };
    let format = time::format_description::parse("[year]-[month]-[day]")
        .map_err(serde::de::Error::custom)?;
    time::Date::parse(&value, &format)
        .map(Some)
        .map_err(serde::de::Error::custom)
}

impl FixtureContext {
    /// Converts fixture data into a render context.
    pub fn into_context(self) -> TemplateContext {
        let mut context = TemplateContext::new(self.now);
        if let Some(item) = self.item {
            context = context.with_item_input(item);
        }
        if let Some(album) = self.album {
            context = context.with_album_input(album);
        }
        if let Some(playlist) = self.playlist {
            context = context.with_playlist_input(playlist);
        }
        for (name, value) in self.extras {
            let value = match value {
                Some(serde_json::Value::String(value)) => TemplateValue::Text(Cow::Owned(value)),
                Some(serde_json::Value::Number(value)) => {
                    TemplateValue::Integer(value.as_i64().unwrap_or_default())
                }
                Some(serde_json::Value::Bool(value)) => TemplateValue::Conditional(value),
                _ => TemplateValue::None,
            };
            context = context.with_extra(name, value.into_owned());
        }
        context
    }
}

/// Parses a JSON fixture into a render context.
pub fn context_from_fixture_str(input: &str) -> TemplateResult<TemplateContext> {
    serde_json::from_str::<FixtureContext>(input)
        .map(FixtureContext::into_context)
        .map_err(|error| TemplateError::FixtureDecode {
            message: error.to_string(),
        })
}

trait IntoOwnedTemplateValue {
    fn into_owned(self) -> TemplateValue<'static>;
}

impl IntoOwnedTemplateValue for TemplateValue<'_> {
    fn into_owned(self) -> TemplateValue<'static> {
        match self {
            TemplateValue::Text(value) => TemplateValue::Text(Cow::Owned(value.into_owned())),
            TemplateValue::Integer(value) => TemplateValue::Integer(value),
            TemplateValue::Explicit(value) => TemplateValue::Explicit(value),
            TemplateValue::Conditional(value) => TemplateValue::Conditional(value),
            TemplateValue::Date(value) => TemplateValue::Date(value),
            TemplateValue::DateTime(value) => TemplateValue::DateTime(value),
            TemplateValue::None => TemplateValue::None,
        }
    }
}
