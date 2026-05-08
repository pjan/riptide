# Slice 4+5: Media Download Pipeline and Metadata

## Overview

Slices 4 and 5 are combined because the download pipeline and metadata tagging
are tightly coupled — a downloaded file is useless without metadata, and metadata
writing needs the download output path and container format. Implementing them
together avoids temporary throwaway interfaces.

## Goal

Implement the complete path from "resolved stream variant" to "tagged file on
disk with lyrics, cover art, and M3U playlist."

## What Already Exists (from Slices 0-3)

- `riptide-core`: `StreamVariant`, `Manifest`, `ContainerKind`, `PostProcessStep`,
  `Lyrics`, `LyricsSource`, `Track`, `Video`, `Album`, `Playlist` with typed fields
- `riptide-provider-tidal`: `TidalClient` (read-only API), `TokenStore`/`ApiCache`
  traits, typed model enums, structured OAuth errors
- `riptide-template`: `CompiledTemplate`, `TemplateContext`, path sanitization
- `riptide-config`: `ValidatedConfig` with `audio_quality`/`video_quality`

## What Needs To Be Built

### New Crate: `riptide-media`

Provider-neutral manifest resolution, segment download with streaming I/O,
ffmpeg post-processing, and download plan construction.

### New Crate: `riptide-metadata`

Provider-neutral metadata tagging (FLAC, M4A, MP4), cover art, lyrics sidecar,
M3U playlists.

### Extensions to Existing Crates

- `riptide-core`: Add provider-neutral `ContainerExtension` enum used by media
  and metadata crates
- `riptide-provider-tidal`: Stream endpoints, TIDAL-specific manifest decoding
  into `StreamVariant`, cover art ID type
- `riptide-config`: Download, metadata, and M3U config sections with typed
  validation
- `riptide-cli`: Download command

---

## Manifest Ownership Contract

TIDAL-specific manifest knowledge lives in `riptide-provider-tidal`.
Provider-neutral manifest resolution lives in `riptide-media`.

**Provider crate responsibility:**
1. Call TIDAL stream endpoint → raw response (base64 manifest + MIME type)
2. Base64-decode the manifest
3. Route by MIME type:
   - `application/vnd.tidal.bts`: Parse BTS JSON, extract URLs and codec
   - `application/dash+xml`: Keep as raw XML string
   - Video manifests: Keep the M3U8 master URL
4. Return a `StreamVariant` with the appropriate `Manifest` variant:
   - BTS → `Manifest::DirectSegments(Vec<Url>)`
   - DASH → `Manifest::DashXml(String)`
   - Video → `Manifest::HlsMasterPlaylistUrl(Url)`

**Media crate responsibility:**
1. Accept a `Manifest` variant
2. Resolve it to concrete segment URLs:
   - `DirectSegments` → pass through
   - `DashXml` → parse DASH MPD XML, construct segment URLs (pure, sync)
   - `HlsMasterPlaylistUrl` → fetch master, select variant, fetch media
     playlist, extract segment URLs (async, requires HTTP client)
   - `HlsMediaPlaylist` → parse directly (pure, sync)
3. Download segments to temp file with streaming I/O

This means `decode_video_stream` in the provider is **not** synchronous — it
returns `Manifest::HlsMasterPlaylistUrl(url)` and the async HLS resolution
happens in `riptide-media`.

---

## Data Flow

```
1. User: `riptide-rs download https://listen.tidal.com/album/12345`

2. Resolve (existing):
   TidalResource::parse(url) → MediaRef
   TidalProvider::resolve(ref) → CatalogEntity::Album
   TidalProvider::children(ref) → Vec<CatalogEntity>

3. Stream (provider crate):
   TidalProvider::streams(ref, quality) → Vec<StreamVariant>
   ─ internally: TIDAL API → base64 decode → BTS/DASH/HLS routing
   ─ returns: StreamVariant { manifest: Manifest, container, codec, ... }

4. Plan (riptide-media, pure):
   stream_variant + template_context → DownloadPlan
   ─ computes output path, download extension, final extension
   ─ determines post-process steps from container + codec

5. Resolve manifest (riptide-media):
   For each plan (bounded concurrency via semaphore):
     DownloadPlan.manifest → ResolvedDownloadPlan
     ─ pure sync for DirectSegments, DashXml, HlsMediaPlaylist
     ─ async I/O only for HlsMasterPlaylistUrl

6. Download (riptide-media, async I/O):
   ResolvedDownloadPlan.segments → temp file

7. Post-process (riptide-media, subprocess I/O):
   temp file + steps → processed temp file

8. Tag (riptide-metadata, blocking I/O via spawn_blocking):
   processed temp file + FileMetadata → tagged temp file

9. Persist (filesystem):
   tagged temp file → final path (atomic rename)
   sidecars → sidecar paths (atomic rename)

10. Collection outputs (riptide-metadata):
   write_m3u(entries, output)
   save_cover(cover_url, directory)
```

---

## Transaction Model

A downloaded item is **all-or-nothing**. An untagged file must never appear at
the final path.

```
target_dir/.riptide-tmp-XXXXX/
  .riptide-download-XXXXX.m4a     ← segment download target
  .riptide-download-XXXXX.flac    ← after ffmpeg extract_flac
  .riptide-download-XXXXX.flac    ← after tag writing

final_dir/
  Artist/Album/01 Track.flac      ← atomic persist (rename) only after
                                     download + post-process + tagging all succeed
```

Steps:
1. Create a `TempDir` in the **target directory** (same filesystem for atomic
   rename).
2. Download segments → write to a temp path inside that temp directory.
3. Post-process (ffmpeg) → creates a new temp path when the container changes.
   The post-process function returns the path that should continue through the
   pipeline. Intermediate source files are removed only after the replacement
   exists and the step succeeds.
4. Write tags → `spawn_blocking` with `lofty` on the current temp path.
5. Atomic rename → moves the current temp path to the final path.
6. Write sidecars (`.lrc`, cover `.jpg`) using same-directory temp files and
   atomic rename.

If any step fails, the temp directory is cleaned up. The final path is never
touched until download, post-process, and tagging all succeed.

---

## Detailed Design: `riptide-provider-tidal` Extensions

### Stream Endpoint

```rust
impl TidalProvider {
    // Implements StreamProvider::streams from riptide-core
}

// New API methods on TidalClient
impl TidalClient {
    pub async fn track_stream(
        &self,
        reference: &MediaRef,
        quality: AudioQualityRequest,
    ) -> TidalResult<StreamVariant>;

    pub async fn video_stream(
        &self,
        reference: &MediaRef,
        quality: VideoQualityRequest,
    ) -> TidalResult<StreamVariant>;
}
```

Parameters use `&MediaRef` (not `&str`) to enforce typed boundaries.

### TIDAL Manifest Decoding (internal)

```rust
/// TIDAL raw stream API response.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct TidalStreamResponse {
    manifest: String, // base64-encoded
    manifest_mime_type: TidalManifestMime,
    audio_quality: Option<String>,
}

/// Stream manifest MIME discriminator parsed at the serde boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
enum TidalManifestMime {
    #[serde(rename = "application/vnd.tidal.bts")]
    Bts,
    #[serde(rename = "application/dash+xml")]
    Dash,
    #[serde(other)]
    Other,
}

// `audio_quality` remains `Option<String>` at the raw response boundary because
// TIDAL quality labels are open-ended provider labels. It is converted
// immediately to the existing typed quality model or `ProviderQualityLabel`.

/// Decodes a TIDAL track stream response into a provider-neutral StreamVariant.
///
/// This function handles base64 decoding and MIME routing but does NOT
/// resolve DASH/HLS manifests — it wraps them in the appropriate Manifest
/// variant for riptide-media to resolve.
fn decode_track_manifest(
    response: &TidalStreamResponse,
    source: &MediaRef,
    requested_quality: AudioQualityRequest,
) -> TidalResult<StreamVariant>;

fn decode_video_manifest(
    response: &TidalVideoStreamResponse,
    source: &MediaRef,
) -> TidalResult<StreamVariant>;
```

Codec-to-container-to-postprocess mapping (pure logic):
- `flac` at non-HiRes → container `Flac`, no post-process
- `flac` at HiRes → container `M4a`, post-process `[ExtractFlac, RemoveSource]`
- `mp4a.*` → container `M4a`, no post-process
- Video → container `TransportStream`, post-process `[RemuxToMp4, RemoveSource]`

### Cover Art

```rust
/// TIDAL cover art identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CoverArtId(String);

impl CoverArtId {
    pub fn new(uid: impl Into<String>) -> TidalResult<Self>;
    pub fn as_str(&self) -> &str;
}

/// Requested cover art dimension (square).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoverArtSize(u16);

impl CoverArtSize {
    /// Creates a size, capping at 1280.
    pub fn new(size: u16) -> Self;
    pub fn get(self) -> u16;
}

impl Default for CoverArtSize {
    fn default() -> Self { Self(1280) }
}

/// Constructs a TIDAL cover art URL.
pub fn cover_url(id: &CoverArtId, size: CoverArtSize) -> Url;
```

---

## Detailed Design: `riptide-media`

### Crate Structure

```
riptide-media/
  src/
    lib.rs
    error.rs
    manifest.rs      — DASH XML and HLS parsing + manifest resolution
    download.rs      — Segment fetcher with streaming I/O
    ffmpeg.rs        — FFmpeg subprocess wrapper
    plan.rs          — Download plan construction (pure)
```

### Dependencies

```toml
[dependencies]
dash-mpd.workspace = true
futures-util.workspace = true
m3u8-rs.workspace = true
reqwest.workspace = true
riptide-core = { path = "../riptide-core" }
tempfile.workspace = true
thiserror.workspace = true
tokio.workspace = true
url.workspace = true
```

### Error Type

```rust
#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("failed to parse {format} manifest: {message}")]
    ManifestParse {
        format: &'static str,
        message: String,
    },

    #[error("failed to download segment {segment_index}")]
    SegmentDownload {
        segment_index: usize,
        #[source]
        source: reqwest::Error,
    },

    #[error("ffmpeg was not found on PATH")]
    FfmpegNotFound,

    #[error("ffmpeg {operation} failed with exit code {exit_code:?}: {stderr}")]
    FfmpegFailed {
        operation: &'static str,
        exit_code: Option<i32>,
        stderr: String,
    },

    #[error("I/O error while {operation}")]
    Io {
        operation: &'static str,
        #[source]
        source: std::io::Error,
    },
}

pub type MediaResult<T> = Result<T, MediaError>;
```

### Manifest Resolution

```rust
/// Resolved manifest: concrete segment URLs ready for download.
pub struct ResolvedSegments {
    pub urls: Vec<Url>,
    pub codec: Option<Codec>,
}

/// Resolves provider-neutral manifest variants that require no I/O.
///
/// Returns `None` when the manifest requires async HTTP, such as
/// `Manifest::HlsMasterPlaylistUrl`.
pub fn resolve_manifest_sync(manifest: &Manifest) -> MediaResult<Option<ResolvedSegments>>;

/// Resolves provider-neutral manifest variants that require async HTTP.
///
/// This is currently only needed for HLS master playlist URLs. Callers should
/// use `resolve_manifest_sync` first so pure manifest variants stay visibly
/// pure and testable.
pub async fn resolve_manifest_async(
    manifest: &Manifest,
    http: &reqwest::Client,
) -> MediaResult<ResolvedSegments>;

/// Parses a DASH MPD XML document into segment URLs and codec info.
/// Pure function — no I/O.
pub fn parse_dash_mpd(xml: &str) -> MediaResult<ResolvedSegments>;

/// Parses an HLS media playlist into segment URLs.
/// Pure function — no I/O.
pub fn parse_hls_media_playlist(
    content: &str,
    base_url: &Url,
) -> MediaResult<Vec<Url>>;
```

### Segment Download

```rust
/// Segment download behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DownloadOptions {
    pub segment_timeout: std::time::Duration,
    /// Retry attempts per segment after the first failed attempt.
    ///
    /// Keep this in the small range 0-5. The default is 3. This is not exposed
    /// as user config in this slice; if it becomes configurable, introduce a
    /// validating newtype at the config boundary.
    pub max_retries_per_segment: u8,
}

impl Default for DownloadOptions {
    fn default() -> Self {
        Self {
            segment_timeout: std::time::Duration::from_secs(30),
            max_retries_per_segment: 3,
        }
    }
}

/// Downloads segments to a file with streaming I/O.
///
/// Opens `output_path`, wraps it in `tokio::io::BufWriter`, and streams each
/// segment's response body directly to disk. Never buffers an entire segment in
/// memory.
///
/// Segment retries discard the partially written current attempt before retrying
/// that segment. If all retries fail, the caller removes the temp work directory.
pub async fn download_segments(
    http: &reqwest::Client,
    segments: &[Url],
    output_path: &Path,
    options: DownloadOptions,
    progress: Option<&dyn DownloadProgress>,
) -> MediaResult<()>;

/// Progress callback for download tracking.
pub trait DownloadProgress: Send + Sync {
    fn on_segment_start(
        &self,
        segment_index: usize,
        total_segments: usize,
        content_length: Option<u64>,
    );
    fn on_bytes(&self, bytes_downloaded: u64);
}
```

Do not pre-flight `HEAD` every segment just to compute a total byte count. The
progress UI may show bytes downloaded and per-segment progress; percentage is
optional unless the response provides enough content-length data naturally.

### FFmpeg Post-Processing

```rust
/// Checks whether ffmpeg is available on PATH.
pub async fn ffmpeg_available() -> bool;

/// Extracts FLAC from an M4A container (stream copy, no re-encoding).
/// Returns the path to the extracted file.
pub async fn extract_flac(source: &Path) -> MediaResult<PathBuf>;

/// Remuxes a transport stream to MP4 (stream copy, no re-encoding).
/// Returns the path to the remuxed file.
pub async fn remux_to_mp4(source: &Path) -> MediaResult<PathBuf>;

/// Applies a sequence of post-processing steps to a file.
/// Returns the final file path (may differ from input if container changed).
pub async fn post_process(
    path: PathBuf,
    steps: &[PostProcessStep],
) -> MediaResult<PathBuf>;

/// Constructs the ffmpeg Command for a given operation without executing it.
/// Exposed for testing.
pub fn ffmpeg_extract_flac_command(source: &Path, output: &Path) -> std::process::Command;
pub fn ffmpeg_remux_to_mp4_command(source: &Path, output: &Path) -> std::process::Command;
```

All ffmpeg operations use `-c copy` (stream copy) — no re-encoding.
FFmpeg is executed via `tokio::process::Command`.

### Download Plan (Pure)

```rust
/// Pure plan for downloading one media item.
///
/// Constructing this type must not perform network or filesystem I/O.
pub struct DownloadPlan {
    /// Where to write the final file (without extension).
    pub output_stem: PathBuf,
    /// Final file extension after all post-processing.
    pub final_extension: ContainerExtension,
    /// Provider-neutral manifest, not yet resolved to segment URLs.
    pub manifest: Manifest,
    /// Post-processing steps to apply after download.
    pub post_process: Vec<PostProcessStep>,
    /// Intermediate extension during download (before post-processing).
    pub download_extension: ContainerExtension,
}

/// Plan after manifest resolution, ready for segment download.
pub struct ResolvedDownloadPlan {
    pub plan: DownloadPlan,
    pub segments: Vec<Url>,
}

/// Typed file extension derived from container kind and post-processing.
///
/// Lives in `riptide-core` so both `riptide-media` and `riptide-metadata` can
/// depend on it without creating a crate cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerExtension {
    Flac,
    M4a,
    Mp4,
    Ts,
}

impl ContainerExtension {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Flac => ".flac",
            Self::M4a => ".m4a",
            Self::Mp4 => ".mp4",
            Self::Ts => ".ts",
        }
    }
}

impl DownloadPlan {
    /// The final output path after all post-processing.
    pub fn final_path(&self) -> PathBuf;
    /// The intermediate download path.
    pub fn download_path(&self) -> PathBuf;
}
```

`DownloadPlan` is a pure data struct. Filesystem checks (`exists`) are the
caller's responsibility — the plan does not perform I/O.

---

## Detailed Design: `riptide-metadata`

### Crate Structure

```
riptide-metadata/
  src/
    lib.rs
    error.rs
    tags.rs           — Metadata model and tag writing
    cover.rs          — Cover art fetching and saving
    lyrics.rs         — Lyrics sidecar writing
    m3u.rs            — M3U playlist generation
```

### Dependencies

```toml
[dependencies]
lofty.workspace = true
riptide-core = { path = "../riptide-core" }
thiserror.workspace = true
time.workspace = true
tokio.workspace = true
```

Note: `reqwest` is NOT a dependency of riptide-metadata. Cover art bytes are
fetched by the caller and passed in. The metadata crate is I/O-minimal —
only filesystem writes for tags, sidecars, and playlists.

### Error Type

```rust
#[derive(Debug, thiserror::Error)]
pub enum MetadataError {
    #[error("unsupported metadata container: {extension:?}")]
    UnsupportedContainer { extension: ContainerExtension },

    #[error("failed to write tags")]
    TagWrite {
        #[source]
        source: lofty::error::LoftyError,
    },

    #[error("I/O error while {operation}")]
    Io {
        operation: &'static str,
        #[source]
        source: std::io::Error,
    },
}

pub type MetadataResult<T> = Result<T, MetadataError>;
```

### Metadata Model

```rust
/// Metadata to write to a media file.
///
/// Uses typed core values until the tag-writer boundary, where they are
/// formatted into strings for the specific container format.
pub struct FileMetadata {
    pub title: String,
    pub artist: String,
    pub artists: Vec<String>,
    pub album_title: Option<String>,
    pub album_artist: Option<String>,
    pub track_number: Option<TrackNumber>,
    pub disc_number: Option<DiscNumber>,
    pub date: Option<time::Date>,
    pub copyright: Option<String>,
    pub isrc: Option<String>,
    pub bpm: Option<i64>,
    pub lyrics: Option<String>,
    pub cover_data: Option<std::sync::Arc<[u8]>>,
    pub comment: Option<String>,
}

/// Builds FileMetadata from a core Track and its album context.
pub fn metadata_from_track(
    track: &Track,
    album: Option<&Album>,
) -> FileMetadata;

/// Builds FileMetadata from a core Video.
pub fn metadata_from_video(video: &Video) -> FileMetadata;
```

`metadata_from_track` and `metadata_from_video` are pure domain mapping
functions. The orchestration layer attaches I/O-sourced values afterwards:

```rust
let mut metadata = metadata_from_track(track, album);
metadata.cover_data = cover_cache.get(&cover_id).cloned();
metadata.lyrics = lyrics.and_then(|lyrics| lyrics.synchronized_lrc.or(lyrics.text));
```

### Tag Writing

```rust
/// Writes metadata tags to a media file.
///
/// Dispatches to FLAC or M4A/MP4 writer based on the typed container.
/// This is a synchronous, blocking function — callers in async context
/// must use `tokio::task::spawn_blocking`.
pub fn write_tags(
    path: &Path,
    container: ContainerExtension,
    metadata: &FileMetadata,
) -> MetadataResult<()>;
```

Do not dispatch by parsing `path.extension()`. The download planner already
knows the final container, and tag-writing should match on that typed value.

The async orchestration layer wraps this in `spawn_blocking`:

```rust
// In CLI download command:
tokio::task::spawn_blocking(move || {
    riptide_metadata::write_tags(&temp_path, final_extension, &metadata)
}).await??;
```

### Cover Art

```rust
/// Saves cover art bytes as a JPEG file using atomic temp-then-persist.
///
/// Synchronous — call from spawn_blocking in async context.
pub fn save_cover(
    data: &[u8],
    path: &Path,
) -> MetadataResult<()>;
```

Cover fetching (HTTP) is the caller's responsibility. The metadata crate only
writes bytes to disk. Cover art is cached per-album during a download command
using `HashMap<CoverArtId, Arc<[u8]>>` in the orchestration layer.

### Lyrics Sidecar

```rust
/// Writes synchronized lyrics to a .lrc sidecar file using atomic
/// temp-then-persist.
///
/// Synchronous — call from spawn_blocking in async context.
pub fn write_lyrics_sidecar(
    media_path: &Path,
    lrc_content: &str,
) -> MetadataResult<()>;
```

### M3U Playlist

```rust
/// A single entry in an M3U playlist.
pub struct M3uEntry {
    /// Relative path to the media file from the playlist location.
    pub path: PathBuf,
    /// Display title.
    pub title: String,
    /// Media duration, formatted as integer seconds at the M3U write boundary.
    pub duration: Option<MediaDuration>,
}

/// Writes an Extended M3U playlist.
///
/// Paths are written relative to the playlist file location.
/// Entry order is preserved from the input slice.
/// Synchronous — call from spawn_blocking in async context.
pub fn write_m3u(
    entries: &[M3uEntry],
    output: &Path,
) -> MetadataResult<()>;
```

---

## Config Extensions

Add to `riptide-config`:

```toml
[download]
audio_quality = "high"         # existing
video_quality = "high"         # existing
skip_existing = true           # existing
concurrency = 4                # new — bounded download concurrency

[download.templates]
default = "{album.artists}/{album.date:%Y} - {album.title}/{item.number:02d} {item.title}"
# track = "{album.artists}/{item.title}"  # optional override
# video = "{item.artist}/{item.title}"    # optional override
# album = "{album.artists}/{album.title}" # optional override
# playlist = "{playlist.title}/{playlist.index:02d} {item.title}"

[metadata]
enable = true
lyrics = true
cover = true

[m3u]
save = false
```

Template override keys are optional. Absence means "inherit `default`"; empty
strings are invalid and must not be used as sentinel values.

### Typed Config Structs

```rust
/// Download concurrency limit.
pub struct Concurrency(NonZeroU8);
impl Concurrency {
    pub fn new(value: u8) -> ConfigResult<Self>;
    pub fn get(self) -> u8;
}
impl Default for Concurrency {
    fn default() -> Self { Self(NonZeroU8::new(4).unwrap()) }
}

/// Validated download root directory.
pub struct DownloadRoot(PathBuf);
impl DownloadRoot {
    pub fn new(path: PathBuf) -> ConfigResult<Self>;
    pub fn as_path(&self) -> &Path;
}

/// How to handle singles when downloading an artist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SinglesMode {
    /// Exclude singles and EPs.
    Exclude,
    /// Include singles and EPs alongside albums.
    Include,
    /// Download only singles and EPs.
    Only,
}

/// How to handle videos when downloading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VideosMode {
    /// Skip all videos.
    Skip,
    /// Include videos alongside audio tracks.
    Include,
    /// Download only videos.
    Only,
}

/// Validated download configuration.
///
/// This extends the existing `riptide-config::DownloadConfig`; it does not
/// replace the existing `audio_quality`, `video_quality`, or `skip_existing`
/// fields.
pub struct DownloadConfig {
    pub audio_quality: AudioQualityRequest,
    pub video_quality: VideoQualityRequest,
    pub skip_existing: bool,
    pub concurrency: Concurrency,
    pub download_root: Option<DownloadRoot>,
    pub singles_mode: SinglesMode,
    pub videos_mode: VideosMode,
}

/// Validated template configuration.
pub struct TemplateConfig {
    pub default: CompiledTemplate,
    pub track: Option<CompiledTemplate>,
    pub video: Option<CompiledTemplate>,
    pub album: Option<CompiledTemplate>,
    pub playlist: Option<CompiledTemplate>,
}

/// Validated metadata configuration.
pub struct MetadataConfig {
    pub enable: bool,
    pub lyrics: bool,
    pub cover: bool,
}

/// Validated M3U configuration.
pub struct M3uConfig {
    pub save: bool,
}
```

When `download_root` is `None`, use `directories::UserDirs::audio_dir()`
falling back to `~/Music`. Never default to empty string.

CLI flags merge over config values. The merge rule is simple: CLI flag present
→ use it; absent → use config value.

### Validation Rules

- `concurrency`: 1-32, reject 0 and >32
- `download_root`: must be writable directory or creatable path
- Template strings: validated at config parse time via `CompiledTemplate::parse`
- Quality values: existing validation from Slice 3
- `singles_mode`/`videos_mode`: enum deserialization rejects unknown values
- Template override strings: absent means inherit default; present empty string
  fails validation

---

## Runtime Config Loading

The `download` command is the first command that consumes runtime config.

Command startup must:

1. Resolve the config path:
   - explicit `--config <path>` if provided by the global CLI
   - otherwise platform app config path via `directories::ProjectDirs`
2. If the config file exists, read and validate it.
3. If the config file is absent, use `ValidatedConfig::default()`.
4. Resolve runtime-only defaults such as `download_root` after validation.
5. Merge CLI flags over validated config:
   - CLI flag present → use CLI value
   - CLI flag absent → use config/default value

The merge result should be a typed `EffectiveDownloadOptions` struct consumed by
the orchestration layer. Do not pass raw CLI strings beyond command-edge parsing.

---

## CLI: `download` Command

```
riptide-rs download <URL>...
    --audio-quality <QUALITY>     Override audio quality
    --video-quality <QUALITY>     Override video quality
    --output <TEMPLATE>           Override output template
    --path <DIR>                  Override download root
    --concurrency <N>             Override concurrency limit
    --skip-existing               Skip existing files (default: true)
    --no-skip-existing            Overwrite existing files
    --no-metadata                 Skip metadata tagging
    --no-lyrics                   Skip lyrics fetch
    --no-cover                    Skip cover art
```

---

## New Workspace Dependencies

Add to root `Cargo.toml` `[workspace.dependencies]`:

```toml
dash-mpd = "0.17"
futures-util = "0.3"
lofty = "0.24"
m3u8-rs = "6"
```

Add `ffmpeg` to the nix flake `nativeBuildInputs` for both the devShell and
test checks.

---

## Testing Strategy

### Unit Tests (riptide-media)

- **DASH MPD parsing**: Fixture XML → correct segment URLs, codec extraction,
  segment count with repeat attributes
- **HLS media playlist parsing**: Fixture M3U8 → correct segment URLs with
  base URL resolution
- **Download plan construction**: Given `StreamVariant`, verify correct
  extensions, post-process steps, and output path (pure, no I/O)
- **FFmpeg command construction**: `ffmpeg_extract_flac_command` and
  `ffmpeg_remux_to_mp4_command` produce correct argument lists (no execution)
- **Post-process step sequencing**: `[ExtractFlac, RemoveSource]` applied
  in order

### Unit Tests (riptide-metadata)

- **Metadata model**: `metadata_from_track` preserves typed `TrackNumber`,
  `DiscNumber`, `time::Date` through to `FileMetadata`
- **M3U writing**: Fixture entries → correct `#EXTM3U`/`#EXTINF` output,
  relative paths, preserved order
- **Lyrics sidecar**: Correct `.lrc` path derivation, UTF-8 content

### Integration Tests (gated, require nix environment)

- **FFmpeg smoke test**: `#[ignore]` test using tiny fixture audio files.
  Run `extract_flac` and `remux_to_mp4` on real (tiny) media files. Verify
  output exists and is valid. Gated behind ffmpeg availability check.
- **Tag round-trip**: Write tags to a fixture FLAC/M4A via `lofty`, read
  back, verify all fields survived. Uses `lofty` test support or tiny fixture
  files.
- **Segment download**: Spin up a local HTTP server (`axum` or `hyper` in
  test), serve fixture segments, verify streaming download produces correct
  concatenated output.

### What We Mock

- TIDAL API responses (fixture JSON, no network)
- Cover art fetch (fixture bytes, no network)

### What We Do NOT Mock

- FFmpeg execution (test with real binary via nix, gated)
- Streaming HTTP download (test with local fixture server)
- Tag writing (test with real lofty on fixture files)

---

## Implementation Order

### Phase A: Foundation (riptide-media core)

1. `riptide-media` crate skeleton with error types
2. `ContainerExtension` enum in `riptide-core`
3. DASH MPD parser (`parse_dash_mpd`) with fixture tests
4. HLS playlist parser (`parse_hls_media_playlist`) with fixture tests
5. `resolve_manifest_sync` and `resolve_manifest_async`
6. `DownloadOptions` and progress callback contract
7. FFmpeg wrapper with command construction tests
8. Segment downloader (`download_segments`) with local HTTP fixture test
9. `DownloadPlan` and `ResolvedDownloadPlan` structs

### Phase B: TIDAL Stream Integration

10. TIDAL stream response serde models, including `TidalManifestMime`
11. `TidalClient::track_stream` and `video_stream` methods
12. TIDAL manifest decoding (`decode_track_manifest`, `decode_video_manifest`)
13. `StreamProvider` trait implementation on `TidalProvider`
14. `CoverArtId`, `CoverArtSize`, `cover_url` types and function

### Phase C: Metadata (riptide-metadata)

15. `riptide-metadata` crate skeleton with error types
16. `FileMetadata` model with typed core values and `Arc<[u8]>` cover data
17. Pure `metadata_from_track` / `metadata_from_video` builders
18. Prototype `lofty` tag writing — FLAC + M4A round-trip test
19. `write_tags(path, ContainerExtension, metadata)` with `spawn_blocking`
    contract documented
20. `write_lyrics_sidecar` with atomic persist
21. `M3uEntry` + `write_m3u` with fixture test
22. `save_cover` with atomic persist

### Phase D: Config + CLI Wiring

23. Config types: `Concurrency`, `DownloadRoot`, `SinglesMode`, `VideosMode`
24. Config sections: `[download]`, `[download.templates]`, `[metadata]`, `[m3u]`
25. Template validation at config parse time; empty override strings fail
26. Runtime config discovery/loading and CLI-over-config merge into
    `EffectiveDownloadOptions`
27. `download` CLI command skeleton
28. Orchestration: plan → resolve manifest → download → post-process → tag →
    persist pipeline
29. Semaphore-based concurrency with `Concurrency` config
30. Skip-existing logic (pure path check before download)
31. Cover art cache: `HashMap<CoverArtId, Arc<[u8]>>` per command invocation
32. Integration testing

---

## Resolved Design Decisions

| Decision | Resolution |
|---|---|
| Manifest ownership | Provider decodes TIDAL-specific wrapping into `StreamVariant { manifest: Manifest }`. Media crate resolves `Manifest` into segment URLs. |
| Pure vs resolved plans | `DownloadPlan` is pure and holds unresolved `Manifest`; `ResolvedDownloadPlan` adds concrete segment URLs after manifest resolution. |
| Transaction model | All-or-nothing per item. Download → post-process → tag → atomic persist from a same-directory `TempDir`. Temp cleanup on failure. |
| Cover art caching | Per-album `HashMap<CoverArtId, Arc<[u8]>>` in orchestration layer. `FileMetadata` also uses `Arc<[u8]>`. Required, not optional. |
| Container extensions | Closed `ContainerExtension` enum in `riptide-core`; no stringly extension dispatch. |
| Filter naming | `SinglesMode` (exclude/include/only), `VideosMode` (skip/include/only). No legacy names. |
| Download root absent | `Option<DownloadRoot>`, falls back to `UserDirs::audio_dir()` or `~/Music`. Never empty string. |
| Blocking metadata writes | `lofty` is sync. Wrapped in `spawn_blocking` at orchestration layer. Metadata crate API is explicitly synchronous. |
| FFmpeg testing | Gated `#[ignore]` smoke tests with real ffmpeg binary (nix-provided). Command construction tested without execution. |
| Metadata types | `FileMetadata` keeps `TrackNumber`, `DiscNumber`, `time::Date` from core. Formatting to strings at writer boundary only. |
| M3U entries | `M3uEntry { path, title, duration }` — works for tracks and videos. Relative paths. Preserved order. |
| Manifest decoding location | TIDAL-specific (BTS, base64) in provider. DASH/HLS parsing in media crate. |
| Config loading | `download` loads config from `--config` or `ProjectDirs`, defaults when absent, then merges CLI flags into `EffectiveDownloadOptions`. |
| Template overrides | Optional keys inherit the default template; empty strings are invalid. |
| Segment retries | Default per-segment timeout 30s and 3 retries; no per-segment HEAD preflight for progress totals. |

## Open Questions

| Question | Decision Needed Before |
|---|---|
| Is `lofty` sufficient for all tag types (FLAC vorbis, M4A iTunes, MP4 video)? | Phase C, step 17 (prototype) |
| Should the download command show a progress bar (`indicatif`)? | Phase D, step 25 |
