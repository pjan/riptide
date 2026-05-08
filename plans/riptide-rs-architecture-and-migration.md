# riptide-rs Architecture and Migration Plan

## Status

Slices 0–3 implemented. Slice 3 audit completed and all 26 actions applied
(see `riptide-rs-slice-3-audit.md`). Architecture document updated to reflect
as-built state after audit.

## Goal

Build a Rust implementation in `riptide-rs/` that starts with feature parity for
the current Python TIDAL downloader, while keeping the design open to additional
catalog, stream, metadata, and local-library providers.

The project should not model "TIDAL" as the product boundary. It should model a
provider-neutral media pipeline:

```text
input reference or URL
  -> provider resolution
  -> catalog entity graph
  -> stream/media resolution
  -> download plan
  -> segment/container acquisition
  -> post-processing
  -> metadata/write/export
```

TIDAL is the first provider implementation.

## Working Principles

These principles are binding design constraints for `riptide-rs`.

### Strict typing at boundaries

Parse TOML, CLI values, audio metrics, scanner requirements, and output format
choices into typed structs, enums, or newtypes before execution. Raw primitives
are acceptable only when the unit or domain cannot be confused.

Examples:

- Use `TrackQuality`, `VideoQuality`, `ProviderId`, `ResourceId`,
  `SampleRateHz`, `BitDepth`, `Bytes`, `TrackNumber`, and `ConcurrencyLimit`.
- Parse CLI strings into typed values before calling application services.
- Parse config into a raw serde shape first, then validate into runtime config.
- Represent output format choices as typed enums instead of ad hoc strings.

### Pure functions first

Implement rule algorithms, config transforms, scanner feature planning,
cache-key construction, and formatter row building as pure functions before
wiring filesystem, CLI, network, or output side effects.

### TDD by default

Start each slice with focused tests for parser and pure behavior. Add
integration tests after wiring. Add regression tests for every bug found during
implementation.

### Data structures matter

Use sets/maps for lookup and deduplication, ordered collections for
user-visible output, explicit cache keys for repeated work, and streaming I/O
for unbounded file sizes.

### Avoid hot-loop allocation

Prefer borrowing over cloning, avoid per-frame/per-track string allocation when
byte or `&str` comparison is enough, and do not recompile or reparse inside
per-track loops.

### Bound scanner cost explicitly

Derive scanner, decode, and hash requirements from active typed rule config
before scanning. Default runs must not perform expensive work merely because a
future rule could need it.

This matters for future local-library and quality-audit features. It should be
reflected in scanner APIs from the beginning, even if the first TIDAL parity
slice does not scan local media deeply.

### Config names must match semantics

Keys with severity values should name the emitted condition, such as
`missing = "info"`, not a boolean-sounding property such as
`required = "info"`. Mode selectors should use names like `accept` when they
define accepted variants.

### No unwired knobs

Do not expose config keys such as `threads` unless the runtime behavior is
implemented. Add the schema and generated-config comments in the same slice that
wires the feature.

### Guided breaking changes

While `riptide-rs` is pre-release, breaking config cleanup is acceptable, but
old keys should fail with clear migration errors such as
`use missing instead of required`.

### CLI errors need context

CLI command edges should preserve operation context in errors. Libraries should
keep structured error enums. The binary crate may use contextual error handling
for user-facing failures.

### Documentation is part of API hardening

Public items in foundational crates need `///` docs before stabilization.
Generated configs should teach the mental model. TOML examples should prefer
literal strings for regex patterns.

### Reviewed plan before development

No substantial slice should start from a high-level roadmap alone. Create a
detailed plan in `docs/`, review it for correctness, config UX, performance,
edge cases, and testability, and resolve review feedback before implementation.

This file is the master architecture plan. Per-slice implementation plans
belong under `docs/riptide-rs/`.

## Repository Layout

Create the Rust implementation under:

```text
riptide-rs/
  flake.nix
  flake.lock
  Cargo.toml
  crates/
    riptide-core/
    riptide-config/
    riptide-template/
    riptide-http/
    riptide-media/
    riptide-metadata/
    riptide-provider-tidal/
    riptide-listener/
    riptide-cli/
```

The `riptide-rs/flake.nix` should provide:

- Rust toolchain.
- `cargo`, `rustc`, `rustfmt`, `clippy`.
- `cargo-nextest` for test execution.
- `cargo-deny` for dependency/license checks.
- `cargo-audit` if available in the chosen nixpkgs input.
- `pkg-config` and media-related native dependencies only when required.
- `ffmpeg` as a runtime dependency for parity with the Python implementation.

The flake should provide at least:

```text
nix develop
nix build
nix flake check
```

## Crate Boundaries

### `riptide-core`

Provider-neutral domain types, capabilities, traits, identifiers, and shared
errors.

This crate must not depend on TIDAL, HTTP clients, CLI output, ffmpeg, local
filesystem layout policy, or terminal UI.

Candidate types:

```rust
pub struct ProviderId(String);
pub struct ResourceId(String);
pub struct Codec(String);
pub struct ProviderQualityLabel(String);
pub struct ProviderContainerLabel(String);
pub struct ProviderExternalIdKind(String);
pub struct BitDepth(u32);
pub struct SampleRateHz(u32);
pub struct ConcurrencyLimit(NonZeroUsize);
pub struct MediaDuration(std::time::Duration);
pub struct TrackNumber(u32);
pub struct DiscNumber(u32);
pub struct ItemCount(u32);
pub struct ExternalIds(BTreeMap<ExternalIdKind, String>);

pub enum ExternalIdKind {
    Isrc,
    Upc,
    ProviderSpecific(ProviderExternalIdKind),
}

pub struct MediaRef {
    pub provider: ProviderId,
    pub kind: ResourceKind,
    pub id: ResourceId,
}

pub enum ResourceKind {
    Track,
    Video,
    Album,
    Playlist,
    Artist,
    Mix,
}
```

Provider-neutral catalog entities (provider-specific fields are typed directly
on the structs rather than carried as untyped JSON):

```rust
pub struct ArtistCredit {
    pub artist: Artist,
    pub role: ArtistRole,
}

pub struct AlbumSummary {
    pub id: MediaRef,
    pub title: String,
    pub artists: Vec<ArtistCredit>,
}

pub struct Track {
    pub id: MediaRef,
    pub title: String,
    pub artists: Vec<ArtistCredit>,
    pub album: Option<AlbumSummary>,
    pub duration: Option<MediaDuration>,
    pub track_number: Option<TrackNumber>,
    pub disc_number: Option<DiscNumber>,
    pub explicit: Option<bool>,
    pub external_ids: ExternalIds,
    pub version: Option<String>,
    pub copyright: Option<String>,
    pub bpm: Option<i64>,
    pub quality_label: Option<String>,
    pub dolby_atmos: bool,
}

pub struct Video {
    pub id: MediaRef,
    pub title: String,
    pub artists: Vec<ArtistCredit>,
    pub album: Option<AlbumSummary>,
    pub duration: Option<MediaDuration>,
    pub track_number: Option<TrackNumber>,
    pub disc_number: Option<DiscNumber>,
    pub explicit: Option<bool>,
    pub quality_label: Option<String>,
}

pub struct Album {
    pub id: MediaRef,
    pub title: String,
    pub artists: Vec<ArtistCredit>,
    pub release_date: Option<time::Date>,
    pub track_count: Option<ItemCount>,
    pub video_count: Option<ItemCount>,
    pub explicit: Option<bool>,
    pub release_type: Option<String>,
    pub hires_lossless: bool,
}

pub struct Playlist {
    pub id: MediaRef,
    pub title: String,
    pub owner: Option<String>,
    pub item_count: Option<ItemCount>,
    pub created_at: Option<time::OffsetDateTime>,
    pub updated_at: Option<time::OffsetDateTime>,
}

pub struct Artist {
    pub id: Option<MediaRef>,
    pub name: String,
}

pub enum ArtistRole {
    Main,
    Featured,
    Contributor,
    Unknown,
}

pub struct Collection {
    pub id: MediaRef,
    pub kind: CollectionKind,
    pub title: Option<String>,
    pub owner: Option<String>,
}

pub enum CollectionKind {
    AlbumFormat,
    Mix,
    ArtistDiscography,
    SearchResults,
    LocalFolder,
}

pub enum CatalogEntity {
    Track(Track),
    Video(Video),
    Album(Album),
    Playlist(Playlist),
    Artist(Artist),
    Collection(Collection),
}
```

These entity structs reflect the as-built state after the Slice 3 audit.
Provider-specific fields (version, copyright, BPM, quality labels, Dolby Atmos,
hi-res lossless, release type) are typed directly on the core structs rather
than carried as untyped JSON via the removed `ProviderExtras` escape hatch.

`Artist.id` is optional because credits-only contributors may have a display
name but no provider-resolvable artist reference. Artist roles are contextual:
use `ArtistCredit.role` for track, album, and credit roles rather than storing a
role on `Artist` itself.

`Collection` is a generic ordered container used when a provider has a group of
items that is not semantically identical to a playlist or album.
`CollectionKind::AlbumFormat` represents an album-format container such as an
album, EP, single, or compilation when a provider returns it through a generic
collection path. Provider-native playlists should normalize to
`CatalogEntity::Playlist`, not `CollectionKind`; generic imported playlist-like
data can become `CollectionKind::SearchResults` or a later explicit collection
variant if needed. A TIDAL mix can normalize to `CollectionKind::Mix`. A future
local folder can normalize to `CollectionKind::LocalFolder`.

Use `time::Date` for release dates, `time::OffsetDateTime` for absolute
timestamps, and `MediaDuration(std::time::Duration)` for media durations.
`MediaDuration` keeps duration semantics explicit while allowing the core model
to use Rust's standard monotonic duration type internally.

"Download artist" means expanding an `Artist` into provider-selected child
collections according to typed command/config filters. For TIDAL parity this
means albums, optionally EPs/singles, and optionally videos. The expansion rule
must be implemented as a pure planning function before network or filesystem
side effects.

Quality and stream candidates:

```rust
pub enum AudioQualityRequest {
    Low,
    High,
    Lossless,
    HiResLossless,
    Best,
}

pub enum VideoQualityRequest {
    Low,
    Medium,
    High,
    Best,
}

pub enum QualityRequest {
    Audio(AudioQualityRequest),
    Video(VideoQualityRequest),
    BestAvailable,
}

pub enum AudioQuality {
    Low,
    High,
    Lossless,
    HiResLossless,
    ProviderSpecific(ProviderQualityLabel),
}

pub enum VideoQuality {
    Low,
    Medium,
    High,
    ProviderSpecific(ProviderQualityLabel),
}

pub enum ResolvedQuality {
    Audio(AudioQuality),
    Video(VideoQuality),
    ProviderSpecific {
        provider: ProviderId,
        label: ProviderQualityLabel,
    },
}
```

`QualityRequest` is what a caller asks for. `ResolvedQuality` is what the
provider actually returns after catalog and entitlement checks.

Provider-neutral stream candidates:

```rust
pub enum Manifest {
    DirectSegments(Vec<Url>),
    DashXml(String),
    HlsMasterPlaylistUrl(Url),
    HlsMediaPlaylist(String),
}

pub enum ContainerKind {
    Flac,
    M4a,
    Mp4,
    TransportStream,
    ProviderSpecific(ProviderContainerLabel),
}

pub struct StreamVariant {
    pub source: MediaRef,
    pub quality: ResolvedQuality,
    pub container: ContainerKind,
    pub codec: Option<Codec>,
    pub bit_depth: Option<BitDepth>,
    pub sample_rate: Option<SampleRateHz>,
    pub manifest: Manifest,
}

pub struct Lyrics {
    pub text: Option<String>,
    pub synchronized_lrc: Option<String>,
    pub source: Option<LyricsSource>,
}

pub enum LyricsSource {
    Provider(ProviderId),
    External(LyricsSourceName),
}

pub enum PostProcessStep {
    ExtractFlac,
    RemuxToMp4,
    RemoveSource,
}
```

`Manifest` describes media acquisition inputs after provider-specific response
wrappers have been decoded. TIDAL BTS JSON is not a core manifest format; it is
decoded by `riptide-provider-tidal` into one of these provider-neutral variants.
`PostProcessStep` is intentionally narrow at first because the Python parity
steps are ffmpeg-based FLAC extraction and video remuxing.

Provider-neutral capability traits also live in this crate. Use `async-trait`
for the first implementation because the registry needs trait objects. Revisit
native async trait syntax only if dynamic dispatch is no longer required.

```rust
#[async_trait::async_trait]
pub trait Resolver: Send + Sync {
    async fn parse_ref(&self, input: &str) -> Result<Option<MediaRef>>;
    async fn resolve(&self, reference: &MediaRef) -> Result<CatalogEntity>;
}

#[async_trait::async_trait]
pub trait CollectionProvider: Send + Sync {
    async fn children(&self, reference: &MediaRef) -> Result<Vec<CatalogEntity>>;
}

#[async_trait::async_trait]
pub trait StreamProvider: Send + Sync {
    async fn streams(
        &self,
        reference: &MediaRef,
        requested: QualityRequest,
    ) -> Result<Vec<StreamVariant>>;
}

#[async_trait::async_trait]
pub trait LyricsProvider: Send + Sync {
    async fn lyrics(&self, reference: &MediaRef) -> Result<Option<Lyrics>>;
}

pub trait Provider:
    Resolver + CollectionProvider + StreamProvider + LyricsProvider + Send + Sync
{
    fn id(&self) -> &ProviderId;
    fn capabilities(&self) -> ProviderCapabilities;
}

bitflags! {
    pub struct ProviderCapabilities: u8 {
        const RESOLVE     = 1 << 0;
        const COLLECTIONS = 1 << 1;
        const STREAMS     = 1 << 2;
        const LYRICS      = 1 << 3;
        const ARTWORK     = 1 << 4;
        const CREDITS     = 1 << 5;
        const SEARCH      = 1 << 6;
    }
}
```

If a provider does not support a capability, its implementation should return a
typed `UnsupportedCapability` error. Optional capability routing can be
introduced later if this single supertrait becomes too restrictive.

This supertrait is an intentional first-phase simplification for TIDAL parity,
not the final extension model. Slice 8 must review whether to keep this shape or
split the registry by capability, for example separate maps for
`Arc<dyn Resolver>`, `Arc<dyn StreamProvider>`, and local-library scanner/tagger
capabilities. That review is required before adding providers that are not
catalog-plus-stream services.

Use layered models:

```text
provider raw model
  -> provider-normalized catalog entity
  -> download metadata model
  -> file tag model
```

Avoid one universal "perfect track" structure. Keep shared fields typed on the
core structs. The `ProviderExtras` JSON escape hatch has been removed — all
provider-specific fields are now typed directly.

Crates:

- `async-trait`
- `bitflags`
- `serde`
- `time`
- `url`
- `thiserror`

Note: `serde_json` moved to dev-dependencies only (used in tests).

### `riptide-config`

TOML parsing, defaults, validation, and app directories.

Uses a two-step parse:

```text
RawConfig via serde
  -> ValidatedConfig via TryFrom
```

Implemented responsibilities:

- Reject unknown keys with path-context errors (`download.threads_count`).
- Validate quality values (`audio_quality`, `video_quality`) and port.
- `audio_quality` (not `track_quality`) — renamed during Slice 3 audit to
  match domain terminology (`AudioQualityRequest`).
- Empty config produces sensible defaults (high quality, port 8123).

Future config sections (blocked on downstream slices):

- `[download.templates]` — wire to the template engine (Slice 4+).
- `[tidal]` — auth/cache file paths, layered over CLI flags.
- `[metadata]`, `[m3u]`, `[list]` — blocked on respective features.
- `config path` subcommand for discoverability.

Likely crates:

- `serde`
- `toml`
- `directories`
- `thiserror`

### `riptide-template`

Template parsing, typed template contexts, formatting, path segment
sanitization, and compatibility tests against the existing Python behavior.

The current Python formatter supports nested fields like `{album.artist}`,
datetime formatting like `{album.date:%Y}`, integer padding like
`{item.number:02d}`, special explicit/master/dolby formatting, and path segment
sanitization.

Do not implement this as simple string replacement. Build a parser for the
documented template grammar and compile templates once.

Design goals:

- Parse once, format many times.
- Borrow values where possible.
- Validate template fields before download starts.
- Keep sanitized path segments deterministic across platforms.

### `riptide-http` (not yet a separate crate)

HTTP client construction is currently in `riptide-provider-tidal::http` (private
module) and exposed via `http_client()`. The client is built once at the CLI
boundary with optional PEM certificate roots from environment variables, then
injected into both `TidalAuthClient` and `TidalClient`.

Cache keys are typed values (`CacheKey`) built by pure functions. The `ApiCache`
trait abstracts cache storage with `get`/`put` methods. `RedbApiCache` is the
default implementation using an embedded redb database.

A separate `riptide-http` crate may be extracted if a second provider needs
shared HTTP infrastructure. For now, keeping it provider-local avoids premature
abstraction.

### `riptide-provider-tidal`

TIDAL auth, TIDAL REST API models, endpoint client, pagination, resource URL
parsing, TIDAL quality mapping, TIDAL manifest wrapper decoding, lyrics,
credits, covers, and TIDAL-to-core model conversion.

This crate owns the raw `serde` models for TIDAL API responses with typed
enums (`TidalArtistRole`, `TidalMediaTag`, `TidalTopHitKind`) instead of
string-typed fields. It also owns tolerant deserialization for unstable TIDAL
responses via `#[serde(other)]` fallback variants.

Architecture after Slice 3 audit:

- **`TokenStore` trait** — abstracts token persistence. `FileTokenStore` is
  the default implementation (atomic JSON file writes via tempfile).
- **`ApiCache` trait** — abstracts API response caching. `RedbApiCache` is
  the default implementation (redb embedded database).
- **`reqwest::Client` injection** — HTTP client is constructed once at the
  CLI boundary and passed into both `TidalAuthClient` and `TidalClient`.
- **`TIDAL_PROVIDER_ID`** — `LazyLock<ProviderId>` static avoids repeated
  validation of the literal "tidal" string.
- **Structured OAuth errors** — `AuthorizationPending`, `SlowDown`,
  `ExpiredToken` variants parsed from OAuth2 error responses instead of
  matching on error body substrings.
- **Single token load per request** — `get_json` loads the token once and
  derives country code from it, eliminating the previous double filesystem
  read.

The provider implements the provider-neutral traits defined in `riptide-core`.
It does not define provider capability traits of its own.

TIDAL-specific manifest wrapper knowledge lives here. For example, this crate
decodes TIDAL BTS JSON and base64 API payloads, then returns a provider-neutral
`Manifest` variant such as `DirectSegments`, `DashXml`, or
`HlsMasterPlaylistUrl`. Generic DASH and HLS parsing still belongs in
`riptide-media`.

Start with compile-time provider crates. Do not build dynamic plugin loading in
the first phase.

### `riptide-media`

Manifest parsing, segment download, temporary files, streaming writes, ffmpeg
execution, and post-processing.

Likely crates:

- `tokio`
- `m3u8-rs`
- `quick-xml`
- `base64`
- `tempfile`
- `url`

Responsibilities:

- Parse DASH XML manifests.
- Parse HLS playlists.
- Download segments with bounded concurrency.
- Stream bytes to disk without buffering complete media in memory.
- Run ffmpeg for container conversion/extraction.
- Clean up temporary files robustly.

Candidate processing model:

```rust
pub struct DownloadPlan {
    pub output_stem: PathBuf,
    pub variant: StreamVariant,
    pub post_process: Vec<PostProcessStep>,
}
```

### `riptide-metadata`

Provider-neutral metadata model, cover art fetching integration points, lyrics
file writing, M3U writing, and file tag writing.

Likely crates:

- `lofty`
- `time`
- `thiserror`

Prototype `lofty` before broad implementation. Verify:

- FLAC Vorbis comments.
- FLAC embedded JPEG cover.
- M4A title, artist, album, album artist, date, copyright.
- M4A lyrics.
- MP4 video metadata.
- Credits and custom/freeform fields where feasible.

If `lofty` cannot cover a required MP4 edge case, evaluate `mp4ameta` or an
external metadata tool for that narrow case.

### `riptide-listener`

HTTP listener server and queue. Replace Flask with `axum`.

Responsibilities:

- Accept local HTTP requests.
- Validate optional auth header.
- Parse input URL/reference.
- Deduplicate pending requests with a set.
- Process queued resources through the same application service used by CLI.

The listener must not duplicate download logic.

Initial listener queues are in-memory only, matching the current Python
behavior. Persistent queues are out of scope until a later explicit scheduling
or daemon slice.

### `riptide-cli`

Binary crate. CLI parsing, user-facing output, logging setup, and command
dispatch.

Likely crates:

- `clap`
- `tracing-subscriber`
- `miette` or `anyhow` at binary edges
- `indicatif` or `console` for progress output

The CLI should not contain provider, HTTP, manifest, or metadata logic. It
should parse values into typed command structs, build application services, and
render results.

## Application Services

Use service structs between CLI/listener and low-level crates:

```rust
pub struct AuthService;
pub struct ResolveService;
pub struct ListService;
pub struct DownloadService;
pub struct ExportService;
```

`ExportService` writes resolved provider resources to non-download artifacts,
such as URL lists, M3U playlists, JSON manifests, or future provider migration
exports. It should reuse provider resolution and collection expansion instead
of maintaining separate traversal logic.

M3U writing has two entry points that share the same writer from
`riptide-metadata`: `DownloadService` may emit an M3U alongside downloaded files
as a configured side effect, while `ExportService` may generate an M3U without
downloading media.

`DownloadService` owns orchestration:

```text
input
  -> provider.parse_ref
  -> provider.resolve
  -> expand collection
  -> apply typed filters
  -> build metadata
  -> compile/apply output template
  -> provider.streams
  -> build DownloadPlan
  -> media downloader
  -> ffmpeg post-process
  -> metadata writer
  -> m3u/cover side effects
```

## Provider Registry

Start with an in-process registry:

```rust
pub struct ProviderRegistry {
    providers: HashMap<ProviderId, Arc<dyn Provider>>,
}
```

Compile providers through Cargo features:

```toml
[features]
default = ["tidal"]
tidal = ["riptide-provider-tidal"]
```

Future provider examples:

- `riptide-provider-qobuz`
- `riptide-provider-bandcamp`
- `riptide-provider-local`
- `riptide-provider-json`

Dynamic plugin loading should be deferred. If it becomes necessary later,
evaluate `abi_stable` and define a narrow provider ABI after the in-process
traits have stabilized.

## Future Source Model

Design around capabilities rather than provider identity.

Some providers may only provide catalog data. Some may resolve streams. Some may
only enrich metadata. Some may operate on local files.

Capability examples:

```text
Resolve references
List collection children
Search catalog
Resolve media streams
Fetch lyrics
Fetch artwork
Fetch credits
Scan local files
Match local files to catalog entities
Export playlists
```

This keeps the project open to:

- TIDAL as the first full provider.
- Qobuz as another authenticated catalog and stream provider.
- Local filesystem as a scan/tag/export provider.
- JSON/CSV imports as lightweight catalog providers.
- Existing library reconciliation and stale-file detection.

## Testing Strategy

Use TDD for each slice.

### Unit tests

- Config parse and migration errors.
- Typed CLI value parsers.
- Cache-key construction.
- Template parser and formatter.
- Path sanitization.
- Manifest parsing.
- Quality selection.
- Resource URL/reference parsing.
- Scanner feature planning.
- Metadata model mapping.

### Integration tests

- Mocked TIDAL API responses.
- HTTP cache hit/miss behavior.
- Download segment ordering and temp-file cleanup.
- ffmpeg wrapper command construction.
- Metadata write/read round trips on fixture files.
- Listener request validation and queue dedupe.

### Golden fixtures

Create fixtures from current Python behavior before porting:

- TIDAL album, playlist, track, video, lyrics, stream, and review response JSON.
- TIDAL BTS JSON manifest wrapper.
- DASH XML manifest.
- HLS master and media playlists.
- Template inputs and expected paths.
- Expected M3U output.

Never require real TIDAL credentials in default tests.

## Implementation Slices

Each slice must start with a detailed plan in `docs/riptide-rs/`.

### Slice 0: Workspace and Nix shell ✓

Deliverables:

- `riptide-rs/flake.nix`
- Cargo workspace.
- Formatting, clippy, test, and check commands.
- Initial crate skeletons only where needed.

Tests/checks:

- `nix develop`
- `cargo fmt --check`
- `cargo clippy`
- `cargo test`
- `nix flake check`

### Slice 1: Core identifiers and config ✓

Deliverables:

- `riptide-core` typed IDs, qualities, resource kinds, candidate catalog
  entities, provider-neutral capability traits, provider registry shape, and
  errors.
- `riptide-config` raw and validated config.
- Unknown/unwired config keys rejected with key-path context.

Tests:

- Config defaults.
- Invalid quality values.
- Unknown and renamed key errors.
- Newtype validation.

### Slice 2: Template engine ✓

Deliverables:

- Compiled template parser.
- Typed template context.
- Explicit/master/dolby formatting.
- Path sanitization.

Tests:

- Golden cases from current Python docs.
- Invalid field errors.
- Date and integer format handling.
- Segment cleanup.

### Slice 3: TIDAL auth and read-only API ✓

Deliverables:

- OAuth device flow with structured error variants.
- Refresh flow.
- TIDAL API client with cache policy and single token load per request.
- Read-only `list` support for track, video, album, playlist, search.
- `TokenStore` and `ApiCache` trait abstractions.
- Injected `reqwest::Client` (constructed once at CLI boundary).
- `clippy::pedantic` and `clippy::nursery` enabled workspace-wide.

Tests (77 total across workspace):

- Auth: country code validation, secret redaction, polling interval, credential
  loading, URL parsing.
- Cache: key sorting, expiry eviction.
- Convert: full track/video/album/playlist/lyrics conversion, artist role
  mapping, quality label mapping, missing optional fields.
- Resource: URL parsing (full URLs, path-style, plural kinds, edge cases).
- Config: defaults, validation, unknown keys.
- Template: parsing, rendering, sanitization.
- CLI integration: config check, template render, auth status.

Audit: 26 actions applied (see `riptide-rs-slice-3-audit.md`).

### Slice 4: Manifest and media download

Deliverables:

- TIDAL stream response decoding into provider-neutral manifest variants.
- Provider-neutral manifest handling.
- DASH XML parser.
- HLS parser.
- Segment downloader with streaming I/O.
- ffmpeg command wrapper.

Tests:

- Manifest fixture parsing.
- Segment ordering.
- Partial failure cleanup.
- ffmpeg command construction.

### Slice 5: Metadata and cover/lyrics/M3U

Deliverables:

- Metadata model.
- FLAC writer.
- M4A writer.
- MP4 video metadata writer.
- Lyrics sidecar writer.
- M3U writer.
- Cover fetching/saving integration.

Tests:

- Fixture file write/read round trips.
- Missing optional fields.
- Multi-artist and credit mapping.
- M3U ordering.

### Slice 6: Download parity

Deliverables:

- Download track.
- Download video.
- Download album.
- Download playlist.
- Download mix.
- Download artist.
- Filters for singles/videos.
- Skip existing and scan path behavior.

Tests:

- Mocked provider integration.
- Deduplication.
- Skip existing variants.
- Bounded concurrency.
- Template prevalidation before download.

### Slice 7: Listener

Deliverables:

- `axum` listener.
- Optional auth header.
- Queue.
- Pending URL dedupe.
- Shared `DownloadService` wiring.

Tests:

- Request validation.
- Auth behavior.
- Duplicate queue handling.
- Service invocation.

### Slice 8: Provider-neutral extension hardening

Deliverables:

- Provider registry.
- Capability introspection.
- Decision on whether the first-phase `Provider` supertrait should remain or be
  replaced by capability-specific registries before non-streaming providers are
  added.
- `tidal:` canonical refs.
- Initial local provider design document. This deliverable is a plan, not a
  local provider implementation.

Tests:

- Provider dispatch.
- Unsupported capability errors.
- Cross-provider reference parse failures.

## Open Design Decisions

These should be resolved before the linked slice plan is approved.

| Decision | Blocks | Resolution |
| --- | --- | --- |
| Whether `riptide-rs` must preserve exact Python template compatibility or only documented compatibility. | Slice 2 | **Resolved.** New app — no backwards compatibility with Python. Own vocabulary and naming. |
| Whether auth tokens should remain JSON files initially or use OS keyrings. | Slice 3 | **Resolved.** `TokenStore` trait extracted. `FileTokenStore` is the default. `KeyringTokenStore` can be added later behind the same trait. |
| Whether API cache should use `http-cache-reqwest` directly or a custom cache for tighter TTL/debug behavior. | Slice 3 | **Resolved.** `ApiCache` trait extracted. `RedbApiCache` (redb embedded DB) is the default. |
| Whether `lofty` is sufficient for all metadata tags. | Slice 5 | Open |
| Whether listener mode remains an in-memory queue or becomes a more general scheduler. | Slice 7 | Open |
| What canonical provider reference syntax should be. | Slice 8 | Open |
| Whether the first-phase `Provider` supertrait remains or must be split by capability. | Slice 8 | Open. `ProviderCapabilities` now uses bitflags which simplifies introspection. |
| How much local-library scanning belongs in the initial architecture. | Slice 8 | Open |

## Review Checklist

Before implementation starts, review this plan and each per-slice plan for:

- Boundary typing: are raw strings/ints converted early?
- Pure behavior: can the main rules be tested without side effects?
- Test-first path: are tests identified before implementation?
- Data structures: are lookup, ordering, cache, and dedupe structures explicit?
- Allocation: are hot loops avoiding avoidable clone/parse/compile work?
- Scanner cost: is expensive work derived from active config?
- Config UX: do names describe emitted conditions and selected modes?
- Wiring: is every exposed config key implemented in runtime behavior?
- Config compatibility: are unsupported keys and values rejected clearly without
  adding aliases?
- Error context: do CLI errors preserve the failed operation?
- Documentation: are public foundational items documented before stabilization?
