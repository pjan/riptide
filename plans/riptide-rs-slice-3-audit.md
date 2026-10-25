# Riptide-RS Code Review

## 1. The Workspace Architect (Config & Coherence)

### 1.1 Workspace Inheritance — Well Done

The root `Cargo.toml` centralizes `[workspace.package]` (edition, license, version) and `[workspace.dependencies]` correctly. All crate Cargo.toml files inherit via `.workspace = true`. **No issues found.**

### 1.2 Workspace Lints — Missing Pedantic/Nursery Baseline

`Cargo.toml` (root):39-42

The existing workspace lints deny `unwrap_used`, `expect_used`, `dbg_macro`, and `todo` — good. But there are no `clippy::pedantic` or `clippy::nursery` lints, and no `rustfmt.toml` or `.clippy.toml` for codified formatting rules. Consider:

```toml
[workspace.lints.clippy]
# existing
unwrap_used = "deny"
expect_used = "deny"
dbg_macro = "deny"
todo = "deny"
# suggested additions
pedantic = { level = "warn", priority = -1 }
nursery = { level = "warn", priority = -1 }
# then override specific noisy lints
module_name_repetitions = "allow"
must_use_candidate = "allow"
```

### 1.3 `http` Module Visibility is Inconsistent

`crates/riptide-provider-tidal/src/lib.rs` declares `mod http` as private but exposes every other module as `pub mod`. This is correct for `http` — it's an internal helper. However, there's a question of whether `convert` and `model` should be `pub`. They're TIDAL-internal response shapes and conversion logic. A consumer of `riptide-provider-tidal` shouldn't need to call `convert::track()` directly — they should use the `Provider` trait.

**Recommendation:** Make `convert` and `model` `pub(crate)`:

```rust
// crates/riptide-provider-tidal/src/lib.rs
pub(crate) mod convert;
pub(crate) mod model;
```

### 1.4 `tempfile` is a Runtime Dependency in the Provider Crate

`crates/riptide-provider-tidal/Cargo.toml`:14 — `tempfile` is listed under `[dependencies]` (used by `TidalTokenStore::save` for atomic writes). This is fine and intentional. However, it's also duplicated under `[dev-dependencies]` (line 17), which is redundant when it's already a normal dependency.

---

## 2. The Strict Rustacean (Idiomatic Rust & Typing)

### 2.1 BUG: `TidalClient::new()` Silently Swallows Errors

`crates/riptide-provider-tidal/src/client.rs`:46-65

```rust
pub fn new(...) -> Self {
    Self::try_new(api_base_url.clone(), auth.clone(), token_store.clone(), cache.clone())
        .unwrap_or_else(|_| Self {
            http: reqwest::Client::new(),
            ...
        })
}
```

If `try_new` fails (e.g., certificate bundle is corrupt), this silently falls back to a default client that may lack the required root certificates. The same pattern exists in `TidalAuthClient::new()` (`auth.rs`:359-364). This masks real failures.

**Recommendation:** Remove `new()` entirely and only expose `try_new()`. The CLI already calls `try_new()`:

```rust
impl TidalClient {
    /// Creates a client, loading extra PEM roots from certificate env vars.
    pub fn new(
        api_base_url: Url,
        auth: TidalAuthClient,
        token_store: TidalTokenStore,
        cache: TidalApiCache,
    ) -> TidalResult<Self> {
        Ok(Self {
            http: crate::http::client()?,
            api_base_url,
            auth,
            token_store,
            cache,
        })
    }
}
```

Same fix for `TidalAuthClient`.

### 2.2 BUG: `unwrap_or_default()` Masks Missing Refresh Tokens

`crates/riptide-provider-tidal/src/auth.rs`:494-499

```rust
let refresh_token = response.refresh_token.unwrap_or_else(|| {
    form.iter()
        .find(|(key, _)| *key == "refresh_token")
        .map(|(_, value)| (*value).to_owned())
        .unwrap_or_default()  // <--- empty string
});
Ok(TidalTokenState {
    refresh_token: RefreshToken::new(refresh_token)?, // fails on empty
    ...
})
```

If the server returns no `refresh_token` **and** the form doesn't contain one (e.g., initial device code exchange), `unwrap_or_default()` produces `""`, and `RefreshToken::new("")` returns `Err`. The error message will be unhelpful ("cannot be empty" on `RefreshToken`). This is technically caught, but the error context is poor.

**Recommendation:** Return a specific error:

```rust
let refresh_token = response
    .refresh_token
    .or_else(|| {
        form.iter()
            .find(|(key, _)| *key == "refresh_token")
            .map(|(_, value)| (*value).to_owned())
    })
    .ok_or(TidalError::InvalidResponse {
        field: "refresh_token".to_owned(),
        message: "server did not return a refresh token".to_owned(),
    })?;
```

### 2.3 BUG: `unwrap_or_default()` in `response_json` Swallows Error Bodies

`crates/riptide-provider-tidal/src/auth.rs`:549 and `client.rs`:386

```rust
let message = response.text().await.unwrap_or_default();
```

If reading the error body itself fails (e.g., connection reset mid-response), the actual transport error is silently dropped. Use:

```rust
let message = response.text().await.unwrap_or_else(|e| format!("<body read failed: {e}>"));
```

### 2.4 Repeated `ProviderId::new("tidal")` — Use a Constant

`ProviderId::new("tidal")` appears in at least 6 places across the provider crate (`convert.rs`:12-17, `resource.rs`:51, `error.rs`:136, `provider.rs`:38, `client.rs`:190). Each call re-validates the same literal. The pattern in `convert.rs` (`unreachable!`) and `error.rs` (`unreachable_provider_id()`) show the awkwardness.

**Recommendation:** Add a `const`-like helper in the provider crate root or use `std::sync::LazyLock`:

```rust
// crates/riptide-provider-tidal/src/lib.rs
use std::sync::LazyLock;
use riptide_core::ProviderId;

pub(crate) static TIDAL_PROVIDER_ID: LazyLock<ProviderId> = LazyLock::new(|| {
    // SAFETY: "tidal" is a non-empty string literal
    match ProviderId::new("tidal") {
        Ok(id) => id,
        Err(_) => unreachable!("literal provider ID"),
    }
});
```

Then replace all `ProviderId::new("tidal").map_err(...)` with `TIDAL_PROVIDER_ID.clone()`.

### 2.5 `ProviderCapabilities` Uses Booleans Instead of a Bitflags-style Enum

`crates/riptide-core/src/provider.rs`:61-77

Seven individual `bool` fields are easy to extend wrong (forget to set one). Consider `bitflags` or at minimum a builder pattern. Not urgent but worth noting for when capability count grows.

### 2.6 Unnecessary `.clone()` on `cli.command`

`crates/riptide-cli/src/main.rs`:131

```rust
match cli.command.clone() {
```

`Command` implements `Clone`, but this clones the entire enum (including `String` payloads) just to pattern-match. Since `main` doesn't need `cli.command` afterward, move instead:

```rust
match cli.command {
```

This requires changing `Cli`'s lifetime, but since it's only used once, the refactor is straightforward. At minimum, use `cli.command.as_ref()` or destructure via `match &cli.command`.

### 2.7 `#[allow(dead_code)]` on `assert_path_is_local`

`crates/riptide-cli/src/main.rs`:474

```rust
#[allow(dead_code)]
fn assert_path_is_local(path: &Path) -> &Path {
    path
}
```

This is a no-op. Either implement the check or remove the function.

---

## 3. The Functional Purist (Core vs. Shell Architecture)

### 3.1 Good: Library Crates Are I/O-Free (Mostly)

`riptide-core` and `riptide-template` have zero I/O — they operate on strings, typed inputs, and return `Result` values. This is excellent. Template rendering takes a `TemplateContext` and returns `RenderedPath` — no filesystem access.

### 3.2 `TidalTokenStore` Mixes I/O Into the Provider Library

`crates/riptide-provider-tidal/src/auth.rs`:260-340

`TidalTokenStore` directly reads/writes JSON files. This couples the library to filesystem I/O. A purer design would define a `trait TokenStore` with `load`/`save`/`clear`, then provide a `FileTokenStore` implementation in the CLI or a separate module.

**Recommendation for later refactoring:**

```rust
// In riptide-core or riptide-provider-tidal
#[async_trait]
pub trait TokenStore: Send + Sync {
    type Error: std::error::Error + Send + Sync;
    async fn load(&self) -> Result<Option<TidalTokenState>, Self::Error>;
    async fn save(&self, state: &TidalTokenState) -> Result<(), Self::Error>;
    async fn clear(&self) -> Result<(), Self::Error>;
}
```

This would make the `TidalClient` testable without touching the filesystem.

### 3.3 `TidalApiCache` Also Embeds I/O (redb)

`crates/riptide-provider-tidal/src/cache.rs` — Same pattern. The cache uses `redb` directly. A `trait ApiCache` with `get`/`put` would enable in-memory test doubles.

### 3.4 `http::client()` Reads Environment Variables and Filesystem

`crates/riptide-provider-tidal/src/http.rs`:9-32

`client()` reads `SSL_CERT_FILE` etc. and reads files. This is an I/O boundary that should ideally be called once at the CLI level and injected. Currently, `TidalClient::try_new` and `TidalAuthClient::try_new` each call it independently, potentially reading certs twice.

**Recommendation:** Accept a `reqwest::Client` as a parameter rather than constructing it internally:

```rust
pub fn try_new(
    api_base_url: Url,
    http: reqwest::Client,  // <-- injected
    auth: TidalAuthClient,
    token_store: TidalTokenStore,
    cache: TidalApiCache,
) -> Self { ... }
```

### 3.5 Good Iterator Idioms

The codebase generally uses idiomatic iterator chains. `convert.rs` uses `.into_iter().map().collect()` throughout. `client.rs`:162-216 maps search results cleanly. The `context.rs` artist extraction uses `.iter().filter().map().collect()`.

### 3.6 Duplicated Artist Name Extraction

`crates/riptide-template/src/context.rs`:423-439

```rust
fn artist_names(track: &Track, role: ArtistRole) -> Vec<String> { ... }
fn artist_names_video(video: &Video, role: ArtistRole) -> Vec<String> { ... }
```

These two functions are identical except for the type. Both `Track` and `Video` have `artists: Vec<ArtistCredit>`. Extract a shared function:

```rust
fn artist_names_by_role(credits: &[ArtistCredit], role: ArtistRole) -> Vec<String> {
    credits
        .iter()
        .filter(|credit| credit.role == role)
        .map(|credit| credit.artist.name.clone())
        .collect()
}
```

---

## 4. The Performance Inspector (Speed & Efficiency)

### 4.1 Redundant Token Store Loads

`crates/riptide-provider-tidal/src/client.rs`:273-301

`token_for_request()` and `country_params()` each independently call `self.token_store.load().await`, meaning every API call triggers **two** filesystem reads (or two `spawn_blocking` calls). The call sites in `track()`, `video()`, `album()`, etc. invoke `country_params()` first, then `get_json()` which calls `token_for_request()`.

**Recommendation:** Load the token once per request:

```rust
async fn get_json<T: DeserializeOwned>(
    &self,
    endpoint: &str,
    extra_params: &[(String, String)],
    policy: ApiCachePolicy,
) -> TidalResult<T> {
    let token = self.token_for_request(endpoint).await?;
    let mut params = vec![(
        "countryCode".to_owned(),
        token.country_code.as_str().to_owned(),
    )];
    params.extend_from_slice(extra_params);
    // ... rest of method using `token` and `params`
}
```

### 4.2 Cache Key Clones Parameters Vec Unnecessarily

`crates/riptide-provider-tidal/src/cache.rs`:33

```rust
let mut params = params.to_vec();  // clones all (String, String) pairs
params.sort();
```

For cache key construction, this allocates a new `Vec` and clones all strings. For hot paths, consider sorting references:

```rust
pub fn new(
    method: &str,
    endpoint: &str,
    country_code: Option<&str>,
    params: &[(String, String)],
) -> Self {
    let mut indices: Vec<usize> = (0..params.len()).collect();
    indices.sort_by(|&a, &b| params[a].cmp(&params[b]));
    let encoded = indices
        .into_iter()
        .map(|i| format!("{}={}", params[i].0, params[i].1))
        .collect::<Vec<_>>()
        .join("&");
    Self(format!(
        "{}:{}:{}:{}",
        method,
        endpoint,
        country_code.unwrap_or(""),
        encoded,
    ))
}
```

### 4.3 `TidalApiCache::get()` Uses Two Transactions for Expired Entries

`crates/riptide-provider-tidal/src/cache.rs`:108-140

On cache miss due to expiry, the code opens a read transaction, finds the expired entry, drops it, then opens a **write** transaction to delete. The entry is deserialized from JSON even though it will be discarded. Consider using a single write transaction for the eviction check, or making eviction lazy (skip the delete and let TTL handle it).

### 4.4 `Revalidate` and `Ttl` Cache Handling is Nearly Identical

`crates/riptide-provider-tidal/src/client.rs`:243-265

The `Revalidate` branch and `Ttl` branch both call `self.cache.put()` with nearly identical `CacheEntryToWrite` structs — the only difference is the hardcoded `3600` vs `duration.as_secs()`. Consider consolidating:

```rust
let cache_ttl = match policy {
    ApiCachePolicy::NoCache => None,
    ApiCachePolicy::Ttl(duration) => Some(duration),
    ApiCachePolicy::Revalidate => Some(DEFAULT_TTL),
};
if let Some(ttl) = cache_ttl {
    self.cache
        .put(CacheEntryToWrite {
            key,
            value: value.clone(),
            expires_at: time::OffsetDateTime::now_utc()
                + time::Duration::seconds(
                    i64::try_from(ttl.as_secs()).unwrap_or(3600),
                ),
        })
        .await?;
}
```

### 4.5 String Allocations in Conversion Functions

`crates/riptide-provider-tidal/src/convert.rs` — Every field name is allocated as a new `String` via `.to_owned()`:

```rust
extras.insert("version".to_owned(), ...);
extras.insert("copyright".to_owned(), ...);
```

Since these are JSON map insertions, this is unavoidable with `serde_json::Map<String, Value>`. Not a bug, but if performance becomes a concern, a pre-allocated set of interned keys could help.

### 4.6 No Parallelism for Batch Operations

The `search` function in `client.rs`:162-216 converts all results sequentially via `.into_iter().map(convert::track).collect()`. For a single search response this is fine, but when the app grows to support batch operations (album children, playlist items), consider `rayon` or `tokio::task::JoinSet` for parallel conversion of large result sets.

---

## 5. The Consistency Czar (Maintainability & Documentation)

### 5.1 Good: Doc Comments Are Comprehensive

Every `pub struct`, `pub fn`, `pub enum`, and `pub trait` in the library crates has `///` doc comments. The `//!` module-level docs are present in every file. This is well above average.

### 5.2 Good: CLI Help Interface is Well-Structured

The `clap` derive interface in `main.rs` provides `--help` text for every command and argument via `///` comments on struct fields. Subcommands are well-organized.

### 5.3 Missing Tests: Provider Integration

The `TidalProvider` implementation (`crates/riptide-provider-tidal/src/provider.rs`) has **zero tests**. The `Resolver`, `CollectionProvider`, `StreamProvider`, and `LyricsProvider` trait implementations are untested. Mocking `TidalClient` (see 3.2/3.3 above) would enable unit testing these.

### 5.4 Missing Tests: `convert.rs` Has No Unit Tests

`crates/riptide-provider-tidal/src/convert.rs` — This file contains all the TIDAL-to-core type mapping logic (339 lines) with zero tests. This is the most bug-prone code in the crate (edge cases around missing fields, date parsing, artist role mapping). Add tests with sample TIDAL JSON payloads.

### 5.5 Missing Tests: `resource.rs` Has No Unit Tests

`crates/riptide-provider-tidal/src/resource.rs` — URL parsing logic with no tests. Cover cases like:

- `https://listen.tidal.com/track/12345`
- `track/12345`
- `/track/12345/`
- `https://listen.tidal.com/unknown/12345`
- Empty string
- Non-TIDAL URLs

### 5.6 Missing Tests: `client.rs` and `auth.rs` Networking

The client and auth modules have no unit tests (only one `#[ignore]` live integration test). Mock-based tests would cover token refresh logic, rate limit retry, cache hit/miss paths, and error mapping.

### 5.7 `TidalProviderOptions` Fields Are Unused

`crates/riptide-provider-tidal/src/provider.rs`:15-25

```rust
pub struct TidalProviderOptions {
    pub auth_client: TidalAuthClient,   // unused
    pub api_client: TidalClient,        // used
    pub token_store: TidalTokenStore,   // unused
    pub cache: TidalApiCache,           // unused
}
```

Only `api_client` is used in `TidalProvider::new()`. The other three fields are accepted but discarded. Either remove them or store them for later use.

### 5.8 BUG: `collections: true` in Capabilities but Implementation Returns Error

`crates/riptide-provider-tidal/src/provider.rs`:130

```rust
fn capabilities(&self) -> ProviderCapabilities {
    ProviderCapabilities {
        collections: true,  // <--- claims support
        ...
    }
}
```

But `CollectionProvider::children()` at line 90 returns `UnsupportedCapability`. This is a semantic bug — the capability flag should be `false` until the implementation is complete.

---

## 7. Python Port Artifacts

Patterns in the Rust code that are mechanical translations of Python idioms rather than idiomatic Rust.

### 7.1 `ProviderExtras(serde_json::Value)` — JSON as Internal Data Passing

**Severity: Critical**

**Files:** `catalog.rs`:7-17, `convert.rs`:36-58/99-102/142-155, `context.rs`:441-455

**What:** The data flow is:

1. `convert.rs` builds a `serde_json::Map` by hand, inserting string keys like `"version"`, `"copyright"`, `"bpm"`, `"dolby"`, `"quality"`, `"hires_lossless"`, `"release"`
2. It wraps this in `ProviderExtras(serde_json::Value::Object(extras))` on core types (`Track`, `Video`, `Album`)
3. `context.rs` reads them back out with `extra_string(&track.extras.0, "version")`, `extra_bool(&album.extras.0, "hires_lossless")`, etc.

This is a Python dict pattern transplanted into Rust. Data flows through a typed boundary (`Track` struct) via an untyped escape hatch (`serde_json::Value`), defeating the type system. A typo in a string key silently produces a missing value.

**Fix:** Replace `ProviderExtras` with typed optional fields directly on the core structs. The known fields are small and stable:

- `Track`: `version: Option<String>`, `copyright: Option<String>`, `bpm: Option<i64>`, `quality_label: Option<String>`, `dolby_atmos: bool`
- `Album`: `release_type: Option<String>`, `hires_lossless: bool`
- `Video`: `quality_label: Option<String>`

If provider-neutral purity is important, an alternative is a generic: `Track<E = ()>` with `extras: E` where TIDAL provides `TidalTrackExtras`. But the simpler approach (typed fields on core types) is more practical since these fields are cross-provider concepts, not TIDAL-specific.

### 7.2 String-Typed Fields That Should Be Enums

**Severity: High**

**Files:** `model.rs`:25/145/236, `convert.rs`:285-289/315-337, `context.rs`:139/318

**Instances:**

- `TidalArtist.kind: Option<String>` — values are `"MAIN"`, `"FEATURED"`, etc. Converted to `ArtistRole` via string matching in `convert.rs`:285-289. Should deserialize directly into an enum with `#[serde(rename_all = "SCREAMING_SNAKE_CASE")]` and a fallback variant.
- `TidalAlbum.release_type: Option<String>` — values are `"ALBUM"`, `"SINGLE"`, `"EP"`, etc. Should be an enum.
- `TopHit.kind: String` — values are `"TRACKS"`, `"ALBUMS"`, `"PLAYLISTS"`, `"ARTISTS"`. Used as a type discriminator in `convert.rs`:315-337. Should be an enum.
- `ItemTemplateInput.quality: String` and `AlbumTemplateInput.release: String` — carry provider quality/release labels as bare strings into the template layer.

**Fix:** Define enums for each. For the TIDAL model types, use `#[serde(rename_all = ...)]` with an `Unknown` fallback variant to handle new values from the API gracefully. For template inputs, keep as `String` (these are display values) but source them from typed fields rather than JSON extraction.

### 7.3 Hardcoded Magic Strings for Domain Logic

**Severity: Moderate**

**Files:** `convert.rs`:56/153, `context.rs`:368

**Instances:**

- `convert.rs`:56 — `tag == "DOLBY_ATMOS"` checked against `Vec<String>` of media metadata tags
- `convert.rs`:153 — `tag == "HIRES_LOSSLESS"` same pattern
- `context.rs`:368 — `label.as_str() == "MAX"` to decide album master flag

**Fix:** Define a `MediaTag` enum or at minimum named constants. The media metadata tags from TIDAL should be deserialized into a `HashSet<MediaTag>` (or `BTreeSet`) with an `Unknown(String)` variant for unrecognized tags.

---

## 8. CLI Command Structure Review

The CLI is **not a mechanical Python port** — it uses clap derive macros idiomatically, exhaustive match dispatch, and proper `anyhow` error handling. However, a few patterns can be improved.

### 8.1 `Option<Command>` Silently Succeeds with No Args

`crates/riptide-cli/src/main.rs`:41, 155

```rust
command: Option<Command>,  // line 41
// ...
None => Ok(()),  // line 155
```

Running `riptide-rs` with no arguments produces zero output and exits successfully. The Python app uses `no_args_is_help=True`, which prints help. Silent success on no input is confusing UX.

**Fix:** Make `command` required (not `Option`). Clap will automatically print help when a required subcommand is missing. This is the standard Rust CLI pattern.

### 8.2 Debug URL Flags Are Visible Noise

`crates/riptide-cli/src/main.rs`:33-38

`--auth-base-url` and `--api-base-url` are developer/debug overrides that 99% of users will never use. They appear in `--help` output, adding noise to the interface.

**Fix:** Hide them with `#[arg(long, global = true, hide = true)]`. They remain usable but don't clutter help output.

### 8.3 Auth Flow Matches on Error Message Substrings

`crates/riptide-cli/src/main.rs`:245-250

```rust
Err(TidalError::Api { message, .. }) if message.contains("authorization_pending") => {
    continue;
}
Err(TidalError::Api { message, .. }) if message.contains("slow_down") => {
    interval = interval.slow_down();
}
```

This string-matches on the full API response body to determine control flow. If TIDAL changes their error format, this breaks silently. The TIDAL API returns structured JSON error responses with an `error` field — these should be parsed into typed variants (e.g., `TidalError::AuthorizationPending`, `TidalError::SlowDown`) at the HTTP layer, not matched as substrings in the CLI.

**Fix:** Parse the TIDAL error response JSON into structured variants in `auth.rs` or `client.rs`. The CLI match becomes:

```rust
Err(TidalError::AuthorizationPending) => continue,
Err(TidalError::SlowDown) => interval = interval.slow_down(),
```

### 8.4 Single `main.rs` Monolith — Defer Split

At 474 lines, `main.rs` is at the boundary of acceptability. Not a port artifact — early-stage pragmatism. Worth splitting into `commands/auth.rs`, `commands/list.rs`, etc. when download/listen commands arrive and the file grows past ~600 lines. **Deferred.**

---

## Summary of Priority Items

### Bugs (fix now)

1. `TidalClient::new()` / `TidalAuthClient::new()` silently swallow cert errors (2.1)
2. Missing refresh token produces poor error (2.2)
3. `collections: true` but implementation returns `UnsupportedCapability` (5.8)

### High-value refactors

4. Deduplicate `ProviderId::new("tidal")` with a lazy static (2.4)
5. Eliminate double token load per request (4.1)
6. Remove `.clone()` on `cli.command` (2.6)
7. Make `convert` and `model` `pub(crate)` (1.3)

### Testing gaps

8. `convert.rs` — zero unit tests for 339 lines of mapping code (5.4)
9. `resource.rs` — zero unit tests for URL parsing (5.5)
10. `provider.rs` — zero tests for trait implementations (5.3)

### Architectural recommendations (next iteration)

11. Extract `TokenStore` and `ApiCache` traits for testability (3.2, 3.3)
12. Inject `reqwest::Client` instead of constructing internally (3.4)
13. Remove dead `assert_path_is_local` (2.7)

---

## 6. The CLI Ergonomics Expert (Configuration UX)

### What's Good

- **`deny_unknown_fields` + `inspect_unknown_keys()`** — excellent. Typos in config keys produce clear errors with path context (`download.threads_count` is unknown). This is better UX than most Rust CLI tools.
- **Secret redaction** — `ListenerSecret` Debug prints `***`. Good practice.
- **Sensible defaults** — Empty config file produces a working configuration. Zero-config startup is correct.
- **Validation error paths** — Errors like `"invalid config value at download.track_quality"` tell the user exactly where to look.

### 6.1 The Config Is Too Thin — Missing Sections

**Current state:** 2 sections, 5 keys total (`download.{track_quality, video_quality, skip_existing}`, `listener.{port, secret}`).

**What a user will expect to configure:**

| Section | Keys to add | Priority |
|---|---|---|
| `[download]` | `threads_count`, `download_path`, `scan_path`, `singles_filter`, `videos_filter` | High — core download behavior |
| `[download.templates]` | `default`, `track`, `video`, `album`, `playlist`, `mix` | High — path templating is already built in `riptide-template` |
| `[metadata]` | `enable`, `lyrics`, `cover` | Medium — needed once download pipeline lands |
| `[list]` | `format` | Low — convenience |
| `[m3u]` | `save`, `allowed`, templates | Low — nice-to-have |

**Action:** Add `[download.templates]` and `download_path` now. The template engine is ready. The rest can come with the download pipeline.

### 6.2 Rename `track_quality` → `audio_quality`

**Problem:** The field is called `track_quality` but the type is `AudioQualityRequest`. A track is a resource kind; audio is the media type. Videos also have tracks. The naming leaks an implementation concept into user-facing config.

**Action:** Rename `track_quality` → `audio_quality` in both the TOML key and the Rust struct. No aliases needed — this is a new app.

### 6.3 `video_quality` Vocabulary — Clean It Up

The current accepted values are `low`, `medium`, `high`, `best`. This is clean and consistent. No legacy vocabulary (`fhd`, `sd`, `hd`) should be added.

**Action:** No change needed. The current vocabulary is correct for a new app. Just ensure the error message from `QualityParseError` lists the accepted values clearly (it already does).

### 6.4 `listener.secret = ""` Means "No Secret" — Confusing

**Problem:** An empty string is semantically "no auth", but a user writing `secret = ""` might think they're explicitly setting it to empty (which is different from omitting it). The `ListenerSecret::optional()` coerces `""` → `None` silently.

**Action:** Reject `secret = ""` as an error. If the user omits `secret`, it's `None` (no auth). If they write it, it must be non-empty. The "golden path" example config should use a comment:

```toml
[listener]
port = 8123
# secret = "my-secret"  # uncomment to enable authentication
```

### 6.5 No `[download.templates]` Section Yet

**Problem:** The template engine (`riptide-template`) is fully built and working. The CLI has `--template` flags on `list` subcommands. But there's no way to set default templates in the config file. Users will want:

```toml
[download.templates]
default = "{album.artists}/{album.date:%Y} - {album.title}/{item.number:02d} {item.title}"
```

**Action:** Add a `[download.templates]` section to `RawConfig`/`ValidatedConfig` with a `default` key and per-resource-type overrides. The template crate already has `CompiledTemplate::parse()` ready for validation.

### 6.6 No `download_path` or `scan_path`

**Problem:** There's no way to configure where files go.

**Action:** Add when the download pipeline lands. Use `directories` crate (already a dependency) for platform-appropriate defaults.

### 6.7 CLI Global Flags Should Be Config-File-Overridable

**Problem:** `--auth-file`, `--cache-file`, `--auth-base-url`, `--api-base-url` are CLI-only. Users will want to set these in the config file once rather than passing them every invocation.

**Action:** Add optional fields to the config:

```toml
[tidal]
auth_file = "/path/to/token.json"
cache_file = "/path/to/cache.redb"
# auth_base_url = "https://auth.tidal.com/v1/oauth2/"
# api_base_url = "https://api.tidal.com/v1/"
```

CLI flags override config file values. This is a standard layered-config pattern.

### 6.8 Config File Location Isn't Discoverable

**Problem:** There's no `riptide-rs config init` or `riptide-rs config path` command. The user has no way to know where the config file should live. `project_dirs()` returns platform-specific paths that differ between macOS/Linux/Windows.

**Action:** Add `config path` subcommand that prints the expected config file location. Consider `config init` that writes a commented example.

### Config UX Priority Summary

| # | Change | Effort | Impact |
|---|---|---|---|
| 1 | Add `[download.templates]` section | Medium | High — template engine is ready, config is the missing link |
| 2 | Rename `track_quality` → `audio_quality` | Low | Medium — naming clarity |
| 3 | Add `config path` subcommand | Low | High — discoverability |
| 4 | Make CLI global flags config-overridable (`[tidal]` section) | Medium | High — daily usability |
| 5 | Add `download_path` when download pipeline lands | Medium | High — blocked on pipeline |
| 6 | Reject `secret = ""` instead of coercing to `None` | Low | Low — cleaner semantics |
| 7 | Add `[metadata]`, `[m3u]`, `[list]` sections | Medium | Low — blocked on features |

---

## Action Plan — Triage Decisions

All actions reviewed and triaged on 2026-05-08.

| # | Action | Audit Ref | Decision | Effort |
|---|---|---|---|---|
| 1 | Remove `TidalClient::new()` / `TidalAuthClient::new()` infallible constructors | 2.1 | **Now** | ~10 lines deleted |
| 2 | Return specific error for missing refresh tokens | 2.2 | **Now** | ~3 lines |
| 3 | Preserve error context when reading HTTP error bodies | 2.3 | **Now** | 2 lines |
| 4 | Fix `collections: true` → `false` in `TidalProvider` capabilities | 5.8 | **Now** | 1 line |
| 5 | Deduplicate `ProviderId::new("tidal")` with `LazyLock` | 2.4 | **Now** | ~15 lines across 4-5 files |
| 6 | Eliminate double token store load per API request | 4.1 | **Now** | ~30 lines refactored |
| 7 | Remove `.clone()` on `cli.command` in `main()` | 2.6 | **Now** | ~5 lines |
| 8 | Make `convert` and `model` modules `pub(crate)` | 1.3 | **Now** | 2 lines + re-export cleanup |
| 9 | Remove dead `assert_path_is_local` function | 2.7 | **Now** | 4 lines deleted |
| 10 | Remove unused fields from `TidalProviderOptions` | 5.7 | **Now** | ~10 lines |
| 11 | Deduplicate artist name extraction functions | 3.6 | **Now** | ~10 lines |
| 12 | Consolidate `Revalidate`/`Ttl` cache write logic | 4.4 | **Now** | ~10 lines |
| 13 | Rename `track_quality` → `audio_quality` in config | 6.2 | **Now** | ~15 lines across 4 files |
| 14 | Remove duplicate `tempfile` dev-dependency | 1.4 | **Now** | 1 line deleted |
| 15 | Add `clippy::pedantic` and `clippy::nursery` workspace lints | 1.2 | **Now** | Config + lint fixes |
| 16 | Add unit tests for `convert.rs` | 5.4 | **Now** | ~100-150 lines |
| 17 | Add unit tests for `resource.rs` | 5.5 | **Now** | ~50-70 lines |
| 18 | Switch `ProviderCapabilities` to `bitflags` | 2.5 | **Now** | ~40 lines across 3 files |
| 19 | Extract `TokenStore` and `ApiCache` traits | 3.2, 3.3 | **Now** | ~150-200 lines across 5+ files |
| 20 | Inject `reqwest::Client` instead of constructing internally | 3.4 | **Now** | ~20 lines (pairs with #19) |
| 21 | Replace `ProviderExtras(serde_json::Value)` with typed fields | 7.1 | **Now** | ~100-150 lines across core, tidal, template |
| 22 | Convert string-typed model fields to enums | 7.2 | **Now** | ~60-80 lines across model.rs, convert.rs |
| 23 | Replace magic string comparisons with constants/enums | 7.3 | **Now** | ~20-30 lines across convert.rs, context.rs |
| 24 | Make `Command` required (auto-help on no args) | 8.1 | **Now** | 1 line |
| 25 | Hide debug URL flags from `--help` | 8.2 | **Now** | 2 lines |
| 26 | Parse TIDAL error codes into structured enum variants | 8.3 | **Now** | ~30 lines across auth.rs, main.rs |

### Suggested execution order

**Phase 1 — Quick wins (actions 1-4, 9, 14, 24, 25):** Bug fixes, dead code removal, and trivial CLI improvements. Minimal risk, immediate value.

**Phase 2 — Focused refactors (actions 5, 7, 8, 10, 11, 12, 13, 26):** Internal cleanup that simplifies the codebase. Each is independent. Action 26 (structured TIDAL error codes) fits here — it's a focused refactor to the error parsing layer.

**Phase 3 — Structural changes (actions 6, 19, 20):** Token load dedup and trait extraction are coupled — do them together. Inject `reqwest::Client` at the same time.

**Phase 4 — De-Pythonification (actions 21, 22, 23):** Replace `ProviderExtras` JSON escape hatch with typed fields, convert string-typed model fields to enums, replace magic strings. Action 21 is the largest — do it first, then 22 and 23 follow naturally since they touch overlapping code in `convert.rs` and `model.rs`.

**Phase 5 — Lints and tests (actions 15, 16, 17):** Enable pedantic lints and fix warnings. Add test coverage for `convert.rs` and `resource.rs`. Best done after phases 3-4 since those phases change the code being tested.

**Phase 6 — API design (action 18):** Switch `ProviderCapabilities` to bitflags. Can be done independently but affects the provider trait contract.
