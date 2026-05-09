# riptide-rs Slice 6: Download Parity

## Status

Draft for review.

## Goal

Close the audio download feature gap: artist, mix, and collection downloads;
singles filtering; template prevalidation; and CLI flag parity.

Scan-path is explicitly out of scope — `--skip-existing` is the only
existing-file mechanism in this slice.

## Scope

| Feature | Status before | Delivered |
| --- | --- | --- |
| Download track | ✓ | — |
| Download album | ✓ | — |
| Download playlist | ✓ | — |
| Download artist | bail | Artist → albums → items orchestration |
| Download mix | bail | Mix → items expansion |
| Singles filtering | config exists, unwired | `--singles` flag + filter applied in artist downloads |
| Template prevalidation | no | Validate template before first network call |
| `--skip-errors` flag | no | Continue on item/album failure, report in summary |
| Video download surface | present from earlier slices | Removed from download command/runtime in this audio-only slice |

## Non-Goals

- No scan-path or existing-file deduplication beyond `--skip-existing`.
- No artist discography in riptide-progress group-level rendering yet; artist
  downloads use the existing flat item UI. Group rendering is a separate slice.
- No persistent download history or resume across runs.
- No provider registry changes.
- Slice 6 is audio-only. Non-audio media expansion and filtering are out of
  scope.
- Video download is intentionally removed from the download path in this slice:
  no `--video-quality` download flag, no `VideosMode` filtering, no video
  stream resolution, and no direct video URL downloads.

## Data Model Changes

No new types in `riptide-core`. The existing `ResourceKind::Artist`,
`ResourceKind::Mix`, `CatalogEntity::Artist`, and `CatalogEntity::Collection`
already cover the needed entity kinds.

### `riptide-provider-tidal`

#### New API methods on `TidalClient`

```rust
/// Fetches TIDAL artist metadata.
pub async fn artist(&self, id: &str) -> TidalResult<Artist>;

/// Paginates `artists/{id}/albums?filter=ALBUMS`.
pub async fn artist_albums(&self, id: &str) -> TidalResult<Vec<Album>>;

/// Paginates `artists/{id}/albums?filter=EPSANDSINGLES`.
pub async fn artist_singles(&self, id: &str) -> TidalResult<Vec<Album>>;

/// Returns all artist albums from both album endpoints.
pub(crate) async fn artist_albums_all(&self, id: &str) -> TidalResult<Vec<Album>>;

/// Fetches only the artist album groups needed for the requested download mode.
pub(crate) async fn artist_albums_for_selection(
    &self,
    id: &str,
    selection: ArtistAlbumSelection,
) -> TidalResult<Vec<Album>>;

/// Paginates `mixes/{id}/items`.
pub async fn mix_items(&self, id: &str) -> TidalResult<Vec<CatalogEntity>>;
```

The artist album methods return strongly typed vectors (not `CatalogEntity`)
because the TIDAL API returns a homogeneous album set for each endpoint.
`mix_items` returns
`Vec<CatalogEntity>` because mix items can contain multiple entity kinds,
matching the existing `paged_items` pattern.
`artist_albums_all` is an explicit helper on `TidalClient`, used by
`TidalProvider::children`. It is listed here so the provider code does not rely
on an unstated method.

```rust
/// TIDAL album endpoint selection, independent from riptide-config.
pub enum ArtistAlbumSelection {
    AlbumsOnly,
    SinglesAndEpsOnly,
    AlbumsAndSingles,
}
```

#### Pagination helper for albums

`paged_items` currently calls `convert_catalog_item` on each raw JSON value.
`artist_albums` and `artist_singles` share a private
`paged_albums(endpoint, filter)` helper that deserialises each item as
`model::TidalAlbum` and calls `convert::album`. The pagination loop structure
mirrors `paged_items` — same offset/limit pattern, same termination condition —
but with a typed deserialisation target instead of `serde_json::Value`.

#### Filtered pagination

`paged_albums` takes an additional `filter` parameter forwarded as a query
param:

```text
GET artists/{id}/albums?filter=ALBUMS&limit=20&offset=0
GET artists/{id}/albums?filter=EPSANDSINGLES&limit=20&offset=0
```

Limit `20` is an intentional Rust-side choice for consistency with current
album, playlist, and mix item paging. This differs from Python's artist album
default of `10`, but does not affect feature behavior.

Python currently requests only `filter=ALBUMS` for artist downloads and then
uses a track-count heuristic for single handling. Rust intentionally fetches
both `ALBUMS` and `EPSANDSINGLES` and classifies each album from
`release_type`; this is a deliberate correction, not a byte-for-byte port.

`artist_albums_for_selection` avoids needless endpoint calls:

- `SinglesMode::Exclude` maps to `ArtistAlbumSelection::AlbumsOnly`.
- `SinglesMode::Only` maps to `ArtistAlbumSelection::SinglesAndEpsOnly`.
- `SinglesMode::Include` maps to `ArtistAlbumSelection::AlbumsAndSingles`.

When combining album endpoint results, preserve endpoint order and deduplicate
by `Album.id` with a `HashSet<MediaRef>`. This prevents duplicate album item
fetches and duplicate downloads if TIDAL returns overlapping results.

#### `TidalProvider` changes

Extend `resolve` to handle `ResourceKind::Artist` and `ResourceKind::Mix`:

```rust
ResourceKind::Artist => {
    let artist = self.client.artist(id).await?;
    Ok(CatalogEntity::Artist(artist))
}
ResourceKind::Mix => {
    // Mixes don't have standalone metadata on TIDAL. Return a
    // Collection entity with the mix ID so expand_download_entities
    // can call mix_items.
    Ok(CatalogEntity::Collection(Collection {
        id: reference.clone(),
        kind: CollectionKind::Mix,
        title: None,
        owner: None,
    }))
}
```

Extend `children` to handle `ResourceKind::Artist` and `ResourceKind::Mix`:

```rust
ResourceKind::Artist => {
    let albums = self.client.artist_albums_all(id).await?;
    Ok(albums.into_iter().map(CatalogEntity::Album).collect())
}
ResourceKind::Mix => self.client.mix_items(id).await,
```

`children` is on the `CollectionProvider` trait which takes only `&MediaRef`.
It has no way to receive `singles_mode`.

**Decision:** Keep the provider-neutral trait simple, but avoid needless calls
in the TIDAL download orchestration. For generic `children()` calls, artists
return all albums (both main and singles/EPs) as `CatalogEntity::Album` items.
For the download command, the CLI uses a TIDAL-specific inherent method to fetch
only the album endpoint groups required by the active `SinglesMode`, then still
applies the pure `SinglesMode` filter as a defensive check.

Add a TIDAL-specific inherent method for CLI artist expansion:

```rust
impl TidalProvider {
    pub async fn artist_albums_for_selection(
        &self,
        id: &str,
        selection: ArtistAlbumSelection,
    ) -> TidalResult<Vec<Album>>;
}
```

This keeps the provider-neutral `CollectionProvider` trait unchanged while
allowing the TIDAL download orchestration to avoid fetching endpoint groups the
active `SinglesMode` will discard.

#### Artist children implementation

`children()` for `ResourceKind::Artist` returns albums — not flattened
items. This preserves album context so the CLI can apply
`SinglesMode` filtering per album before fetching each album's items.

```rust
/// Returns all albums (main + singles/EPs) for an artist.
async fn artist_albums_all(&self, id: &str) -> TidalResult<Vec<Album>> {
    let mut albums = self.artist_albums(id).await?;
    let singles = self.artist_singles(id).await?;
    Ok(deduplicate_albums_by_id(albums.into_iter().chain(singles)))
}

fn deduplicate_albums_by_id(albums: impl IntoIterator<Item = Album>) -> Vec<Album> {
    let mut seen = HashSet::new();
    let mut deduped = Vec::new();
    for album in albums {
        if seen.insert(album.id.clone()) {
            deduped.push(album);
        }
    }
    deduped
}
```

`children()` wraps each album in `CatalogEntity::Album`:

```rust
ResourceKind::Artist => {
    let albums = self.client.artist_albums_all(id).await?;
    Ok(albums.into_iter().map(CatalogEntity::Album).collect())
}
```

The CLI is responsible for iterating albums, filtering by `SinglesMode`, and
calling `children()` per album to get tracks. Any non-track items returned by
existing album, playlist, or mix item endpoints are ignored in this audio-only
slice. No group events are emitted in this slice. The progress UI shows
individual tracks under a single operation row, like playlist downloads. When
the top-level entity is `CollectionKind::Mix`, the progress operation kind is
`OperationKind::Mix`, not the generic collection label.

### `riptide-config`

`SinglesMode` is already defined and parsed.

Audio-only cleanup:

- Remove `video_quality` and `videos_mode` from `DownloadConfig` and
  `RawDownloadConfig`.
- Remove `VideosMode` from the public `riptide-config` re-export surface if it
  has no remaining use.
- Remove `download.templates.video` from the accepted download template keys.
  Unknown-key handling should report it like any other unsupported key.
- Update generated/default config examples so they no longer mention video
  quality, video mode, or video templates.

This is a new pre-release app, so no migration compatibility is required for
these removed keys.

### `riptide-cli`

#### New types

```rust
/// Album type classification for singles filtering.
///
/// Derived from the TIDAL `release_type` field on `Album`. Albums without a
/// release type are treated as main albums.
enum AlbumType {
    Album,
    Single,
    Ep,
    Unknown,
}

impl AlbumType {
    fn from_release_type(release_type: Option<&str>) -> Self {
        match release_type {
            Some("ALBUM") | Some("COMPILATION") => Self::Album,
            Some("SINGLE") => Self::Single,
            Some("EP") => Self::Ep,
            _ => Self::Unknown,
        }
    }

    fn is_single_or_ep(&self) -> bool {
        matches!(self, Self::Single | Self::Ep)
    }
}
```

This keeps the classification logic pure and testable.

```rust
fn artist_album_selection_for_singles_mode(
    singles_mode: SinglesMode,
) -> ArtistAlbumSelection {
    match singles_mode {
        SinglesMode::Include => ArtistAlbumSelection::AlbumsAndSingles,
        SinglesMode::Exclude => ArtistAlbumSelection::AlbumsOnly,
        SinglesMode::Only => ArtistAlbumSelection::SinglesAndEpsOnly,
    }
}
```

#### Audio-only download cleanup

Remove the video path from `download` rather than merely hiding it:

- Remove `DownloadArgs.video_quality`.
- Remove `EffectiveDownloadOptions.video_quality`.
- Remove `EffectiveDownloadOptions.videos_mode`.
- Remove `parse_or_config_video`.
- Remove `should_skip_entity`.
- Remove `CatalogEntity::Video` handling from download expansion and
  `download_item_for_entity`.
- Remove `MediaMetadataInput::Video` from the download pipeline if it is used
  only by `download`.
- Direct video URLs resolve successfully only as far as provider resolution;
  expansion returns an empty audio item list and the command completes with a
  summary showing zero downloadable audio tracks. This is not a failure.

`list video` and search result display are outside this slice unless the
implementation naturally removes dead imports. The audio-only requirement is
for the download command and download config surface.

Direct video URL behavior:

- `expand_download_entities` returns `ExpansionResult::default()` for
  `CatalogEntity::Video`.
- The download command does not emit a video operation row for that input,
  because there are no audio items to schedule.
- `OperationKind::Video` is not produced anywhere in the download command after
  this cleanup.

#### Singles filtering

Applied during artist expansion before album items are fetched:

```rust
fn should_skip_by_singles_mode(album: &Album, singles_mode: SinglesMode) -> bool {
    let album_type = AlbumType::from_release_type(album.release_type.as_deref());
    match singles_mode {
        SinglesMode::Include => false,
        SinglesMode::Exclude => album_type.is_single_or_ep(),
        SinglesMode::Only => !album_type.is_single_or_ep(),
    }
}
```

This operates on `&Album` directly because singles filtering is applied
per-album during artist expansion, before individual tracks are fetched.

**Revised approach:** For artist downloads, filtering happens during
expansion, not after. `expand_download_entities` calls
`provider.artist_albums_for_selection()` with the `ArtistAlbumSelection` mapped
from `SinglesMode`, applies the pure `SinglesMode` filter as a defensive check,
then calls `provider.children()` per album to get tracks:

```rust
CatalogEntity::Artist(_) => {
    let albums = provider
        .artist_albums_for_selection(
            reference.id.as_str(),
            artist_album_selection_for_singles_mode(singles_mode),
        )
        .await
        .context("failed to fetch artist albums")?;

    let mut result = ExpansionResult::default();
    for album in albums {
        if should_skip_by_singles_mode(&album, singles_mode) {
            continue;
        }
        let children = match provider.children(&album.id).await {
            Ok(children) => children,
            Err(error) if skip_errors => {
                result.summary.failed_expansions += 1;
                result.summary.failure_messages.push(format!(
                    "failed to fetch album items for {}: {error:#}",
                    album.title
                ));
                continue;
            }
            Err(error) => {
                return Err(error).context("failed to fetch album items");
            }
        };
        for child in children {
            if matches!(child, CatalogEntity::Track(_)) {
                result.items.push((child, Some(album.clone())));
            }
        }
    }
    Ok(result)
}
```

This returns `(entity, parent_album)` tuples so the download loop has album
context for metadata and template rendering. `download_entity` already accepts
`parent_album: Option<&Album>`, so no signature change is needed there.

#### `expand_download_entities` changes

Currently returns `Vec<CatalogEntity>`. Change return type to carry optional
album context and expansion failures:

```rust
async fn expand_download_entities(
    provider: &TidalProvider,
    reference: &MediaRef,
    entity: CatalogEntity,
    singles_mode: SinglesMode,
    skip_errors: bool,
) -> Result<ExpansionResult>

#[derive(Debug, Default)]
struct ExpansionResult {
    items: Vec<(CatalogEntity, Option<Album>)>,
    summary: DownloadSummary,
}
```

For albums: expands via `children()`, `parent_album` = the album.
For playlists: expands via `children()`, keeps audio tracks, `parent_album` = `None`.
For tracks: wraps as single-element, `parent_album` = `None`.
For non-track media items: returns an empty list. Non-audio downloads are out of
scope for this slice.
For artists: calls `artist_albums_for_selection()` to fetch only the album
groups needed for the active `SinglesMode`, applies `should_skip_by_singles_mode`
as a defensive second pass, then calls `children()` per remaining album to get
tracks, attaching each album as `parent_album`.
For mixes/collections: expands items via `children()`, keeps audio tracks,
`parent_album` = `None`.

This eliminates the separate `parent_album` variable that currently lives in
`run_download`. It does **not** remove the `album_context_for_item` fallback:
direct track URLs, playlist items, and mix items still need the fallback album
lookup for album templates and metadata when no parent album is available.

Partial expansion failure semantics:

- Failure to fetch the initial entity, artist metadata, or needed artist album
  endpoint is
  fatal because there is no reliable operation set to continue.
- Failure to fetch one album's items during artist expansion is per-album. With
  `--skip-errors`, increment `summary.failed_expansions`, record a failure
  message for the final summary, and continue with the remaining albums. Without
  `--skip-errors`, propagate the contextual error immediately.
- Failure to fetch playlist or mix children is fatal. There is no subgroup list
  to continue from in this slice.
- Expansion failures do not emit `ItemOutcome::Failed` because no track item ID
  exists yet. They are represented in `DownloadSummary`.

#### Template prevalidation

The template parser already validates field names against allowed namespaces at
parse time (typos like `{album.titl}` are caught by `CompiledTemplate::parse`).
Prevalidation adds a *render-time* check: render the already compiled template
with a fully populated synthetic context to verify that format specifiers,
conditional logic, and namespace combinations produce a valid path before any
network calls.

```rust
fn prevalidate_template(template: &CompiledTemplate) -> Result<()> {
    let test_context = TemplateContext::synthetic();
    template
        .render_with_options(&test_context, TemplateRenderOptions { with_asterisk_ext: false })
        .context("template validation failed — check format specifiers and field combinations")?;
    Ok(())
}
```

`TemplateContext::synthetic()` is a new constructor that returns a context with
all namespaces populated with placeholder values (e.g., `item.title = "test"`,
`album.title = "test"`, `item.number = 1`). This ensures the render path
exercises every field the template references. If the template only uses
fields from one namespace (e.g., only `item.*`), the render succeeds because
unused namespaces simply aren't referenced.

Call `prevalidate_template` immediately after building `EffectiveDownloadOptions`
and before any provider/client construction, token loading, URL parsing, or
network-backed state setup. In the current CLI flow this means it must run
before `tidal_client(...)` and before the input URL loop.

Tests:
- Valid template passes.
- A template parse error, such as an invalid date format, fails before provider
  parsing, token loading, or any network request.
- A syntactically valid template is rendered against the synthetic context
  before provider parsing, token loading, or any network request.

Note: field name typos and invalid date format strings are already caught at
parse time by `CompiledTemplate::parse`. `prevalidate_template` catches
render-time issues that require a context.

#### `--skip-errors` flag

```rust
/// Error behavior flags.
#[derive(Debug, Args)]
struct ErrorFlags {
    /// Continue downloading when individual items fail.
    #[arg(long)]
    skip_errors: bool,
}
```

When active:
- Failed items emit `ItemOutcome::Failed` and do not abort the operation.
- The operation completes with a summary that includes the failure count.
- `run_download` returns `Result<DownloadSummary>`. Fatal setup errors still
  propagate with context. Completed downloads with item failures return `Ok`
  with failed counters set; the command layer exits with code 1 after rendering
  the summary.

When not active (default):
- The first failed item aborts the entire operation. Because downloads are
  concurrent, this must be implemented with a bounded task pipeline, not by
  spawning every item before awaiting failures.

Concurrency model:

- Replace the current "spawn all tasks, then await" shape with a bounded
  `JoinSet` pipeline that keeps at most `concurrency` item tasks in flight.
- When `skip_errors` is `false`, the first item error aborts the remaining
  in-flight tasks with `JoinSet::abort_all()` and returns that contextual error.
  Items not yet scheduled do not start.
- When `skip_errors` is `true`, all items are scheduled and all item errors are
  converted into summary failures.
- M3U entries keep deterministic input ordering by carrying the original item
  index through each task result and sorting successful entries before writing.
- Fatal setup or expansion errors occur before item task scheduling and continue
  to return immediately.

Implementation: wrap `download_entity` in a catch that converts errors to
`ItemOutcome::Failed` events when `skip_errors` is true:

```rust
let result = download_entity(...).await;
match result {
    Ok(ItemCompletion::Downloaded(entry)) => {
        summary.downloaded += 1;
        /* ... */
    }
    Ok(ItemCompletion::Skipped) => {
        summary.skipped += 1;
        /* ... */
    }
    Err(error) if options.skip_errors => {
        summary.failed_items += 1;
        let message = format!(
            "{} ({}) failed: {error:#}",
            item_label,
            item_ref,
        );
        summary.failure_messages.push(message.clone());
        sink.emit(DownloadEvent::ItemFinished {
            item_id,
            outcome: ItemOutcome::Failed {
                message,
            },
        });
    }
    Err(error) => return Err(error),
}
```

`DownloadSummary` is owned by the CLI orchestration layer, not by
`ProgressSink`. The progress sink is an event consumer and remains write-only.

```rust
#[derive(Debug, Default, Clone)]
struct DownloadSummary {
    downloaded: usize,
    skipped: usize,
    failed_items: usize,
    failed_expansions: usize,
    failure_messages: Vec<String>,
}

impl DownloadSummary {
    fn merge(&mut self, other: DownloadSummary);
    fn has_failures(&self) -> bool {
        self.failed_items > 0 || self.failed_expansions > 0
    }
}
```

`run_download` aggregates a single `DownloadSummary` across all input URLs. The
top-level `Command::Download` branch handles the returned summary:

```rust
let summary = run_download(&options, args).await?;
if summary.has_failures() {
    eprintln!(
        "download completed with {} failed item(s) and {} failed expansion(s)",
        summary.failed_items,
        summary.failed_expansions
    );
    print_failure_messages(&summary.failure_messages, 20);
    std::process::exit(1);
}
Ok(())
```

Using `std::process::exit(1)` is acceptable only at the binary command edge.
Library crates must continue returning structured errors.

`print_failure_messages` prints at most 20 detailed failure messages and then a
single `... N more failure(s)` line. Both item failures and expansion failures
append contextual messages to `failure_messages`, so a non-zero completion is
actionable without rerunning in verbose mode.

Item failure messages must include the user-visible item label and the
`MediaRef`, not only the lower-level phase error. This keeps failures such as
`failed to resolve stream` actionable in multi-item runs.

#### `EffectiveDownloadOptions` additions

```rust
struct EffectiveDownloadOptions {
    // ... existing fields ...
    singles_mode: SinglesMode,
    skip_errors: bool,
}
```

`singles_mode` comes from config and can be overridden with the new
`--singles include|exclude|only` CLI flag.

`skip_errors` comes from the `--skip-errors` CLI flag, defaulting to `false`.

#### `--singles` flag

```rust
#[arg(long, value_enum)]
singles: Option<SinglesModeArg>,

#[derive(Debug, Clone, Copy, ValueEnum)]
enum SinglesModeArg {
    Include,
    Exclude,
    Only,
}
```

`SinglesModeArg` maps into `riptide_config::SinglesMode`. The CLI flag overrides
config only for the current invocation.

## Implementation Steps

### Step 1: Config and CLI audio-only cleanup

1. Remove `video_quality`, `videos_mode`, and `download.templates.video` from
   `riptide-config` raw and validated download config.
2. Remove `VideosMode` re-exports if no remaining code needs them.
3. Remove download command `--video-quality` and all video-specific effective
   download options.
4. Remove video handling from download expansion and item download selection.

Tests:
- Config no longer accepts `download.video_quality`.
- Config no longer accepts `download.videos_mode`.
- Config no longer accepts `download.templates.video`.
- Direct video URL download results in zero audio items and no stream request.
- Direct video URL download emits no video operation row and never produces
  `OperationKind::Video`.
- Mixed collection expansion ignores video entities and downloads tracks only.

### Step 2: TIDAL client artist and mix methods

1. Add `artist(id)` method — `GET artists/{id}`, return `Artist`.
2. Add `paged_albums(endpoint, filter)` private helper — paginate typed album
   responses.
3. Add `artist_albums(id)` — calls `paged_albums("artists/{id}/albums", "ALBUMS")`.
4. Add `artist_singles(id)` — calls `paged_albums("artists/{id}/albums", "EPSANDSINGLES")`.
5. Add `artist_albums_all(id)` — extends albums with singles/EPs and
   deduplicates by album `MediaRef`.
6. Add `ArtistAlbumSelection` and `artist_albums_for_selection(id, selection)`.
7. Add `mix_items(id)` — `paged_items("mixes/{id}/items")`, return `Vec<CatalogEntity>`.

Tests:
- Artist albums returns albums only (not singles).
- Artist singles returns singles/EPs only.
- Mix items preserves provider item conversion; audio-only filtering happens in
  CLI expansion.
- Pagination terminates when page is smaller than limit.
- Pagination with an exact-limit page followed by an empty page terminates.
- `artist_albums_all` is visible to `TidalProvider` and compiles from
  `provider.rs`.
- Combined artist album results preserve first-seen order and deduplicate by
  `Album.id`.
- `ArtistAlbumSelection::AlbumsOnly` does not call the singles endpoint.
- `ArtistAlbumSelection::SinglesAndEpsOnly` does not call the albums endpoint.

### Step 3: TidalProvider resolve and children extensions

1. Add `Artist` resolution in `resolve()`.
2. Add `Mix` resolution in `resolve()` (returns `Collection` with `CollectionKind::Mix`).
3. Add `Artist` case in `children()` — returns all albums as `CatalogEntity::Album`.
4. Add `Mix` case in `children()` — delegates to `mix_items`.

Tests:
- `resolve` for artist URL returns `CatalogEntity::Artist`.
- `resolve` for mix URL returns `CatalogEntity::Collection` with `Mix` kind.
- `children` for artist returns albums, not flattened items.
- `children` for mix returns converted catalog entities.

### Step 4: `AlbumType` classification and singles filtering

1. Define `AlbumType` enum and `from_release_type` constructor.
2. Implement `is_single_or_ep()`.
3. Define `should_skip_by_singles_mode(album, singles_mode)`.
4. Define `artist_album_selection_for_singles_mode`.
5. Wire `singles_mode` into `EffectiveDownloadOptions`.
6. Add `--singles include|exclude|only` CLI override.

Tests:
- `AlbumType::from_release_type("ALBUM")` → `Album`.
- `AlbumType::from_release_type("SINGLE")` → `Single`.
- `AlbumType::from_release_type("EP")` → `Ep`.
- `AlbumType::from_release_type(None)` → `Unknown`.
- `Unknown` is not treated as single/EP (included in `Exclude` mode,
  excluded in `Only` mode).
- `artist_album_selection_for_singles_mode` maps each `SinglesMode` to the
  expected `ArtistAlbumSelection`.
- CLI `--singles only` overrides config value.

### Step 5: `expand_download_entities` refactor

1. Refactor `expand_download_entities` to return `ExpansionResult`.
2. For albums: expand via `children()`, `parent_album` = the album.
3. For playlists: expand via `children()`, retain tracks, `parent_album` = `None`.
4. For tracks: wrap as single-element with `parent_album` = `None`.
5. For non-track media items: return an empty list.
6. For artists: call `artist_albums_for_selection()` to fetch only the album
   groups needed for the active `SinglesMode`, apply `SinglesMode` filter via
   `should_skip_by_singles_mode` as a defensive second pass, call `children()`
   per remaining album, attach each album as `parent_album`, retain tracks.
7. For mixes/collections: expand items via `children()`, retain tracks,
   `parent_album` = `None`.
8. Remove the separate `parent_album` variable from `run_download`.
9. Keep `album_context_for_item` as fallback when tuple album context is `None`.
10. Record per-album expansion failures in `DownloadSummary` when
    `--skip-errors` is active.

Tests:
- Album expansion returns entities with album as parent.
- Artist expansion with `SinglesMode::Exclude` omits singles.
- Artist expansion with `SinglesMode::Only` keeps only singles/EPs.
- Mix expansion returns flat entity list.
- Track pass-through returns a single element.
- Non-track media pass-through returns no elements.
- Direct track with an album template still uses fallback album lookup.
- Artist expansion with `--skip-errors` continues when one album's item fetch
  fails and records `failed_expansions`.
- Artist expansion without `--skip-errors` aborts on the first failed album item
  fetch.
- Artist expansion with `SinglesMode::Exclude` still succeeds if the singles
  endpoint would fail, because that endpoint is not called.

### Step 6: Template prevalidation

1. Add `TemplateContext::synthetic()` constructor in `riptide-template` that
   returns a context with all namespaces populated with placeholder values.
2. Add `prevalidate_template` function in `riptide-cli`.
3. Call it immediately after effective option construction and before
   `tidal_client(...)`, token loading, provider parsing/resolution, or network
   work.

Tests:
- Valid template passes.
- Invalid template parse errors fail before provider parsing/resolution, token
  loading, or network work.
- `prevalidate_template` runs before provider parsing/resolution, token loading,
  or network work for valid templates.

Note: field name typos are already caught at parse time by
`CompiledTemplate::parse`. This step catches render-time issues only.

### Step 7: `--skip-errors` flag

1. Add `ErrorFlags` to `DownloadArgs`.
2. Add `skip_errors` to `EffectiveDownloadOptions`.
3. Replace eager task spawning with a bounded `JoinSet` download scheduler.
4. Wrap `download_entity` result in error-catching logic.
5. Emit `ItemOutcome::Failed` on caught errors.
6. Return `Result<DownloadSummary>` from `run_download`.
7. Track item and expansion failures in CLI-owned `DownloadSummary`.
8. Aggregate summaries across multiple input URLs.
9. Convert a completed summary with any failures into process exit code 1 at
   the binary command edge.
10. Print capped detailed failure messages after the summary counts.

Tests:
- With `skip_errors`: failed item does not abort remaining downloads.
- Without `skip_errors`: first failed item aborts in-flight tasks and propagates
  the contextual error.
- Summary includes failed count.
- Multi-input download aggregates failures across all inputs.
- Completed download with failures exits non-zero after printing summary.
- Summary output includes capped detailed failure messages.
- Successful M3U entries remain in deterministic input order despite concurrent
  completion order.

## Crate Boundary Summary

| Change | Crate |
| --- | --- |
| `ArtistAlbumSelection`, `artist()`, `artist_albums()`, `artist_singles()`, `artist_albums_all()`, `artist_albums_for_selection()`, `mix_items()` | `riptide-provider-tidal` |
| `resolve` for Artist/Mix, `children` for Artist/Mix | `riptide-provider-tidal` |
| `AlbumType`, singles filter, `--singles`, `--skip-errors`, template prevalidation, `DownloadSummary` | `riptide-cli` |
| `TemplateContext::synthetic()` constructor | `riptide-template` |
| Remove video download config keys and video template override | `riptide-config` |

No changes to `riptide-core`, `riptide-media`, `riptide-metadata`, or
`riptide-progress`.

## Performance Notes

- Artist album expansion fetches albums first (paginated), then fetches items
  for each album sequentially. This is acceptable because albums are cached
  (1 hour TTL) and the pagination is light (typically <10 albums).
- Item downloads within an artist run concurrently via the existing semaphore.
- Album items are fetched per-album, not as a single artist-wide batch. This
  matches the TIDAL API structure and keeps individual album caching effective.

## Error Handling

- `--skip-errors` catches per-item errors. Album-level expansion errors (e.g.
  album 404) within artist downloads are caught and reported only when
  `--skip-errors` is active.
- Network errors during artist album enumeration are fatal (the operation
  cannot proceed without knowing what to download).
- Template prevalidation errors are fatal and reported before any network calls.
- With `--skip-errors`, the process still exits with code 1 when the completed
  summary has any item or expansion failures.
- Non-skip item failures abort in-flight item tasks and return the first
  contextual item error. Already-completed successful item results are discarded
  for side-effect outputs such as M3U because the command did not complete
  successfully.

## Review Checklist

- All new TIDAL client methods use the existing `paged_items` or the new
  `paged_albums` helper — no manual pagination loops.
- Artist album endpoint selection avoids fetching endpoint groups that the
  active `SinglesMode` will discard.
- Combined artist album results are ordered and deduplicated by `Album.id`.
- `SinglesMode` filtering is pure and testable without a provider.
- `AlbumType` classification is derived from data, not from the TIDAL API
  filter parameter (they are independent).
- `Unknown` album type is treated as a main album (not filtered out).
- No new crate dependencies.
- Download config and CLI no longer expose video-specific options.
- `children()` for artists returns albums (not flattened items); the CLI
  iterates albums, filters by `SinglesMode`, and fetches items per album.
- Template prevalidation uses a synthetic context with all namespaces
  populated; runs before any network calls.
- `--skip-errors` emits structured `ItemOutcome::Failed` events, not raw
  error messages. Exit code is non-zero when any items failed.
- `run_download` returns `Result<DownloadSummary>` to preserve fatal setup error
  context while threading completed failure state to the caller.
- `DownloadSummary` is CLI-owned and not retrieved from `ProgressSink`.
- Non-skip fail-fast behavior is implemented with a bounded `JoinSet` scheduler
  that aborts in-flight item tasks on the first item error.
- Detailed failure messages are printed with a cap, not only counted.
- Mix resolution returns `Collection` (not a new entity kind).
- Non-track media entities are filtered out during expansion in this audio-only
  slice.

## Completion Criteria

- `riptide-rs download https://tidal.com/artist/...` downloads audio tracks from
  all selected albums.
- `riptide-rs download https://tidal.com/mix/...` downloads all tracks.
- `singles_mode = "exclude"` in config omits singles/EPs from artist downloads.
- `--singles only` overrides config for one invocation.
- `--skip-errors` allows partial artist downloads to complete.
- `--skip-errors` returns process exit code 1 if any item or album expansion
  failed.
- Completed-with-failures summaries include actionable failure details, capped
  to avoid flooding the terminal.
- Invalid templates fail immediately, before any downloads start.
- `cargo fmt`, `cargo clippy --workspace --all-targets`, and
  `cargo test --workspace` pass.
