# riptide-rs Slice 2-3 Implementation Plan

## Status

Draft for review. This plan covers the next combined milestone:

- Slice 2: Template engine.
- Slice 3: TIDAL auth and read-only API.

Do not start implementation until this plan is reviewed for correctness, config
UX, performance, edge cases, live-auth testability, and crate boundaries.

## Scope

Build a provider-neutral template engine and the first real provider crate for
TIDAL read-only catalog access.

This slice should end with:

- A compiled template engine that can render paths from typed catalog context.
- TIDAL device auth, token refresh, logout, and token persistence.
- A TIDAL read-only API client with typed request/response boundaries and
  explicit cache behavior.
- CLI commands for auth and read-only list/show behavior.
- Mocked integration tests for all auth/API edge paths.
- A gated live TIDAL auth-flow check that proves the full device flow works
  against TIDAL at the end of the slice.

## Non-Goals

- No media stream manifest decoding.
- No segment downloading or ffmpeg invocation.
- No metadata writing.
- No M3U writing.
- No listener mode.
- No dynamic provider plugins.
- No local-library provider.
- No download orchestration beyond rendering candidate paths for catalog items.
- No exposed config keys unless their runtime behavior is implemented in this
  slice.

## Python Parity Baseline

This slice should stay close to the existing Python implementation unless the
plan explicitly calls out a Rust-side cleanup.

Primary Python sources:

- `riptide/core/utils/format.py`: template data model, explicit/master/dolby
  formatting, `{now}`, path segment cleanup, and path joining.
- `riptide/cli/config.py`: current default download/list template strings,
  quality defaults, list output format, and filter names.
- `riptide/core/utils/const.py`: Python quality string mapping. Current Python
  maps audio `low -> LOW`, `normal -> HIGH`, `high -> LOSSLESS`, and
  `max -> HI_RES_LOSSLESS`; video maps `sd -> LOW`, `hd -> MEDIUM`, and
  `fhd -> HIGH`.
- `riptide/cli/commands/list.py`: read-only list traversal for track, video,
  album, playlist, artist, and mix resources.
- `riptide/core/auth/client.py`, `riptide/core/auth/api.py`, and
  `riptide/cli/commands/auth.py`: TIDAL device auth, polling, refresh, logout,
  and token persistence behavior.
- `riptide/core/api/api.py` and `riptide/core/api/client.py`: TIDAL endpoints,
  endpoint-specific cache lifetimes, 401 refresh callback behavior, and response
  model boundaries.
- `riptide/core/api/models/resources.py` and
  `riptide/core/api/models/base.py`: source JSON field names and current model
  expectations.

Intentional Rust changes in this slice:

- Parse templates into a typed AST instead of using Python `str.format`.
- Fail invalid templates before auth/network work.
- Use typed tokens, IDs, dates, durations, qualities, and cache keys.
- Keep auth/cache paths as CLI runtime options rather than TOML keys.
- Add mock and gated live auth tests that the Python implementation does not
  currently have.
- Add `list search` as a read-only command; Python exposes the search API but
  does not currently wire it into `list.py`.

## Blocking Decisions Resolved

### Template Compatibility

Use documented compatibility with golden parity for the current Python
placeholder surface. Do not attempt byte-for-byte compatibility with every
Python `str.format` edge case.

The Rust engine should support current documented fields and observed format
specs from `riptide/core/utils/format.py`, especially:

- `{item.id}`
- `{item.title}`
- `{item.title_version}`
- `{item.number}` and integer padding such as `{item.number:02d}`
- `{item.volume}`
- `{item.version}`
- `{item.copyright}`
- `{item.bpm}`
- `{item.isrc}`
- `{item.quality}`
- `{item.artist}`
- `{item.artists}`
- `{item.features}`
- `{item.artists_with_features}`
- `{item.explicit}` and explicit specs `long`, `full`, `upper`
- `{item.dolby:<text>}`
- `{album.id}`
- `{album.title}`
- `{album.artist}`
- `{album.artists}`
- `{album.date}` with date formatting
- `{album.explicit}`
- `{album.master:<text>}`
- `{album.release}`
- `{playlist.uuid}`
- `{playlist.title}`
- `{playlist.index}`
- `{playlist.created}` and `{playlist.updated}` with date formatting
- `{now}` with datetime formatting
- extra scalar fields used by download/export templates, such as `{type}` and
  `{mix_id}`

Unsupported Python formatting features must fail at compile time with a
structured template error, not panic or silently pass through.

### Auth Token Storage

Use JSON token storage initially, matching the Python implementation's
operational model. Do not use OS keyrings in this slice.

Token files must:

- Store access token, refresh token, expiry, user ID, and country code.
- Use typed token wrappers with redacted `Debug`.
- Be written atomically.
- Use owner-only permissions on Unix where possible.
- Be overridable with CLI `--auth-file <path>` for tests and isolated live
  verification.

### API Cache

Use a custom typed cache backed by `redb` for TIDAL API JSON responses. Do not
use a generic HTTP cache wrapper in this slice.

Rationale:

- Cache keys can be constructed as pure typed values from method, endpoint,
  normalized query params, country code, and cache namespace.
- TTL behavior can match endpoint semantics directly.
- Tests can verify cache hits without depending on HTTP-cache implementation
  internals.
- This avoids exposing cache config before runtime behavior is wired.

## Repository Changes

Add crates:

```text
riptide-rs/crates/riptide-template/
  Cargo.toml
  src/lib.rs
  src/ast.rs
  src/context.rs
  src/error.rs
  src/parser.rs
  src/render.rs
  src/sanitize.rs

riptide-rs/crates/riptide-provider-tidal/
  Cargo.toml
  src/lib.rs
  src/auth.rs
  src/cache.rs
  src/client.rs
  src/convert.rs
  src/error.rs
  src/model.rs
  src/provider.rs
  src/resource.rs
```

Update:

```text
riptide-rs/Cargo.toml
riptide-rs/flake.nix
riptide-rs/crates/riptide-cli/
riptide-rs/crates/riptide-core/
riptide-rs/crates/riptide-config/
```

Do not create placeholder media/download crates in this slice.

Do not add `riptide-http` in this slice. The architecture plan still reserves
that crate for a future provider-neutral HTTP layer, but it is explicitly
deferred here. `TidalClient` inside `riptide-provider-tidal` is the first-phase
HTTP wrapper for auth headers, typed requests, retry behavior, cache policy, and
cache-key construction. It should be cleanly factored inside the provider crate,
but it does not need to match a future `riptide-http` interface yet.

## Dependency Plan

Keep existing preferred dependencies where applicable:

- `anyhow`
- `clap`
- `redb`
- `serde`
- `serde_json`
- `tempfile`
- `time`
- `toml`

Add only the dependencies needed for this slice:

- `reqwest` with `json`, `rustls-tls`, and no default native TLS dependency.
- `tokio` with `macros`, `rt-multi-thread`, `time`, and `fs`.
- `secrecy` or manual redacted wrappers for token/client secret types. Prefer
  manual wrappers if that keeps the dependency graph smaller.
- `directories` in `riptide-cli` for platform app-data path resolution. Do not
  add it to `riptide-provider-tidal`; provider construction receives typed
  paths and stores from the CLI.
- `open` only if browser launch is implemented; otherwise print the URL and
  make browser opening explicit later.
- `wiremock` or an equivalent local HTTP mock as a dev-dependency for auth/API
  tests. Prefer a local HTTP server over hand-written `reqwest` mocks because
  request shape and retry behavior matter.

`regex` is not a dependency of this slice. Sanitization uses direct character
and `str` operations only, so no `LazyLock`, `once_cell`, or `lazy_static`
dependency is needed for sanitizer behavior.

Use the `reqwest`/`rustls` backend that builds reliably in the Nix shell. If the
selected `reqwest` version's `rustls-tls` feature pulls `aws-lc-rs`, make sure
the flake includes required C build tooling. If avoiding that is preferable,
choose a `rustls` feature set that uses `ring` and document the choice in
`Cargo.toml`.

Do not add `ffmpeg`, `lofty`, `image`, `symphonia`, or audio crates in this
slice.

## Crate Design

### `riptide-template`

Purpose: compile and render path templates from typed provider-neutral context.

Public API candidates:

```rust
CompiledTemplate
TemplateContext
TemplateValue
TemplateRenderOptions
TemplateError
PathSegment
RenderedPath
TemplateNode
TemplateField
FieldSegment
ExtraFieldName
ValueNamespace
FormatSpec
ParsedDateTimeFormat
IntFill
ExplicitMode
ExplicitFormat
PlaylistIndex
```

Rules:

- Parse once into an AST; render many times by borrowing context values.
- Represent field paths as typed segments, not raw strings during rendering.
- Represent format specs as typed enums:
  - integer padding
  - date/datetime formatting
  - explicit mode
  - conditional text for master/dolby flags
- Perform path sanitization segment-by-segment after formatting.
- Preserve path structure from `/` separators.
- Collapse forbidden filesystem characters matching current Python behavior:
  `\ / : " * ? < > |`
- Collapse repeated dots inside a segment.
- Remove trailing dots and spaces.
- Collapse repeated spaces.
- Use `_` when a rendered segment becomes empty.
- Return a typed error for missing context, unknown field, bad format spec,
  unsupported value type, and empty template.
- Avoid per-item reparsing. CLI/config should compile templates before loops.
- Do not read the filesystem.

AST shape:

```rust
pub enum TemplateNode {
    Literal(String),
    NamespaceValue {
        namespace: ValueNamespace,
        format: Option<FormatSpec>,
    },
    Field {
        field: TemplateField,
        format: Option<FormatSpec>,
    },
}

pub struct TemplateField {
    pub namespace: TemplateNamespace,
    pub field: FieldSegment,
}

pub struct FieldSegment(String);
pub struct ExtraFieldName(String);

pub enum TemplateNamespace {
    Item,
    Album,
    Playlist,
}

pub enum ValueNamespace {
    Now,
    Extra(ExtraFieldName),
}

pub enum FormatSpec {
    IntPad { width: u8, fill: IntFill },
    DateTime(ParsedDateTimeFormat),
    Explicit(ExplicitFormat),
    ConditionalText(String),
}

pub struct ParsedDateTimeFormat {
    pub raw: String,
    pub items: Vec<time::format_description::OwnedFormatItem>,
}

pub enum IntFill {
    Zero,
    Space,
}

pub enum ExplicitMode {
    Short,
    Long,
    Full,
}

pub struct ExplicitFormat {
    pub mode: ExplicitMode,
    pub upper: bool,
}
```

`TemplateValue` shape:

```rust
use std::borrow::Cow;

pub enum TemplateValue<'a> {
    Text(Cow<'a, str>),
    Integer(i64),
    Explicit(Option<bool>),
    Conditional(bool),
    Date(time::Date),
    DateTime(time::OffsetDateTime),
    None,
}
```

Format specs must be validated against value types before rendering:

- `IntPad` applies only to `Integer`.
- `IntPad` supports only zero fill and space fill. Do not allow arbitrary
  Unicode fill characters.
- No format spec or a text/date default format can render `Text`, `Integer`,
  `Date`, `DateTime`, or `None` according to documented defaults.
- `DateTime` applies only to `Date` and `DateTime`.
- `Explicit` applies only to `Explicit`.
- `ConditionalText` applies only to `Conditional`.
- Fields that are structurally absent use `TemplateValue::None`. Rendering
  `None` without a format spec produces an empty string. Rendering `None` with a
  typed format spec returns `TemplateError::TypeMismatch`.

Validate `DateTime` format strings during `CompiledTemplate::parse` by calling
`time::format_description::parse_owned` or the equivalent owned parser for the
selected `time` crate version. Store the parsed owned format description in
`FormatSpec::DateTime`; keep the raw string only for diagnostics. Return
`TemplateError::InvalidFormatSpec` if the format description is rejected. Date
format errors must not be deferred until render time, and render must not
reparse date format strings per item.

`TemplateNode::NamespaceValue` represents placeholders without a dot:

- `{now}` maps to `ValueNamespace::Now`.
- `{type}`, `{mix_id}`, and later top-level extras map to
  `ValueNamespace::Extra(ExtraFieldName)`.

`TemplateNode::Field` represents dotted fields only, such as `{item.number}`,
`{album.date}`, and `{playlist.updated}`.

Exact explicit formatting:

- `{item.explicit}` uses `ExplicitMode::Short`: explicit -> `E`, clean ->
  empty, unknown -> empty.
- `{item.explicit:long}`: explicit -> `explicit`, clean -> empty, unknown ->
  empty.
- `{item.explicit:upper}`: explicit -> `EXPLICIT`, clean -> empty, unknown ->
  empty.
- `{item.explicit:full}`: explicit -> `explicit`, clean -> `clean`, unknown ->
  empty.
- comma-combined Python specs are supported for parity:
  - `upper` alone parses as `ExplicitFormat { mode: ExplicitMode::Long, upper:
    true }`, so `{item.explicit:upper}` renders `EXPLICIT`, not `E`
  - `{item.explicit:long,upper}` -> `EXPLICIT` for explicit content
  - `{item.explicit:full,upper}` -> `EXPLICIT` for explicit content and
    `CLEAN` for clean content
  - recognized comma-separated features are accepted in any order, so
    `upper,long` is equivalent to `long,upper`
  - specifying more than one mode feature, such as `long,full`, fails during
    parse with `TemplateError::InvalidFormatSpec`; only one of `long`, `full`,
    or no mode may appear alongside `upper`
  - unknown comma features fail during parse with
    `TemplateError::InvalidFormatSpec`

`{album.master:<text>}` and `{item.dolby:<text>}` use
`FormatSpec::ConditionalText(<text>)`; true renders the text and false renders
empty.

Use byte/character-level matching and `str` operations for sanitization. Do not
add `regex` for this slice.

Context model:

- Build `TemplateContext` through pure builders from `riptide-core` catalog
  entities.
- Keep item, album, playlist, `now`, and extra fields as distinct namespaces.
- Avoid raw dynamic maps for stable fields.
- Allow a small typed extra-field map for `{type}`, `{mix_id}`, and later export
  use cases.

Builder API:

```rust
TemplateContext::new(now: time::OffsetDateTime)
    .with_track(&Track)
    .with_video(&Video)
    .with_album(&Album)
    .with_effective_quality_label(Option<ProviderQualityLabel>)
    .with_playlist(&Playlist, PlaylistIndex)
    .with_extra(ExtraFieldName, TemplateValue)
```

pub struct PlaylistIndex(u32);

Convenience constructors may wrap this builder, such as
`TemplateContext::for_track(track, album, now)` and
`TemplateContext::for_playlist_item(item, playlist, index, now)`, but tests
should target the builder first because it keeps required context explicit.

`PlaylistIndex` is 1-based for user-visible output, matching Python's playlist
item index/template behavior. It rejects zero.

`with_track` and `with_video` are separate methods so the type system prevents
passing `CatalogEntity::Album`, `CatalogEntity::Artist`, `CatalogEntity::Playlist`,
or `CatalogEntity::Collection` into the `item` namespace.

Missing namespace policy:

- If `with_album` is not called, `album.*` fields render as empty strings.
- If `with_playlist` is not called, `playlist.*` fields render as empty strings.
- If neither `with_track` nor `with_video` is called and the template references
  `item.*`, return `TemplateError::MissingContext`.

Item field mapping:

- `Track.explicit: Option<bool>` and `Video.explicit: Option<bool>` map to
  `TemplateValue::Explicit`.
- `Some(true)` means explicit, `Some(false)` means clean, and `None` means
  unknown.
- `{item.title_version}` is `"{title} ({version})"` when the track version is
  non-empty, otherwise it is the plain title.
- `{item.bpm}` renders `0` when TIDAL omits BPM, matching Python.
- `{item.quality}` renders the provider/TIDAL label used for the request or
  returned item, such as `LOSSLESS` or `HI_RES_LOSSLESS`; do not render Rust enum
  variant names.
- `{item.artists}` and `{item.features}` are alphabetically sorted before being
  joined with `, `, matching Python's `sorted(...)` behavior.
- `{item.artists_with_features}` joins sorted main artists followed by sorted
  featured artists.
- `{album.master:<text>}` is true only when `HIRES_LOSSLESS` is present in
  TIDAL `mediaMetadata.tags` and the effective Python/TIDAL quality label is
  `MAX`, matching Python's `format.py` rule.
- The converter must not decide `album.master` by itself because it has no
  config/request context. The CLI/list/download path passes the effective
  quality label into `TemplateContext` through `with_effective_quality_label`.
  For list commands, use the configured/requested track quality after Python
  migration mapping. If no quality context is available, pass `None` and leave
  `album.master` false.
- For `album.master` determination, `AudioQualityRequest::HiResLossless` maps
  to `ProviderQualityLabel("MAX")`; other request tiers map to their
  provider/TIDAL-equivalent labels but do not satisfy the Python `MAX` rule.

Initial compatibility fields:

- `item`
- `album`
- `playlist`
- `now`
- typed extras

The template crate must not depend on `riptide-provider-tidal`.

### `riptide-provider-tidal`

Purpose: TIDAL-specific auth, API models, typed conversions, and provider trait
implementation.

Public API candidates:

```rust
TidalProvider
TidalClient
TidalAuthClient
TidalTokenStore
TidalProviderOptions
TidalCredentials
TidalApiCache
TidalResource
TidalError
```

Modules:

- `auth.rs`: device authorization, token polling, refresh, logout, token model.
- `client.rs`: typed HTTP client and request execution.
- `cache.rs`: redb-backed response cache and pure cache-key construction.
- `model.rs`: serde models that mirror TIDAL JSON responses.
- `convert.rs`: conversion from TIDAL models into `riptide-core` entities.
- `resource.rs`: URL/path parsing for `track`, `album`, `playlist`, `artist`,
  `video`, and `mix` inputs.
- `provider.rs`: `Resolver`, `CollectionProvider`, and `LyricsProvider`
  implementation. `StreamProvider` should return `UnsupportedCapability` in
  this slice because stream manifest handling belongs to Slice 4.

Keep BTS, DASH, and HLS manifest parsing out of this crate in this slice.

Do not expose `TidalAuthSession` in this slice. Successful device flow and token
refresh operations return the typed persisted token state owned by
`TidalTokenStore`; authenticated API context is represented by that token state
plus `CountryCode` and `TidalUserId`.

`TidalProvider` construction should accept typed dependencies, not discover
paths internally:

```rust
pub struct TidalProviderOptions {
    pub auth_client: TidalAuthClient,
    pub api_client: TidalClient,
    pub token_store: TidalTokenStore,
    pub cache: TidalApiCache,
}
```

The CLI resolves `--auth-file`, `--cache-file`, default app-data paths, hidden
base URLs, and credentials, then constructs `TidalTokenStore`,
`TidalAuthClient`, `TidalClient`, `TidalApiCache`, and finally `TidalProvider`.
Tests can therefore pass temp paths and mock URLs deterministically.

## TIDAL Auth Design

### Credentials

Represent client credentials as:

```rust
TidalClientId
TidalClientSecret
TidalCredentials
```

Load order:

1. `RIPTIDE_TIDAL_CLIENT_ID` and `RIPTIDE_TIDAL_CLIENT_SECRET`.
2. Embedded riptide-rs app credentials, decoded from an internal base64 payload.

If exactly one explicit credential variable is set, auth commands fail with an
actionable message. Do not support combined credential environment variables.

Check whether the selected TIDAL credentials require PKCE before implementing
device auth. If required, add typed `CodeVerifier`, `CodeChallenge`, and
`CodeChallengeMethod::S256` values and include `code_challenge` and
`code_challenge_method` in the authorization request.

### Device Flow

Implement:

- `POST /v1/oauth2/device_authorization`
- poll `POST /v1/oauth2/token` with device grant
- send `scope = "r_usr+w_usr+w_sub"` in both the device authorization request
  and the token grant/refresh form fields, matching Python
  `riptide/core/auth/client.py`
- handle `authorization_pending` by waiting the typed polling interval
- handle `slow_down` by increasing the polling interval by exactly 5 seconds,
  matching RFC 8628 section 3.5
- handle `expired_token`, `access_denied`, invalid credentials, and network
  failures with structured errors
- stop polling at the device-code expiry

Use typed values:

```rust
DeviceCode
UserCode
VerificationUri
VerificationUriComplete
PollingInterval(std::time::Duration)
TokenExpiry(time::OffsetDateTime)
AccessToken
RefreshToken
CountryCode
TidalUserId
```

No raw token strings in logs, `Debug`, or errors.

### Refresh and Logout

Implement:

- refresh token grant
- logout request against `https://api.tidal.com/v1/logout`
- token-store clearing after successful logout

Refresh should be available as both:

- explicit CLI command
- automatic API-client path when a read-only API call returns `401`

Automatic refresh must retry the original request at most once.

Before each API call, check whether the access token expires within a typed
proactive refresh threshold. Use 60 seconds initially:

```rust
ProactiveRefreshThreshold(Duration::from_secs(60))
```

If the token expires within that threshold, refresh before the API request. Keep
reactive refresh on one `401` as a fallback for clock skew or revoked tokens.

If proactive or reactive refresh fails, return a structured refresh error that
includes the operation context and source refresh failure. Do not return the
original `401` alone. For reactive refresh, include the original endpoint and
the fact that the request was not retried successfully.

### Token Store

Define a JSON shape owned by `riptide-provider-tidal`, not `riptide-cli`.

The CLI decides the token-store path:

- default platform app-data path through `directories`
- override through `--auth-file <path>`

Tests must use `--auth-file` or a temporary store path; no test writes to the
real app-data path.

Atomic writes must create the temporary file in the same directory as the target
token file, using `tempfile::NamedTempFile::new_in(parent_dir)`, then persist or
rename into place. Do not create the temp file in `/tmp`, because cross-filesystem
renames are not atomic.

`CountryCode` is a typed two-character ISO 3166-1 alpha-2 value. It rejects
empty strings, non-ASCII values, non-uppercase values, and values whose length is
not exactly two characters. Its serialized/cache-key form is the uppercase
two-character string, such as `US`, `BE`, or `SG`.

## TIDAL API Design

### HTTP Client

Use `reqwest` with a typed request builder. Every API call should be one of:

```rust
ApiCachePolicy::NoCache
ApiCachePolicy::Ttl(Duration)
ApiCachePolicy::Revalidate
```

Do not expose cache config keys yet. The policy is endpoint-owned in this slice.

Policy behavior:

- `NoCache`: never read from cache and never write the response to cache.
- `Ttl(duration)`: read cache first; if a non-expired entry exists, return it;
  otherwise fetch fresh and write the response with the requested TTL.
- `Revalidate`: always fetch fresh and update cache, then return the fresh
  response. Do not serve stale data and do not issue background refreshes in this
  slice.

`Revalidate` is not coherent under concurrent readers in this slice: another
process can read the previous cached entry while a fresh fetch is in flight.
This is acceptable for the first single-command CLI implementation. `redb`
file-level locking prevents simultaneous writers but not stale reads by a racing
process.

Endpoint defaults:

- `sessions`: no cache
- search: no cache
- favorites: revalidate
- playlists and playlist items: revalidate
- tracks, videos, albums, artists, album items, artist albums/videos, lyrics:
  TTL cache with the riptide-rs default TTL of 3600 seconds

Endpoint pagination defaults, matching `riptide/core/api/api.py`:

- artist albums: default limit 10, max 100
- artist videos: default limit 10, max 100
- album items: default limit 20, max 100
- playlist items: default limit 20, max 100
- mix items: default limit 20, max 100

Request planning must clamp caller-provided limits to the endpoint max before
building query parameters.

Normalize query params before cache-key construction. Cache keys must not include
access tokens.

Rate limiting:

- On HTTP `429`, read the `Retry-After` header if present.
- If `Retry-After` is absent or invalid, use a typed default backoff of 2
  seconds.
- Sleep for the backoff duration and retry the request once.
- If the retry also returns `429`, return `TidalError::RateLimited`, which maps
  to `CoreError::Provider { kind: ProviderErrorKind::RateLimited, ... }` at
  provider boundaries.
- Do not retry non-idempotent auth token writes under this API retry policy.

### Cache Storage and Eviction

`redb` is synchronous. All `redb` open, read, and write operations called from
async paths must run inside `tokio::task::spawn_blocking` so cache I/O and write
transactions do not block Tokio worker threads.

Opening a cache file can fail when another process holds `redb`'s exclusive file
lock. This must return a descriptive `TidalError::CacheUnavailable` or equivalent
structured error. Do not panic, and do not silently disable cache for that
process.

`redb` has no built-in TTL eviction. Use on-read eviction in this slice:

- cache read checks the stored expiry before returning a hit
- expired entries are treated as misses
- expired entries are deleted during the same cache access when a write
  transaction is available
- if deletion fails because another writer is active, return the cache miss and
  leave cleanup to a later access

Do not add a size limit or LRU policy in this slice.

Batch cache writes for multi-endpoint user operations where practical. For
example, `list album <id>` may fetch album metadata and album items; collect
cacheable responses from that command path and commit them in one write
transaction instead of opening one write transaction per endpoint. The client
API should still allow single-request cache writes for simple calls.

Cache write API shape:

```rust
pub struct CacheEntryToWrite {
    pub key: CacheKey,
    pub value: serde_json::Value,
    pub expires_at: time::OffsetDateTime,
}

impl TidalApiCache {
    pub async fn get(&self, key: &CacheKey) -> Result<Option<serde_json::Value>>;
    pub async fn put(&self, entry: CacheEntryToWrite) -> Result<()>;
    pub async fn put_batch(&self, entries: Vec<CacheEntryToWrite>) -> Result<()>;
}
```

`put_batch` must use one `redb` write transaction for all entries. Avoid a
commit-on-drop handle in this slice because explicit `put_batch` is easier to
test and less surprising.

### Read-Only API Surface

Implement the read-only methods needed by the current Python `list` command and
future download planning:

- session
- track by ID
- video by ID
- album by ID
- album items
- album item credits if the current entity model needs credit mapping
- playlist by UUID
- playlist items
- artist by ID
- artist albums
- artist videos
- mix items
- search
- favorites IDs
- track lyrics

Do not implement playback/stream endpoints in this slice except as typed stubs
returning unsupported capability through the provider trait.

Search results should preserve TIDAL's distinct top hit instead of flattening it
into an unstructured list:

```rust
pub struct SearchResults {
    pub top_hit: Option<CatalogEntity>,
    pub tracks: Vec<Track>,
    pub albums: Vec<Album>,
    pub playlists: Vec<Playlist>,
    pub artists: Vec<Artist>,
    pub videos: Vec<Video>,
}
```

`TidalClient::search` and CLI `list search` may use `SearchResults` directly.
If a future provider-neutral search trait is added, it should preserve this
shape or an equivalent top-hit field. Do not rely on "top hit is first item in a
Vec" as an implicit convention.

Favorites depend on the authenticated user's TIDAL user ID. `TidalProvider`
must either hold user/session data loaded from the token store or fetch
`sessions` before calling `users/{user_id}/favorites/ids`. The dependency should
be explicit in the method implementation and tests.

Pagination memory model for this slice: provider collection methods may return a
fully loaded `Vec<CatalogEntity>` because the current provider trait is not a
streaming trait. Keep pagination request planning pure and page-by-page, but do
not introduce a lazy stream abstraction until Slice 8 revisits provider
capabilities and collection APIs.

### Model Conversion

Keep raw TIDAL JSON models separate from provider-neutral `riptide-core`
entities.

TIDAL serde model policy:

- Do not use `#[serde(deny_unknown_fields)]` on TIDAL response structs.
- Use `Option<T>` and `#[serde(default)]` for fields that may be absent.
- Keep fields we depend on typed and validated during conversion into
  `riptide-core`.
- Preserve truly provider-specific fragments in `ProviderExtras` only when they
  are useful for diagnostics or later mapping.

Conversion rules:

- TIDAL numeric IDs become `MediaRef { provider: tidal, kind, id }`.
- TIDAL artist `type = MAIN` maps to `ArtistRole::Main`.
- TIDAL artist `type = FEATURED` maps to `ArtistRole::Featured`.
- TIDAL contributors without IDs map to `Artist { id: None, name }` in
  `ArtistCredit`.
- TIDAL album `ALBUM`, `SINGLE`, and `EP` map to provider-neutral release
  labels without changing `CollectionKind` playlist rules.
- TIDAL `track.duration`, `video.duration`, and `album.duration` integer seconds
  map to `MediaDuration(std::time::Duration::from_secs(...))`.
- TIDAL `album.releaseDate` ISO date strings map to `time::Date`.
- TIDAL `playlist.created`, `playlist.lastUpdated`, and item added timestamps
  should be parsed as ISO 8601 strings when returned in the existing Python
  model shape. If TIDAL returns Unix milliseconds for a fixture, add a typed
  parser that accepts that shape explicitly and cover it with a regression test.
- TIDAL `playlist.lastUpdated` maps to the `playlist.updated` template field.
- TIDAL quality labels map as:
  - `LOW` -> `AudioQuality::Low`
  - `HIGH` -> `AudioQuality::High`
  - `LOSSLESS` -> `AudioQuality::Lossless`
  - `HI_RES`, `HIRES_LOSSLESS`, and `HI_RES_LOSSLESS` ->
    `AudioQuality::HiResLossless`
  - unknown labels -> `AudioQuality::ProviderSpecific(ProviderQualityLabel)`
- TIDAL video quality labels map to `VideoQuality` where known and
  `VideoQuality::ProviderSpecific` otherwise.
- Date, time, duration, and quality conversion failures for fields required by
  the core model must produce `TidalError::InvalidResponse`.
- TIDAL `mediaMetadata.tags` should set typed flags for explicit template
  support where the current core model supports it. If the current core model is
  missing a necessary flag, add a typed field in `riptide-core` in this slice
  with tests.
- TIDAL lyrics provider strings, such as `Musixmatch`, map to
  `LyricsSource::External(LyricsSourceName)`.

### Error Mapping

Keep `TidalError` as the provider crate's structured error enum. Add a
provider-error escape hatch to `riptide-core` before implementing provider
methods:

```rust
CoreError::Provider {
    provider: ProviderId,
    kind: ProviderErrorKind,
    source: Box<dyn std::error::Error + Send + Sync>,
}
```

`ProviderErrorKind` should cover at least:

- `Authentication`
- `Authorization`
- `RateLimited`
- `Unavailable`
- `NotFound`
- `Restricted`
- `InvalidResponse`
- `UnsupportedCapability`

`TidalError` converts to `CoreError::Provider` at provider trait boundaries.
Library callers that use `TidalClient` directly can still match on
`TidalError`.

## CLI Design

Extend `riptide-rs` with:

```text
riptide-rs auth login
riptide-rs auth refresh
riptide-rs auth logout
riptide-rs auth status
riptide-rs list track <id>
riptide-rs list video <id>
riptide-rs list album <id>
riptide-rs list playlist <uuid>
riptide-rs list artist <id>
riptide-rs list search <query>
riptide-rs template check <template>
riptide-rs template render-fixture <template> --fixture <path>
```

Common flags:

```text
--auth-file <path>
--cache-file <path>
--api-base-url <url>      # hidden, tests only
--auth-base-url <url>     # hidden, tests only
```

The hidden base URL flags are required for integration tests and must not appear
in generated user-facing config.

`auth login` behavior:

- print verification URL and user code
- do not require browser launch
- poll until success, denial, expiry, or Ctrl-C
- persist token atomically after success
- print user ID and country code, not tokens

Use graceful Ctrl-C handling for `auth login`: run the polling sleep/request loop
inside a `tokio::select!` branch with `tokio::signal::ctrl_c()`. On Ctrl-C,
print a cancellation message, do not write a token file, and exit with code 130,
matching the POSIX SIGINT convention.

`auth status` behavior:

- report logged-in/logged-out
- report expiry time and country code if available
- never print access or refresh tokens

`list` behavior:

- if `--template` is provided, compile the template as the first step before
  loading the token, refreshing, or making any API call
- if template compilation fails, return the `TemplateError` immediately without
  making a network request
- load token
- refresh if expired or if the API returns one `401`
- fetch catalog data
- print rows rendered through the default riptide-rs list template unless
  `--template` is provided
- optionally accept `--template <template>` only if template compilation is
  wired in that command path

Default list template for this slice, matching `Config.ListConfig.format` in
`riptide/cli/config.py`:

```text
{item.id} | {item.artist} | {album.title} | {item.number:02d} | {item.title}
```

Track, video, album item, playlist item, artist item, and mix item rows should
all use this template path, matching `riptide/cli/commands/list.py`. Missing
album fields render as empty strings according to the template missing-namespace
policy.

`list search` output is grouped by resource type in this order:

1. top hit, when TIDAL returns one
2. tracks
3. albums
4. playlists
5. artists
6. videos

Each group should have a stable plain-text header and rows sorted in the order
returned by TIDAL. Do not interleave result types.

`list artist <id>` should follow the Python command's traversal rather than
printing an artist summary row by default:

1. fetch the artist to validate the resource and drive later diagnostics
2. list tracks/videos from artist albums with the default or provided template
3. include singles/EPs only when the wired filter requests them
4. include videos only when the wired video filter requests them

If the Rust CLI does not wire the singles/videos filters in this slice, document
the defaults as `singles = none` and `videos = none`, and add
the filters in the same slice that wires their runtime behavior.

`template render-fixture` reads a JSON template context fixture from the
required `--fixture <path>` flag. Do not embed permanent fixture data in the
binary. Test fixtures may live under the template crate or CLI integration test
fixtures.

Fixture JSON schema for `template render-fixture`:

```json
{
  "now": "2024-01-15T12:00:00Z",
  "item": {
    "kind": "track",
    "id": "123",
    "title": "Track Title",
    "number": 1,
    "volume": 1,
    "version": "Radio Edit",
    "copyright": "2024 Label",
    "bpm": 120,
    "isrc": "USABC2400001",
    "quality": "LOSSLESS",
    "artist": "Main Artist",
    "artists": "Main Artist",
    "features": "Guest Artist",
    "artists_with_features": "Main Artist, Guest Artist",
    "explicit": true,
    "dolby": false
  },
  "album": {
    "id": "456",
    "title": "Album Title",
    "artist": "Main Artist",
    "artists": "Main Artist",
    "date": "2024-01-01",
    "explicit": true,
    "master": false,
    "release": "ALBUM"
  },
  "playlist": null,
  "extras": {
    "type": "track",
    "mix_id": null
  }
}
```

Fixture parsing should use the same `TemplateContext` builder path as runtime
rendering so tests exercise the real field mapping. Fields such as
`title_version` must be computed by the builder from `title` and `version`, not
precomputed in fixture JSON. `null` maps to `TemplateValue::None`; absent
optional namespaces behave according to the missing namespace policy.

CLI command edges may use `anyhow::Context`. Library crates must keep structured
error enums.

## Config Changes

Do not add persistent template config in this slice.

Rationale:

- Download, cover, and M3U templates are not consumed by runtime download,
  cover, or M3U behavior yet.
- Adding `[templates.download]` or `[templates.cover]` now would create accepted
  but mostly inert knobs.
- Template behavior can be fully exercised through library tests, golden
  fixtures, `template check`, `template render-fixture`, and `list --template`.

When download templates are added in a later slice, all configured templates
must compile during config validation. Invalid template syntax must be a config
error with key-path context.

Do not add auth file or cache file to TOML in this slice unless default path
resolution and runtime use are implemented in the same slice. Prefer CLI flags
for test isolation.

`riptide-config::ValidatedConfig` should not gain `[tidal]`, `[auth]`,
`[cache]`, or `[templates]` sections in this slice. Credentials come from
environment variables; token and cache paths come from CLI flags or default
runtime path resolution. If any config field is added during implementation, it
must be parsed, validated, and consumed by runtime behavior in the same patch.

This slice must preserve the Slice 0-1 config contract: unsupported quality
names such as `normal`, `max`, `sd`, `hd`, and `fhd` fail as invalid values with
key-path context. Do not add compatibility aliases or migration shims.

## Test Plan

Follow TDD: write focused tests for parser, pure conversion, cache keys, and
token-store behavior before wiring CLI side effects.

### Template Unit Tests

- Empty template fails.
- Literal-only template renders.
- Unknown namespace fails.
- Unknown field fails.
- Invalid format spec fails at compile time.
- Invalid date/datetime format specs fail during `CompiledTemplate::parse`, not
  during render.
- Integer padding renders `{item.number:02d}`.
- Unsupported integer fill characters fail during parse.
- Date formatting renders album and playlist dates.
- `Explicit(None)` renders empty.
- `Explicit(true)` renders `E`, `explicit`, or `EXPLICIT` based on spec.
- `Explicit(false)` renders empty or `clean` for `full`.
- combined explicit specs such as `{item.explicit:long,upper}` and
  `{item.explicit:full,upper}` match Python behavior.
- `Conditional(true)` renders the format-spec text.
- `Conditional(false)` renders empty.
- `{item.title_version}` includes the version in parentheses only when present.
- missing `{item.bpm}` renders `0`.
- `{item.quality}` renders the raw/effective provider quality label, not Rust
  enum variant names.
- `{item.artists}`, `{item.features}`, and `{item.artists_with_features}` use
  stable alphabetical ordering.
- `PlaylistIndex` rejects zero and renders user-facing one-based positions.
- Path segment sanitization removes forbidden characters.
- Repeated dots collapse.
- Trailing dots/spaces are removed.
- Empty segment becomes `_`.
- Template compiles once and can render multiple contexts.
- Missing `album` or `playlist` namespace renders fields as empty strings.
- Missing `item` namespace returns `TemplateError::MissingContext`.

### Template Golden Tests

Create fixtures from the Python implementation for representative track, video,
album, playlist, and mix contexts.

Golden coverage:

- default track path
- default album item path
- default playlist item path
- explicit track path
- clean track path using `{item.explicit:full}`
- Dolby conditional path
- master conditional album path
- master conditional uses Python's `HIRES_LOSSLESS` tag plus effective `MAX`
  quality rule.
- video path with no album
- playlist index padding
- path sanitization edge cases

Golden tests should compare rendered path strings, not internal AST structure.

### CLI Template Tests

- `template check <template>` succeeds for a valid template.
- `template check <template>` fails with context for invalid syntax.
- `template render-fixture <template>` renders a stable fixture path.
- `template render-fixture <template> --fixture <path>` reads fixture JSON from
  the provided path.
- `list --template <template>` compiles once before fetching/rendering rows.
- invalid `list --template <template>` returns before token loading, refresh, or
  any API request
- Unsupported template fields fail with unsupported-field context.

### Config Validation Tests

- `download.track_quality = "normal"` fails as an invalid value.
- `download.track_quality = "max"` fails as an invalid value.
- `download.video_quality = "sd"` fails as an invalid value.
- `download.video_quality = "hd"` fails as an invalid value.
- `download.video_quality = "fhd"` fails as an invalid value.

### Auth Unit Tests

- Credentials load from `RIPTIDE_TIDAL_CLIENT_ID` and
  `RIPTIDE_TIDAL_CLIENT_SECRET`.
- Missing explicit credentials fall back to the embedded riptide-rs app
  credentials.
- A partial explicit credential pair fails with actionable error.
- `CountryCode` accepts uppercase two-character ASCII codes and rejects empty,
  lowercase, non-ASCII, and wrong-length values.
- Token wrappers redact `Debug`.
- Token expiry calculations use typed time.
- Token store missing file returns logged-out state.
- Token store write is atomic.
- Token store read/write round trip preserves non-secret metadata.
- Corrupt token JSON fails with file-path context.

### Auth Mock Integration Tests

Use a local mock HTTP server.

Cover:

- device authorization request method, path, form fields, and credentials
- device authorization and token grant requests include
  `scope = "r_usr+w_usr+w_sub"`
- successful poll after one or more `authorization_pending` responses
- `slow_down` increases interval without busy polling
- `slow_down` increases interval by exactly 5 seconds
- expired device code exits cleanly
- access denied exits cleanly
- refresh token request shape
- logout request shape
- proactive refresh occurs before an API request when the access token expires
  within 60 seconds
- automatic refresh on one `401`, then original request retry
- automatic refresh failure returns the refresh error with original endpoint
  context
- automatic refresh does not retry indefinitely
- CLI `auth login --auth-file <temp> --auth-base-url <mock>` persists token
- CLI `auth refresh --auth-file <temp> --auth-base-url <mock>` updates token
- CLI `auth logout --auth-file <temp> --auth-base-url <mock>` clears token

### API Unit Tests

- Cache key construction is stable regardless of query param insertion order.
- Cache key excludes access token.
- Cache TTL expiry is honored.
- TTL-cache endpoints use the riptide-rs 3600 second TTL.
- Expired cache entries are treated as misses and removed on read when a write
  transaction is available.
- Multi-endpoint command paths can batch cache writes into one write
  transaction.
- Cache file-lock contention returns a descriptive cache-unavailable error.
- Integration tests exercise cache read/write through async command paths;
  `spawn_blocking` wrapping is verified in code review because it is a code
  property rather than a reliable runtime assertion.
- No-cache policy never reads or writes cache.
- Revalidate policy always fetches fresh and writes the fresh response.
- JSON model fixtures parse for track, video, album, playlist, artist, search,
  favorites, session, and lyrics.
- TIDAL-to-core conversion maps IDs, roles, dates, durations, quality labels,
  and lyrics source correctly.
- TIDAL search conversion preserves `SearchResults::top_hit` separately from
  type-grouped result lists.
- `TidalError` maps to `CoreError::Provider` with the expected
  `ProviderErrorKind`.
- Pagination request planning is pure and clamps endpoint limits.
- pagination defaults match Python: artist albums/videos 10, album/playlist/mix
  items 20, and all max limits 100.

### API Mock Integration Tests

- `list track <id>` fetches and prints a stable row.
- `list album <id>` fetches album plus items.
- `list playlist <uuid>` fetches playlist plus items.
- `list artist <id>` follows Python traversal: fetch artist, list album tracks
  with the configured list template, and honor wired singles/videos defaults.
- `list search <query>` fetches search and prints grouped results.
- `list search <query>` prints groups in the documented order: top hit, tracks,
  albums, playlists, artists, videos.
- API errors include endpoint, status, and TIDAL user message.
- HTTP `429` with `Retry-After` sleeps once, retries once, and succeeds if the
  retry succeeds.
- HTTP `429` without `Retry-After` uses the typed default backoff.
- Repeated HTTP `429` returns a rate-limited provider error.
- Debug output never includes tokens.
- Cache hit avoids the second HTTP request for TTL-cache endpoints.
- `Revalidate` endpoints always fetch fresh and update cache.

### Gated Live TIDAL Auth Flow

At the end of the slice, run a live manual-gated test against TIDAL. This is not
part of default `nix flake check` because it requires human approval in a browser
and real account access.

Provide a script or documented command:

```text
RIPTIDE_TIDAL_CLIENT_ID=... \
RIPTIDE_TIDAL_CLIENT_SECRET=... \
cargo test -p riptide-provider-tidal --test live_auth_flow -- --ignored
```

The live test must:

1. Request a device code from TIDAL.
2. Print the verification URL and user code.
3. Poll until the user completes authorization or the code expires.
4. Persist tokens to a temporary auth file.
5. Use the access token to call `sessions`.
6. Force a refresh-token grant.
7. Use the refreshed token to call `sessions` again.
8. Logout or clear the temporary token file.
9. Assert that no token value appears in test logs or panic messages.

If TIDAL refuses logout for the token shape, the test may clear the temporary
file but must record the observed behavior in a test comment and issue note.

Completion criteria for this slice include a recorded successful run of this
ignored test, with timestamp and command in the implementation summary.

## Implementation Order

1. Add `riptide-template` crate skeleton and public error types.
2. Write parser tests for literals, fields, namespaces, and invalid syntax.
3. Implement template AST parser.
4. Write pure render tests for scalar, integer, date, explicit, and conditional
   formatting.
5. Implement typed template context and renderer.
6. Write sanitizer tests and golden Python parity fixtures.
7. Implement segment sanitizer and golden rendering.
8. Amend `riptide-config` Python quality migration errors for `normal`, `max`,
   `sd`, `hd`, and `fhd`, with regression tests.
9. Wire `template check`, `template render-fixture`, and `list --template`
   without adding persistent template config.
10. Add `riptide-provider-tidal` typed auth models and credential tests.
11. Implement credential loading, optional PKCE support if required, device auth
    request construction, token wrappers, `CountryCode`, and token-store
    read/write.
12. Add mock auth integration tests, then implement polling, refresh, logout,
    and CLI auth commands.
13. Add typed TIDAL API JSON fixtures and conversion tests.
14. Add redb cache-key, policy, TTL, eviction, lock-error, and batch-write tests,
    then implement `TidalApiCache`.
15. Implement `TidalClient`, endpoint methods, cache integration, rate-limit
    handling, and TIDAL-to-core conversions.
16. Implement `TidalProvider` trait methods for resolve, collection expansion,
    and lyrics. Return structured unsupported errors for streams.
17. Add CLI `list` and `template` commands with integration tests.
18. Add ignored live auth-flow test and document exact command.
19. Run full checks and the live auth-flow check.
20. Review public item docs and error messages before committing.

## Performance Requirements

- Templates must be compiled once before item loops.
- Rendering should borrow strings from context where practical through
  `TemplateValue<'a>`.
- Path sanitization should use direct byte/character checks and `str`
  operations for the fixed cleanup rules. Do not use regex in this slice.
- Cache-key construction should avoid JSON serialization in per-request hot
  paths.
- Cache writes for multi-endpoint commands should be batched where practical to
  avoid repeated `redb` write transactions.
- API pagination should stream page handling through iterators or async loops
  without accumulating unnecessary intermediate strings.
- Token polling must sleep according to server interval; no busy loops.

## Risks and Mitigations

| Risk | Mitigation |
| --- | --- |
| TIDAL auth behavior changes or rejects credentials. | Keep credentials external through env vars, model OAuth errors structurally, and keep the live test ignored/gated. |
| Selected TIDAL credentials require PKCE. | Verify before implementation. Add typed verifier/challenge support if required. |
| Live auth test cannot run in CI. | Keep mock tests exhaustive and make the live test a documented release gate, not a default check. |
| Template compatibility expands into Python `str.format` reimplementation. | Support only documented fields/specs and fail unsupported specs at compile time. |
| Cache stores sensitive data. | Never include tokens in cache keys or values written for auth endpoints. Do not cache auth responses. |
| Expired redb cache entries accumulate. | Use on-read eviction for expired entries in this slice; defer size/LRU policy. |
| API model fixtures drift from TIDAL. | Keep serde models strict for fields we depend on and tolerant for provider extras where needed. Add fixture regression tests for every model bug. |
| Provider supertrait remains awkward for streams. | Return structured `UnsupportedCapability` for stream requests in this slice and preserve the Slice 8 review note. |
| Config grows unwired knobs. | Do not add template/auth/cache config in this slice; keep auth/cache paths as CLI runtime options for now. |

## Review Checklist

Before coding starts:

- Confirm supported placeholders and format specs.
- Confirm TIDAL credentials must be env-provided and no secret is committed.
- Confirm JSON token storage is acceptable for this pre-release slice.
- Confirm redb cache remains the right Slice 3 implementation choice.
- Confirm hidden test-only base URL flags are acceptable.
- Confirm live auth-flow test expectations and who will complete browser auth
  during final verification.
- Confirm whether selected TIDAL credentials require PKCE.
- Confirm no `riptide-http` crate should be introduced until a later
  provider-neutral HTTP slice.

## Completion Criteria

- `cargo fmt --check` passes.
- `cargo clippy --workspace --all-targets -- -D warnings` passes.
- `cargo nextest run --workspace` passes.
- `nix flake check` passes.
- Template golden tests pass.
- Auth and API mock integration tests pass.
- The ignored live TIDAL auth-flow test has been run successfully once and the
  implementation summary records the command and date.
- No token value appears in logs, errors, test output, or `Debug`.
- Public foundational items added in this slice have `///` docs before commit.
- The final commit includes only Slice 2-3 plan and implementation files.
