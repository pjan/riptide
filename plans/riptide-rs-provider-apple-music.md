# riptide-rs: Apple Music Provider

## Status

Draft for review.

## Goal

Add Apple Music as a second provider alongside TIDAL. Support catalog
browsing (resolve, list, search) and audio downloads via WebPlayback API +
Widevine CDM decryption.

## Context

### Reference implementation

[Antra `apple.py`](https://github.com/anandprtp/Antra/blob/main/antra/sources/apple.py)
— Python adapter that implements both direct WebPlayback and mirror/proxy
modes. The direct path uses `pywidevine` for Widevine CDM.

### Key constraints

- **Audio quality:** WebPlayback API serves **AAC only** (~256 kbps lossy) for
  all flavor requests. True ALAC lossless requires `extendedAssetUrls.enhancedHls`
  with FairPlay DRM (`skd://` key delivery), which is Apple-hardware-only and
  out of scope.
- **Authentication:** Not an OAuth device flow like TIDAL. Requires a Bearer JWT
  (`authorization_token`) and `music-user-token` extracted manually from a
  browser session on `music.apple.com`.
- **DRM:** All WebPlayback streams are Widevine CENC-encrypted. Even lossy AAC
  requires CDM challenge/license/decrypt.
- **Widevine CDM crate:** [`widevine` (ThetaDev)](https://codeberg.org/ThetaDev/widevine-rs)
  — 2k+ downloads, clean API, `thiserror`-based errors. **GPL-3.0 licensed.**
- **GPL impact:** The `widevine` crate is GPL-3.0. Linking it statically makes
  the binary GPL. Must be feature-gated so default builds remain Apache-2.0.

### Apple Music API endpoints

| Endpoint | Purpose |
|---|---|
| `amp-api.music.apple.com/v1/catalog/{storefront}/songs/{id}` | Song metadata |
| `amp-api.music.apple.com/v1/catalog/{storefront}/songs?filter[isrc]={isrc}` | ISRC lookup |
| `amp-api.music.apple.com/v1/catalog/{storefront}/albums/{id}` | Album metadata |
| `amp-api.music.apple.com/v1/catalog/{storefront}/albums/{id}/tracks` | Album tracks |
| `amp-api.music.apple.com/v1/catalog/{storefront}/playlists/{id}` | Playlist metadata |
| `amp-api.music.apple.com/v1/catalog/{storefront}/playlists/{id}/tracks` | Playlist tracks |
| `amp-api.music.apple.com/v1/catalog/{storefront}/search?term={q}&types=songs,albums` | Search |
| `itunes.apple.com/search?term={q}&entity=song&media=music` | iTunes text search |
| `play.itunes.apple.com/WebObjects/MZPlay.woa/wa/webPlayback` | Stream manifest |
| `play.itunes.apple.com/WebObjects/MZPlay.woa/wa/acquireWebPlaybackLicense` | Widevine license |

### Stream acquisition pipeline

```
1. Catalog lookup (metadata + audioTraits for quality reporting)
2. WebPlayback POST {salableAdamId} → songList[0] → assets
3. Select best asset (flavor priority: 28:ctrp256 > 30:cbcp256 > others)
4. Fetch HLS master playlist from asset URL
5. Resolve lossless variant (follow nested master playlists, prefer ALAC codec)
   Note: ALAC variants exist but are FairPlay-locked; falls back to AAC
6. Extract Widevine PSSH URIs from EXT-X-KEY headers
7. CDM: open session → generate challenge from PSSH
8. POST challenge to Apple license server → receive license
9. Parse license → extract content keys (KID → key mapping)
10. Download encrypted HLS segments
11. Decrypt CENC (AES-CTR) with content key
12. Patch MP4 codec label: "enca" → "mp4a"
```

## Architecture

### New crate: `riptide-provider-apple`

Parallel structure to `riptide-provider-tidal`.

```
crates/riptide-provider-apple/
├── Cargo.toml
└── src/
    ├── lib.rs              — module exports, APPLE_PROVIDER_ID, http_client()
    ├── error.rs            — AppleError enum (thiserror)
    ├── provider.rs         — AppleProvider implementing Provider trait
    ├── client.rs           — AppleMusicClient (catalog API)
    ├── auth.rs             — credential types, token management
    ├── resource.rs         — URL/ID parsing (music.apple.com URLs)
    ├── model.rs            — Apple Music API response models (serde)
    ├── convert.rs          — Apple models → riptide-core types
    ├── stream.rs           — WebPlayback + HLS variant selection
    ├── drm.rs              — Widevine CDM integration (feature-gated)
    └── cover.rs            — artwork URL building
```

### Feature flags

```toml
[features]
default = []
widevine = ["dep:widevine"]

[dependencies]
widevine = { version = "0.1", optional = true }
```

- **Without `widevine`:** Catalog-only mode. `StreamProvider::streams()` returns
  an error explaining that the `widevine` feature is required.
- **With `widevine`:** Full stream acquisition + decryption.

### License strategy

- `riptide-provider-apple` crate itself: **Apache-2.0** (no GPL deps in default)
- With `widevine` feature: the compiled binary becomes **GPL-3.0** due to
  static linking. This is opt-in.
- `riptide-cli` Cargo.toml:
  ```toml
  riptide-provider-apple = { path = "../riptide-provider-apple" }
  # User opts in:
  # riptide-provider-apple = { path = "../riptide-provider-apple", features = ["widevine"] }
  ```

### Changes to existing crates

#### `riptide-core`

- Add `ContainerExtension::M4a` already exists — no change needed.
- Potentially add a new `PostProcessStep::DecryptCenc` variant (or handle
  decryption as part of the provider's stream pipeline, before returning the
  manifest). **Recommendation:** handle decryption inside the provider, not as a
  generic post-process step. CENC decryption is provider-specific (requires the
  content key from CDM), unlike ffmpeg-based post-processing which is generic.

#### `riptide-media`

- **HLS variant selection:** Current `select_hls_variant` in `manifest.rs`
  picks by bandwidth only. Apple Music needs codec-aware selection (prefer ALAC
  over AAC). Options:
  - (a) Add a `VariantPreference` parameter to the existing function
  - (b) Let the provider handle variant selection itself and pass a resolved
    media playlist URL as the manifest
  - **Recommendation:** Option (b). The provider already fetches and walks the
    master playlist to extract PSSH headers. It should resolve down to the
    media playlist before returning a `Manifest::HlsMediaPlaylist` or
    `Manifest::DirectSegments` to `riptide-media`. This keeps `riptide-media`
    simple and provider-neutral.

#### `riptide-cli`

- Register `AppleProvider` alongside `TidalProvider` in the provider registry.
- Add `--provider` flag or auto-detect from URL domain.
- Add `auth login apple` / `auth status apple` subcommands.
- Token storage: separate file (`apple-token.json`) via same `FileTokenStore`
  pattern, but storing JWT + music-user-token instead of OAuth tokens.

### Authentication UX

Apple Music does not support OAuth device flow. The user must:

1. Open `music.apple.com` in a browser, sign in
2. Open browser dev tools → Network tab
3. Find a request to `amp-api.music.apple.com`
4. Copy the `Authorization` header value (Bearer JWT)
5. Copy the `Media-User-Token` header value
6. Run `riptide auth login apple --token <JWT> --music-user-token <MUT>`
   or set env vars `RIPTIDE_APPLE_AUTH_TOKEN` and `RIPTIDE_APPLE_MUSIC_USER_TOKEN`

Tokens expire periodically and must be re-extracted. The CLI should detect
401 errors and provide clear instructions.

Optional: `--storefront` flag (defaults to `us`). Determines catalog region.

### CENC decryption strategy

Two approaches, tried in order (matching Antra):

1. **ffmpeg native:** `ffmpeg -decryption_key {hex} -i {hls_url} -c copy out.m4a`
   — works when ffmpeg is built with CENC support.
2. **Pure Rust fallback:** Download encrypted segments, parse fragmented MP4
   boxes (moof/mdat/senc/trun), decrypt AES-CTR with subsample patterns,
   patch `enca` → `mp4a`. ~200 lines of MP4 box parsing.

The Rust fallback should live in `riptide-provider-apple::drm` since it's
Apple-specific (the CENC format and box structure are tied to Apple's HLS
output).

### Rate limiting

- 2.5-second spacing between Apple Music API requests
- 429 retry delays: 3.0s, 6.0s, 10.0s
- Implemented as a per-client rate limiter (similar to TIDAL's backoff)

## Implementation Phases

### Phase A — Catalog-only provider (no `widevine` feature)

**Goal:** Browse Apple Music catalog via riptide CLI.

1. **Create `riptide-provider-apple` crate** with workspace membership
2. **`resource.rs`** — Parse `music.apple.com` URLs:
   - `music.apple.com/{storefront}/song/{slug}/{id}`
   - `music.apple.com/{storefront}/album/{slug}/{id}`
   - `music.apple.com/{storefront}/playlist/{slug}/{id}`
   - Plain numeric IDs
3. **`model.rs`** — Serde models for Apple Music API responses:
   - Song, Album, Playlist, Artist attributes
   - Search response with `results.songs`, `results.albums`
   - Paginated `data[]` / `next` cursor
4. **`convert.rs`** — Apple models → `CatalogEntity` (Track, Album, Playlist)
   - Map `audioTraits` → quality metadata (reported but not downloadable
     without Widevine)
5. **`auth.rs`** — `AppleCredentials` (JWT + music-user-token + storefront),
   `AppleTokenStore` (file-backed)
6. **`client.rs`** — `AppleMusicClient`:
   - `song(id)`, `album(id)`, `playlist(id)`
   - `album_tracks(id)`, `playlist_tracks(id)` with pagination
   - `search(query)` (catalog API + iTunes fallback)
   - `lookup_by_isrc(isrc)`
   - Rate limiting (2.5s spacing)
7. **`provider.rs`** — Implement `Resolver`, `CollectionProvider`, `LyricsProvider`
   - `StreamProvider::streams()` returns error without `widevine` feature
8. **`error.rs`** — `AppleError` enum with `into_core()` bridge
9. **CLI integration:**
   - Register `AppleProvider` in provider registry
   - `auth login apple`, `auth status apple`
   - `list track/album/playlist` works with Apple Music URLs
   - `download` produces a clear error about needing `--features widevine`

### Phase B — Stream acquisition (with `widevine` feature)

**Goal:** Download AAC audio from Apple Music.

1. **`stream.rs`** — WebPlayback integration:
   - `get_webplayback(track_id)` → asset selection (flavor priority)
   - `resolve_variant(asset_url)` → walk nested HLS masters, prefer ALAC codec
     (even though it'll be AAC in practice due to WebPlayback limitations)
   - Extract `EXT-X-KEY` PSSH URIs from variant playlist
   - Return `Manifest::HlsMediaPlaylist` with segment URLs
2. **`drm.rs`** (feature-gated behind `widevine`):
   - `get_content_keys(pssh_b64, track_id)` → KID→key map
   - Uses `widevine` crate: `Device::read_wvd()` → `Cdm::new()` →
     `open()` → `get_license_request()` → `challenge()` → POST to Apple
     license server → `get_keys()`
   - CENC decryption: ffmpeg-first with pure-Rust fallback
   - MP4 codec patching (`enca` → `mp4a`)
3. **`provider.rs`** — Wire `StreamProvider::streams()` to return decrypted
   stream variant
4. **New `PostProcessStep` (if needed):** Evaluate whether decryption should
   happen inside the provider (before returning to riptide-media) or as a
   post-processing step. Recommendation: inside the provider, since the
   content key is provider-local state.
5. **CLI integration:**
   - `download` works end-to-end with Apple Music URLs
   - WVD device file path via `--wvd-path` flag or `RIPTIDE_WVD_PATH` env var
   - Progress events for decrypt phase

### Phase C — Polish

1. Cover art URL building (`is{N}-ssl.mzstatic.com` pattern)
2. Lyrics (Apple Music has a lyrics endpoint — investigate availability)
3. ISRC cross-lookup (resolve TIDAL ISRC → Apple Music ID)
4. Config file support (`[apple]` section with storefront, credentials path)
5. Token expiry detection with actionable error messages
6. Tests: unit tests for model conversion, URL parsing, mock catalog responses

## Risks

| Risk | Mitigation |
|---|---|
| Apple changes WebPlayback API | Monitor Antra/similar projects for API changes; the catalog API is stable |
| Widevine CDM crate breaks or is abandoned | It's 0.1.0 with modest adoption; vendor the dependency or fork if needed |
| GPL license concern | Feature-gated; clearly documented; default builds remain Apache-2.0 |
| Token expiry UX is poor | Clear error messages; document re-extraction steps; explore cookie-based refresh |
| AAC-only limitation disappoints users | Document clearly that lossless requires FairPlay; report catalog quality traits accurately so users know what's available vs. what's downloadable |
| Rate limiting more aggressive than TIDAL | Conservative 2.5s default spacing; configurable |

## Dependencies (new)

| Crate | Version | Purpose | License | Conditional |
|---|---|---|---|---|
| `widevine` | 0.1 | Widevine CDM | GPL-3.0 | `features = ["widevine"]` |

All other dependencies (`reqwest`, `serde`, `serde_json`, `base64`, `url`,
`thiserror`, `tokio`, `time`) are already in the workspace.

## Commit Strategy

One commit per logical unit within each phase. Phase A is independently
shippable. Phase B depends on Phase A.
