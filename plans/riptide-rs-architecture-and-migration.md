# riptide-rs Architecture and Migration Plan

## Status

Draft for review. No substantial implementation should start from this roadmap
alone.

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
pub struct TrackNumber(u32);
pub struct DiscNumber(u32);
pub struct ItemCount(u32);
pub struct ExternalIds(BTreeMap<ExternalIdKind, String>);
pub struct ProviderExtras(serde_json::Value);

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

`ProviderExtras` is the typed escape hatch for provider-specific response
fragments. It should not be used when a field has stable cross-provider
semantics.

Provider-neutral catalog entities:

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
    pub duration: Option<Duration>,
    pub track_number: Option<TrackNumber>,
    pub disc_number: Option<DiscNumber>,
    pub explicit: Option<bool>,
    pub external_ids: ExternalIds,
    pub extras: ProviderExtras,
}

pub struct Video {
    pub id: MediaRef,
    pub title: String,
    pub artists: Vec<ArtistCredit>,
    pub album: Option<AlbumSummary>,
    pub duration: Option<Duration>,
    pub track_number: Option<TrackNumber>,
    pub disc_number: Option<DiscNumber>,
    pub explicit: Option<bool>,
    pub extras: ProviderExtras,
}

pub struct Album {
    pub id: MediaRef,
    pub title: String,
    pub artists: Vec<ArtistCredit>,
    pub release_date: Option<Date>,
    pub track_count: Option<ItemCount>,
    pub video_count: Option<ItemCount>,
    pub explicit: Option<bool>,
    pub extras: ProviderExtras,
}

pub struct Playlist {
    pub id: MediaRef,
    pub title: String,
    pub owner: Option<String>,
    pub item_count: Option<ItemCount>,
    pub created_at: Option<DateTime>,
    pub updated_at: Option<DateTime>,
    pub extras: ProviderExtras,
}

pub struct Artist {
    pub id: Option<MediaRef>,
    pub name: String,
    pub role: ArtistRole,
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
    Playlist,
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

These entity structs are candidate shapes for Slice 1, not a final field lock.
Slice 1 should define the minimal public fields needed by later slices and leave
provider-specific response details in `ProviderExtras`.

`Artist.id` is optional because credits-only contributors may have a display
name but no provider-resolvable artist reference.

`Collection` is a generic ordered container used when a provider has a group of
items that is not semantically identical to a playlist or album.
`CollectionKind::AlbumFormat` represents an album-format container such as an
album, EP, single, or compilation when a provider returns it through a generic
collection path. A TIDAL mix can normalize to `CollectionKind::Mix`. A future
local folder can normalize to `CollectionKind::LocalFolder`.

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
    pub provider: Option<String>,
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

pub struct ProviderCapabilities {
    pub resolve: bool,
    pub collections: bool,
    pub streams: bool,
    pub lyrics: bool,
    pub artwork: bool,
    pub credits: bool,
    pub search: bool,
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

Avoid one universal "perfect track" structure. Keep shared fields typed and use
explicit provider extras for non-portable data.

Likely crates:

- `async-trait`
- `serde`
- `url`
- `thiserror`

### `riptide-config`

TOML parsing, defaults, validation, migration errors, app directories, and
generated example config.

Use a two-step parse:

```text
RawConfig via serde
  -> ValidatedConfig via TryFrom
```

Responsibilities:

- Reject unknown renamed keys with actionable migration errors.
- Reject unwired keys.
- Validate templates, quality values, concurrency limits, and paths before
  execution.
- Keep comments in generated config aligned with implemented behavior.

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

### `riptide-http`

HTTP client wrapper, cache policy, retry policy, auth headers, debug response
capture, and cache-key construction.

Likely crates:

- `reqwest`
- `reqwest-middleware`
- `http-cache-reqwest`
- `tokio`
- `tracing`
- `serde_json`

Cache keys must be explicit typed values built by pure functions. Different API
operations need different cache behavior: static catalog calls can cache,
sessions/search/stream URLs should generally not cache or should use very short
TTL.

### `riptide-provider-tidal`

TIDAL auth, TIDAL REST API models, endpoint client, pagination, resource URL
parsing, TIDAL quality mapping, TIDAL manifest wrapper decoding, lyrics,
credits, covers, and TIDAL-to-core model conversion.

This crate owns the raw `serde` models equivalent to the current Python
Pydantic models. It also owns tolerant deserialization for unstable TIDAL
responses.

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

### Slice 0: Workspace and Nix shell

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

### Slice 1: Core identifiers and config

Deliverables:

- `riptide-core` typed IDs, qualities, resource kinds, candidate catalog
  entities, provider-neutral capability traits, provider registry shape, and
  errors.
- `riptide-config` raw and validated config.
- Migration errors for known legacy/unwanted keys.

Tests:

- Config defaults.
- Invalid quality values.
- Unknown and renamed key errors.
- Newtype validation.

### Slice 2: Template engine

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

### Slice 3: TIDAL auth and read-only API

Deliverables:

- OAuth device flow.
- Refresh flow.
- TIDAL API client with cache policy.
- Read-only `list` support for track, album, playlist.

Tests:

- Mocked auth responses.
- Token refresh behavior.
- API cache key and cache policy.
- Serde models for fixture JSON.

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

| Decision | Blocks |
| --- | --- |
| Whether `riptide-rs` must preserve exact Python template compatibility or only documented compatibility. | Slice 2 |
| Whether auth tokens should remain JSON files initially or use OS keyrings. | Slice 3 |
| Whether API cache should use `http-cache-reqwest` directly or a custom SQLite cache for tighter TTL/debug behavior. | Slice 3 |
| Whether `lofty` is sufficient for all metadata tags. | Slice 5 |
| Whether listener mode remains an in-memory queue with single-resource sequential processing and per-resource track concurrency, matching current behavior, or becomes a more general scheduler. Persistent queues are out of scope unless this decision changes. | Slice 7 |
| What canonical provider reference syntax should be: `tidal:track:123`, `tidal://track/123`, or both. | Slice 8 |
| Whether the first-phase `Provider` supertrait remains acceptable or must be replaced by capability-specific registries before local/import-only providers are added. | Slice 8 |
| How much local-library scanning belongs in the initial architecture versus a later provider slice. | Slice 8 |

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
- Migration: do breaking changes fail with actionable messages?
- Error context: do CLI errors preserve the failed operation?
- Documentation: are public foundational items documented before stabilization?
