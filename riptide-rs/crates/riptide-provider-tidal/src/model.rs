//! TIDAL JSON response models.

use serde::Deserialize;

/// TIDAL session response.
#[derive(Debug, Clone, Deserialize)]
pub struct SessionResponse {
    /// User ID.
    #[serde(rename = "userId")]
    pub user_id: i64,
    /// Country code.
    #[serde(rename = "countryCode")]
    pub country_code: String,
}

/// TIDAL artist.
#[derive(Debug, Clone, Deserialize)]
pub struct TidalArtist {
    /// Artist ID.
    pub id: Option<i64>,
    /// Artist name.
    pub name: String,
    /// Artist role type.
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
}

/// TIDAL media metadata.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct MediaMetadata {
    /// Tags.
    #[serde(default)]
    pub tags: Vec<String>,
}

/// TIDAL album summary.
#[derive(Debug, Clone, Deserialize)]
pub struct TidalAlbumSummary {
    /// Album ID.
    pub id: Option<i64>,
    /// Album title.
    pub title: Option<String>,
}

/// TIDAL track.
#[derive(Debug, Clone, Deserialize)]
pub struct TidalTrack {
    /// Track ID.
    pub id: i64,
    /// Title.
    pub title: String,
    /// Duration seconds.
    pub duration: Option<u64>,
    /// Track number.
    #[serde(default, rename = "trackNumber")]
    pub track_number: Option<u32>,
    /// Volume number.
    #[serde(default, rename = "volumeNumber")]
    pub volume_number: Option<u32>,
    /// Version.
    #[serde(default)]
    pub version: Option<String>,
    /// Copyright.
    #[serde(default)]
    pub copyright: Option<String>,
    /// BPM.
    #[serde(default)]
    pub bpm: Option<i64>,
    /// ISRC.
    #[serde(default)]
    pub isrc: Option<String>,
    /// Explicit flag.
    #[serde(default)]
    pub explicit: Option<bool>,
    /// Audio quality.
    #[serde(default, rename = "audioQuality")]
    pub audio_quality: Option<String>,
    /// Main artist.
    #[serde(default)]
    pub artist: Option<TidalArtist>,
    /// Artists.
    #[serde(default)]
    pub artists: Vec<TidalArtist>,
    /// Album summary.
    #[serde(default)]
    pub album: Option<TidalAlbumSummary>,
    /// Media metadata.
    #[serde(default, rename = "mediaMetadata")]
    pub media_metadata: MediaMetadata,
}

/// TIDAL video.
#[derive(Debug, Clone, Deserialize)]
pub struct TidalVideo {
    /// Video ID.
    pub id: i64,
    /// Title.
    pub title: String,
    /// Duration seconds.
    pub duration: Option<u64>,
    /// Track number.
    #[serde(default, rename = "trackNumber")]
    pub track_number: Option<u32>,
    /// Volume number.
    #[serde(default, rename = "volumeNumber")]
    pub volume_number: Option<u32>,
    /// Explicit flag.
    #[serde(default)]
    pub explicit: Option<bool>,
    /// Quality.
    #[serde(default)]
    pub quality: Option<String>,
    /// Main artist.
    #[serde(default)]
    pub artist: Option<TidalArtist>,
    /// Artists.
    #[serde(default)]
    pub artists: Vec<TidalArtist>,
    /// Album summary.
    #[serde(default)]
    pub album: Option<TidalAlbumSummary>,
}

/// TIDAL album.
#[derive(Debug, Clone, Deserialize)]
pub struct TidalAlbum {
    /// Album ID.
    pub id: i64,
    /// Title.
    pub title: String,
    /// Release date.
    #[serde(default, rename = "releaseDate")]
    pub release_date: Option<String>,
    /// Track count.
    #[serde(default, rename = "numberOfTracks")]
    pub number_of_tracks: Option<u32>,
    /// Video count.
    #[serde(default, rename = "numberOfVideos")]
    pub number_of_videos: Option<u32>,
    /// Explicit flag.
    #[serde(default)]
    pub explicit: Option<bool>,
    /// Release type.
    #[serde(default, rename = "type")]
    pub release_type: Option<String>,
    /// Main artist.
    #[serde(default)]
    pub artist: Option<TidalArtist>,
    /// Artists.
    #[serde(default)]
    pub artists: Vec<TidalArtist>,
    /// Media metadata.
    #[serde(default, rename = "mediaMetadata")]
    pub media_metadata: MediaMetadata,
}

/// TIDAL playlist.
#[derive(Debug, Clone, Deserialize)]
pub struct TidalPlaylist {
    /// Playlist UUID.
    pub uuid: String,
    /// Title.
    pub title: String,
    /// Track count.
    #[serde(default, rename = "numberOfTracks")]
    pub number_of_tracks: Option<u32>,
    /// Video count.
    #[serde(default, rename = "numberOfVideos")]
    pub number_of_videos: Option<u32>,
    /// Creation timestamp string.
    #[serde(default)]
    pub created: Option<String>,
    /// Last update timestamp string.
    #[serde(default, rename = "lastUpdated")]
    pub last_updated: Option<String>,
}

/// Paginated wrapper.
#[derive(Debug, Clone, Deserialize)]
#[serde(bound(deserialize = "T: Deserialize<'de>"))]
pub struct Items<T> {
    /// Limit.
    pub limit: u32,
    /// Offset.
    pub offset: u32,
    /// Items.
    #[serde(default)]
    pub items: Vec<T>,
}

impl<T> Default for Items<T> {
    fn default() -> Self {
        Self {
            limit: 0,
            offset: 0,
            items: Vec::new(),
        }
    }
}

/// TIDAL item wrapper.
#[derive(Debug, Clone, Deserialize)]
pub struct ItemWrapper<T> {
    /// Wrapped item.
    pub item: T,
}

/// Search response.
#[derive(Debug, Clone, Deserialize)]
pub struct TidalSearch {
    /// Tracks.
    #[serde(default)]
    pub tracks: Items<TidalTrack>,
    /// Videos.
    #[serde(default)]
    pub videos: Items<TidalVideo>,
    /// Albums.
    #[serde(default)]
    pub albums: Items<TidalAlbum>,
    /// Playlists.
    #[serde(default)]
    pub playlists: Items<TidalPlaylist>,
    /// Artists.
    #[serde(default)]
    pub artists: Items<TidalArtist>,
    /// Top hit.
    #[serde(default, rename = "topHit")]
    pub top_hit: Option<TopHit>,
}

/// Search top hit.
#[derive(Debug, Clone, Deserialize)]
pub struct TopHit {
    /// Hit type.
    #[serde(rename = "type")]
    pub kind: String,
    /// Raw value.
    pub value: serde_json::Value,
}

/// Lyrics response.
#[derive(Debug, Clone, Deserialize)]
pub struct TidalLyrics {
    /// Plain lyrics.
    #[serde(default)]
    pub lyrics: Option<String>,
    /// LRC/subtitles.
    #[serde(default)]
    pub subtitles: Option<String>,
    /// Provider attribution.
    #[serde(default, rename = "lyricsProvider")]
    pub lyrics_provider: Option<String>,
}
