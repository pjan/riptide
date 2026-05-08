# riptide-rs Slice 0-1 Implementation Plan

## Status

Draft for review. This plan covers only the first implementation milestone:
workspace/bootstrap plus foundational core/config crates. Do not start coding
until this plan has been reviewed for correctness, config UX, performance, edge
cases, and testability.

## Scope

Combine:

- Slice 0: Workspace and Nix shell.
- Slice 1: Core identifiers and config.

The result should be a buildable `riptide-rs/` workspace with the smallest
useful foundation for later slices.

This slice must not implement TIDAL auth, TIDAL API calls, template parsing,
downloads, metadata writing, listener mode, or CLI commands beyond the minimal
binary needed to prove the workspace builds.

## Goals

1. Create `riptide-rs/` as an independent Rust workspace managed by its own Nix
   flake.
2. Provide repeatable development commands through `nix develop`,
   `nix build`, and `nix flake check`.
3. Establish foundational crates:
   - `riptide-core`
   - `riptide-config`
   - `riptide-cli`
4. Define strict typed boundaries for identifiers, resource kinds, quality
   requests, stream metadata, catalog entity candidates, provider traits, and
   validated config.
5. Add focused tests before behavior is wired into command/runtime paths.
6. Keep all exposed config keys wired to validation behavior in this slice.

## Non-Goals

- No TIDAL provider crate yet.
- No HTTP client crate yet.
- No template engine crate yet.
- No media, ffmpeg, metadata, M3U, or listener implementation.
- No dynamic plugin loading.
- No local-library scanner behavior.
- No generated full example config beyond keys that are implemented by this
  slice.

## Repository Changes

Create:

```text
riptide-rs/
  flake.nix
  Cargo.toml
  crates/
    riptide-core/
      Cargo.toml
      src/lib.rs
    riptide-config/
      Cargo.toml
      src/lib.rs
    riptide-cli/
      Cargo.toml
      src/main.rs
```

Optional if needed by the chosen flake pattern:

```text
riptide-rs/rust-toolchain.toml
riptide-rs/flake.lock
```

Do not create placeholder crates for future slices unless a current test or
dependency requires them.

## Nix Flake Plan

`riptide-rs/flake.nix` should provide:

- Stable Rust toolchain.
- `cargo`, `rustc`, `rustfmt`, `clippy`.
- `cargo-nextest`.
- `cargo-deny` if available.
- `cargo-audit` if available.
- `pkg-config` only if required by current dependencies.
- `ffmpeg` is not required in this slice because no media behavior is wired.

Commands:

```text
nix develop
nix build
nix flake check
```

`nix flake check` should run at least:

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo nextest run --workspace`

Use plain `cargo test --workspace` only as a fallback if the selected flake
input genuinely cannot provide `cargo-nextest`. Do not write tests or scripts
that require an unavailable optional tool.

## Crate Design

### `riptide-core`

Purpose: provider-neutral domain and trait definitions.

Likely dependencies:

- `async-trait`
- `serde`
- `serde_json`
- `thiserror`
- `time = { version = "0.3", features = ["formatting", "serde"] }`
- `url`

The crate should expose typed values instead of raw primitives at boundaries.
Use newtypes for domains where confusion is possible.

Candidate modules:

```text
src/
  lib.rs
  id.rs
  catalog.rs
  metrics.rs
  quality.rs
  stream.rs
  provider.rs
  error.rs
```

#### Identifiers

Define:

```rust
ProviderId
ResourceId
MediaRef
ResourceKind
```

Behavior:

- `ProviderId` and `ResourceId` reject empty strings.
- `ProviderId` must derive or implement `Hash`, `Eq`, and `PartialEq` because
  `ProviderRegistry` uses it as a `HashMap` key.
- `ResourceKind` is an enum with `Track`, `Video`, `Album`, `Playlist`,
  `Artist`, and `Mix`.
- `ResourceKind` serde uses lowercase `snake_case`: `track`, `video`, `album`,
  `playlist`, `artist`, and `mix`.
- `MediaRef` combines provider, kind, and ID.
- Implement `Display` where useful for error messages.
- Avoid parsing canonical `tidal:` refs in this slice; that decision belongs to
  Slice 8.

Tests first:

- Empty provider ID fails.
- Empty resource ID fails.
- Valid IDs preserve original value.
- `ResourceKind` serde round trip uses lowercase `snake_case`.

#### Catalog Entities

Define candidate structs with minimal stable fields:

```rust
Artist
ArtistCredit
ArtistRole
AlbumSummary
Track
Video
Album
Playlist
Collection
CollectionKind
CatalogEntity
ExternalIds
ExternalIdKind
ProviderExternalIdKind
ProviderExtras
```

Rules:

- `Artist` has `id: Option<MediaRef>` and `name`, but no role.
- Role is contextual and lives on `ArtistCredit`.
- `ArtistRole` variants are `Main`, `Featured`, `Contributor`, and `Unknown`.
- Provider-native playlists use `CatalogEntity::Playlist`.
- `CollectionKind` should not include `Playlist`.
- `CollectionKind::AlbumFormat` covers album/EP/single/compilation-like
  containers when a provider returns them through a generic collection path.
- `CollectionKind` variants are `AlbumFormat`, `Mix`, `ArtistDiscography`,
  `SearchResults`, and `LocalFolder`.
- `ProviderExtras(serde_json::Value)` is allowed as a deliberate JSON-specific
  escape hatch. Pulling `serde_json` into `riptide-core` is intentional only for
  this provider-specific data escape hatch; do not use it for stable
  cross-provider fields.
- `ExternalIdKind` and `ProviderExternalIdKind` must derive or implement
  `Ord` and `PartialOrd` because `ExternalIds` uses `BTreeMap`.

Tests first:

- `ArtistCredit` can represent a credits-only contributor with `artist.id =
  None` and a contextual role.
- `CatalogEntity::Playlist` is the only provider-native playlist
  representation.
- `ProviderExtras` can round-trip provider-specific JSON.

#### Time and Metrics

Define:

```rust
MediaDuration(std::time::Duration)
BitDepth(u32)
SampleRateHz(u32)
TrackNumber(u32)
DiscNumber(u32)
ItemCount(u32)
ConcurrencyLimit(NonZeroUsize)
```

Use:

- Place these types in `metrics.rs`.
- `time::Date` for release dates.
- `time::OffsetDateTime` for absolute timestamps.
- `MediaDuration(std::time::Duration)` for track/video durations.

Validation:

- Numeric newtypes reject zero where zero has no valid domain meaning.
- `ConcurrencyLimit` gets zero rejection structurally from `NonZeroUsize`; do
  not add redundant custom validation beyond the constructor path.
- If zero is a valid unknown/placeholder in a future provider response, keep it
  out of the validated newtype and model it as `Option`.

Tests first:

- Invalid zero values fail for `TrackNumber`, `DiscNumber`, `ItemCount`,
  `BitDepth`, `SampleRateHz`, and `ConcurrencyLimit`.
- Valid values construct successfully.
- `MediaDuration` preserves second-level values.

#### Quality and Streams

Define:

```rust
AudioQualityRequest
VideoQualityRequest
QualityRequest
AudioQuality
VideoQuality
ResolvedQuality
ProviderQualityLabel
Codec
ContainerKind
ProviderContainerLabel
Manifest
StreamVariant
Lyrics
LyricsSource
LyricsSourceName
PostProcessStep
```

Rules:

- `QualityRequest` represents caller intent.
- `ResolvedQuality` represents what a provider actually returned.
- Canonical TOML/serde strings:
  - `AudioQualityRequest`: `low`, `high`, `lossless`, `hi_res_lossless`, `best`.
  - `VideoQualityRequest`: `low`, `medium`, `high`, `best`.
  - `QualityRequest`: use tagged serde only if needed by APIs; config should
    parse directly into audio/video-specific request types.
- `Codec` is a newtype, not a raw string in stream structs.
- Lyrics attribution uses a typed source enum:
  `LyricsSource::Provider(ProviderId)` for registered providers and
  `LyricsSource::External(LyricsSourceName)` for external attribution such as
  `Musixmatch`. The `Lyrics` struct should use `source: Option<LyricsSource>`.
- `Lyrics` fields are `text: Option<String>`,
  `synchronized_lrc: Option<String>`, and `source: Option<LyricsSource>`.
- TIDAL BTS JSON is not a `Manifest` variant. Providers decode wrappers into
  provider-neutral variants.
- `PostProcessStep` starts narrow: `ExtractFlac`, `RemuxToMp4`,
  `RemoveSource`.

Tests first:

- `Codec` rejects empty strings.
- `ResolvedQuality` can represent provider-specific quality through typed
  labels.
- `LyricsSource::External` can hold a non-provider attribution string such as
  `Musixmatch`.
- `LyricsSource::Provider` wraps a validated `ProviderId`.
- `StreamVariant` can represent direct segments, DASH XML, and HLS playlist
  inputs.

#### Provider Traits and Registry Shape

Define provider-neutral traits:

```rust
Resolver
CollectionProvider
StreamProvider
LyricsProvider
Provider
ProviderCapabilities
ProviderRegistry
```

Rules:

- Use `async-trait` for first-phase trait objects.
- `ProviderRegistry` may initially hold `HashMap<ProviderId, Arc<dyn Provider>>`.
- This supertrait shape is a known first-phase simplification.
- Add code comments or docs noting that Slice 8 must revisit capability-specific
  registries before adding non-streaming/local/import-only providers.
- Unsupported capability should be represented by a structured error.

Tests first:

- Registry rejects duplicate provider IDs.
- Registry returns provider by ID.
- Registry returns a structured missing-provider error.
- A fake provider can return `UnsupportedCapability`.

#### Errors

Use `thiserror`.

Define structured errors for:

- Invalid newtype values.
- Missing provider.
- Duplicate provider.
- Unsupported capability.
- Invalid config values surfaced from `riptide-config` if shared error types
  are needed.

Libraries should not use `anyhow` internally.

### `riptide-config`

Purpose: parse raw TOML into validated runtime config.

Likely dependencies:

- `riptide-core`
- `serde`
- `toml`
- `thiserror`

Candidate modules:

```text
src/
  lib.rs
  raw.rs
  validated.rs
  error.rs
```

#### Config Shape

Implement only keys needed for foundational validation and later CLI setup:

```toml
[download]
track_quality = "high"
video_quality = "high"
skip_existing = true

[listener]
port = 8123
secret = ""
```

Defaults:

- `download.track_quality = high`
- `download.video_quality = high`
- `download.skip_existing = true`
- `listener.port = 8123`
- `listener.secret = None` when the TOML value is missing or an empty string

Do not include `enable_cache` or `debug` in this slice's config. There is no
HTTP cache, API client, or debug response capture behavior in Slice 0-1, so
those keys would be accepted-but-inert knobs. Add them in the slice that wires
their runtime behavior.

Do not include `concurrency`, `threads`, or `concurrent_downloads` in this
slice's config. Define `ConcurrencyLimit` in `riptide-core`, but expose
concurrency config only in the later slice that wires runtime concurrency
behavior.

Use Rust quality names in config:

- audio: `low`, `high`, `lossless`, `hi_res_lossless`, and `best`
- video: `low`, `medium`, `high`, and `best`

Python quality strings from `riptide/core/utils/const.py` should fail with
actionable migration guidance:

- `download.track_quality = "normal"` -> use `"high"` in Rust, matching
  Python's `normal -> HIGH`
- `download.track_quality = "high"` remains accepted in Rust but means the
  Rust tier named `high`; Python users who intended Python `high -> LOSSLESS`
  should migrate to `"lossless"` explicitly
- `download.track_quality = "max"` -> use `"hi_res_lossless"` in Rust, matching
  Python's `max -> HI_RES_LOSSLESS`
- `download.video_quality = "sd"` -> use `"low"` in Rust
- `download.video_quality = "hd"` -> use `"medium"` in Rust
- `download.video_quality = "fhd"` -> use `"high"` in Rust

Because Python and Rust both use the string `"high"` but with different
semantics, generated migration docs should call out this semantic difference.

#### Raw to Validated

Use:

```text
RawConfig via serde
  -> ValidatedConfig via TryFrom
```

Rules:

- Raw config may contain strings and primitives.
- Validated config must contain typed fields.
- Unknown old keys should fail with migration errors when known.
- Do not silently ignore unknown keys.
- Config names must match semantics.
- Use a two-pass unknown-key strategy:
  1. Deserialize TOML into `toml::Value`.
  2. Inspect the value tree for known legacy keys that cannot exist on
     `RawConfig`, such as root `enable_cache`, root `debug`, and
     `[download].threads_count`.
  3. Deserialize the same value into `RawConfig` using strict serde structs with
     unknown-field rejection for generic unknown-key errors.
  4. Convert `RawConfig` into `ValidatedConfig` with `TryFrom`; value-level
     migrations such as `[download].video_quality = "fhd"` are reported here.
  This preserves actionable migration messages while still rejecting unknown
  keys with path context.

#### Initial Validated Config

Recommended minimal runtime config for this slice:

```rust
ValidatedConfig {
    download: DownloadConfig {
        track_quality: AudioQualityRequest,
        video_quality: VideoQualityRequest,
        skip_existing: bool,
    },
    listener: ListenerConfig {
        port: ListenerPort,
        secret: Option<ListenerSecret>,
    },
}
```

Typed config values:

```rust
ListenerPort(u16)
ListenerSecret(String)
```

Validation:

- `ListenerPort` rejects `0`.
- Empty listener secret becomes `None`.
- `ListenerSecret` must not expose its value through `Debug`; implement redacted
  debug output manually or use a secret wrapper type.
- Track quality accepts only documented values.
- Video quality accepts only documented values.
- Python compatibility/migration keys should use the current Python config as
  the source of truth: `riptide/cli/config.py`.
  For this slice, explicitly handle only keys that collide with the minimal
  schema:
  - `[download].video_quality = "fhd"` should fail with `use "high" instead of
    "fhd"`.
  - `[download].video_quality = "sd"` should fail with `use "low" instead of
    "sd"`.
  - `[download].video_quality = "hd"` should fail with `use "medium" instead
    of "hd"`.
  - `[download].track_quality = "normal"` should fail with `use "high" instead
    of "normal"`.
  - `[download].track_quality = "max"` should fail with `use
    "hi_res_lossless" instead of "max"`.
  - `[download].threads_count` should fail with `concurrency is not implemented
    yet; remove threads_count until the download slice wires it`.
  - Root `enable_cache` and `debug` should fail with `not implemented in this
    slice; add these when HTTP cache/debug behavior is wired`.
  Do not invent migration errors for unrelated future sections such as
  template, metadata, scanner, M3U, or cover config.

Tests first:

- Empty config gives defaults.
- Valid config parses into typed runtime config.
- Invalid track quality fails with key path context.
- Invalid video quality fails with key path context.
- Port `0` fails.
- Empty listener secret becomes `None`.
- Known old key fails with an actionable migration message.
- Unknown key fails with key path context.

#### Config UX

Generated example config, if added in this slice, must only document keys
implemented by this slice.

Do not include comments for future template, scanner, metadata, M3U, or
download-thread behavior.

### `riptide-cli`

Purpose: minimal binary to prove workspace wiring.

Dependencies:

- `riptide-config`
- `riptide-core`
- `clap = { version = "4", features = ["derive"] }`
- `anyhow = "1.0"` for binary edge context.

Minimal behavior:

- `riptide-rs --version`
- `riptide-rs config check --path <file>`, where `--path` is required in this
  slice.

CLI values must parse into typed values before calling library code. CLI errors
must include operation context, such as `failed to load config from <path>`.
Use `anyhow::Context` at the binary edge for those user-facing contexts while
keeping library errors structured.

No download/list/listen/auth commands in this slice.
Default config path discovery is out of scope for Slice 0-1, so
`riptide-config` does not need the `directories` crate yet. Add default path
resolution in the slice that introduces normal runtime config loading.

## Test Plan

Write tests before implementation in each area.

### Unit Tests

`riptide-core`:

- Newtype validation.
- Resource kind serde representation.
- Catalog entity invariants.
- Quality request parsing/serde.
- Codec validation.
- Provider registry duplicate/missing behavior.
- Unsupported capability error.

`riptide-config`:

- Defaults.
- Valid TOML.
- Invalid quality values.
- Invalid listener port.
- Empty listener secret handling.
- Unknown key handling.
- Known migration key handling.

### Integration Tests

- `config check` succeeds for a valid fixture.
- `config check` fails for an invalid fixture with context.

### No Network Tests

This slice must not require network access, TIDAL credentials, ffmpeg, or local
media fixtures.

## Performance and Allocation Notes

- Newtype constructors should borrow `&str` and allocate only once when storing.
- Do not parse or allocate provider refs in hot loops; canonical ref parsing is
  not part of this slice.
- Use `HashMap` for provider registry lookup.
- Use ordered collections only where user-visible output order matters. There
  is no substantial user-visible output in this slice.
- Config validation should parse once into raw config and convert once into
  validated config.

## Documentation Requirements

Before this slice is considered done:

- Public foundational types in `riptide-core` have `///` docs.
- Provider trait docs explain first-phase supertrait simplification and Slice 8
  revisit.
- `riptide-config` docs explain raw vs validated config.
- Any generated config comments match implemented behavior only.

## Deferred Types

`PostProcessStep` is in scope for `riptide-core` because stream variants need to
describe provider-neutral post-processing intent later, but `DownloadPlan` is
not in scope for this slice. `DownloadPlan` belongs in `riptide-media` in Slice
4, where media acquisition and ffmpeg command construction are implemented.

## Implementation Order

1. Add Nix flake and empty Cargo workspace.
2. Add crate skeletons with no public API beyond compile stubs.
3. Add first `riptide-core` tests for newtypes and resource kinds.
4. Implement newtypes/resource kinds.
5. Add catalog entity tests.
6. Implement candidate catalog entities.
7. Add quality/stream tests.
8. Implement quality/stream types.
9. Add provider registry/error tests with a fake provider.
10. Implement provider traits, registry shape, and errors.
11. Add `riptide-config` tests for defaults and validation.
12. Implement raw and validated config.
13. Add minimal CLI wiring and `config check`.
14. Add CLI integration tests for `config check`.
15. Add `///` docs for public foundational APIs and verify generated config
    comments only describe wired behavior.
16. Run `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
    `cargo nextest run --workspace`, and `nix flake check`.

## Completion Criteria

- `riptide-rs/` builds through Cargo and Nix.
- All tests pass without network or credentials.
- `cargo fmt --check` passes.
- `cargo clippy --workspace --all-targets -- -D warnings` passes.
- `cargo nextest run --workspace` passes.
- `nix flake check` passes.
- Public foundational items have docs.
- No future/unwired config knobs are documented.
- The final commit only includes Slice 0-1 files and this plan if the plan is
  committed separately.

## Review Checklist

Before implementation, review:

- Are all raw strings/integers converted into typed values before runtime use?
- Are pure constructors and config transforms testable without filesystem or
  CLI side effects?
- Do tests precede each behavior area?
- Are provider lookup/deduplication structures explicit?
- Are any config keys exposed without implemented validation?
- Do old-key migration errors give concrete replacement guidance?
- Do CLI errors preserve config path and operation context?
- Are public foundational APIs documented?
- Does the plan avoid template/download/listener behavior until later slices?
