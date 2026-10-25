# riptide-rs Download Progress UX Spec

## Status

Draft for review. This plan covers the terminal download experience and the
provider-neutral progress/event API needed to support it.

This is intentionally scoped to progress and status reporting. It does not
change stream resolution, manifest parsing, metadata tagging, cover fetching, or
download templates except where those operations need to emit typed events.

## Goals

- Make active download rows readable during concurrent downloads.
- Show song names first, not output paths.
- Show two-digit track numbers before track names when available.
- Show resolved quality with a compact symbol and text.
- Add overall context for the thing being downloaded: track, album, playlist,
  artist, mix, or collection.
- Keep completed song lines human-facing and omit song file paths by default.
- Expose provider-neutral progress APIs that can support TIDAL streams, future
  providers, local file copies, metadata-only enrichment, and providers without
  cover art.
- Keep all terminal rendering in `riptide-cli`; library crates emit typed
  events and pure row models only.

## Non-Goals

- No new dynamic plugin system.
- No changes to the provider trait shape in this slice.
- No terminal dashboard crate dependency beyond `indicatif`.
- No persistent progress database.
- No full artist download implementation if it is still outside the active
  download slice; the UX API must support artist/group progress, but runtime
  behavior only needs to cover commands implemented at the time.
- No song output paths in default completed rows. Paths may appear in verbose or
  path-detail modes.

## UX Decisions

### Active Item Rows

Active item rows should optimize for recognition while several files move at the
same time. The primary label is the media title, not the rendered output path.

For tracks and videos with a track number:

```text
01 Pull Me Under
05 Metropolis - Part I
```

For tracks and videos without a track number:

```text
Pull Me Under
Metropolis - Part I
```

If artist context is useful and the row is not already nested under an album or
artist row, render:

```text
Dream Theater - 01 Pull Me Under
```

### Active Row Columns

Default interactive active item rows use this order:

```text
{symbol} {phase:<7} [{bar}] {bytes/speed} {quality_badge:<14} {item_label}
```

Examples:

```text
● dl       [██████████░░░░░░░░]  28.4/42.1 MiB  4.8 MiB/s  ◇ Lossless      01 Pull Me Under
◆ tag      [                  ]                            ◇ Lossless      03 Take the Time
○ queued   [                  ]                            ◇ Lossless      08 Learning to Live
```

Bytes and speed are shown only during the transfer phase. Non-transfer phases
must not carry stale throughput values.

### Completion Rows

Completed song rows omit paths by default:

```text
✓ done      ◇ Lossless  01 Pull Me Under
• exists    ◇ Lossless  02 Another Day
✕ failed    ◇ Lossless  07 Wait for Sleep  stream unavailable
```

Artifact completion rows may show artifact file names:

```text
✓ cover     cover.jpg
✓ playlist  riptide.m3u
✓ lyrics    01 Pull Me Under.lrc
```

Full paths are available only in a detail mode:

```text
--progress-detail path
--progress-detail both
```

### Overall Rows

The overall row should describe the resource, not just item count.

Single track during download:

```text
● track    [████████████░░░░░░]  28.4/42.1 MiB  4.8 MiB/s  ◇ Lossless  Dream Theater - 01 Pull Me Under
```

Single track completed:

```text
✓ done     ◇ Lossless  Dream Theater - 01 Pull Me Under

Downloaded 1 track in 8s
```

Album during download:

```text
● album    [██████████████░░░░]  5/8 tracks  142.6/318.4 MiB  Dream Theater - Images and Words

  ● dl     [██████████░░░░░░░░]  28.4/42.1 MiB  4.8 MiB/s  ◇ Lossless      01 Pull Me Under
  ● dl     [██████░░░░░░░░░░░░]  12.9/38.0 MiB  3.1 MiB/s  ◇ Lossless      05 Metropolis - Part I
  ◆ tag    [                  ]                            ◇ Lossless      03 Take the Time
  ◇ cover  [                  ]                            cover.jpg
  ○ queued [                  ]                            ◇ Lossless      08 Learning to Live
```

Album completed:

```text
✓ done     ◇ Lossless  01 Pull Me Under
✓ done     ◇ Lossless  02 Another Day
✓ done     ◇ Lossless  03 Take the Time
✓ done     ◇ Lossless  04 Surrounded
✓ done     ◇ Lossless  05 Metropolis - Part I
✓ done     ◇ Lossless  06 Under a Glass Moon
✓ done     ◇ Lossless  07 Wait for Sleep
✓ done     ◇ Lossless  08 Learning to Live
✓ cover    cover.jpg
✓ playlist riptide.m3u

Downloaded 8 tracks in 42s
Saved cover: cover.jpg
Saved playlist: riptide.m3u
```

Artist during download:

```text
● artist   [██████░░░░░░░░░░░░]  2/5 albums  17/46 tracks  Dream Theater

  ● album  [████████████░░░░░░]  5/8 tracks   Images and Words
    ● dl   [██████████░░░░░░░░]  28.4/42.1 MiB  4.8 MiB/s  ◇ Lossless  01 Pull Me Under
    ◆ tag  [                  ]                            ◇ Lossless  03 Take the Time

  ● album  [██████░░░░░░░░░░░░]  4/11 tracks  Awake
    ● dl   [████████░░░░░░░░░░]  19.1/48.0 MiB  3.9 MiB/s  ◇ Lossless  02 6:00

  ○ album  [                  ]  queued       Falling Into Infinity
```

Artist completed:

```text
✓ album    Images and Words        8 tracks
✓ album    Awake                   11 tracks
✓ album    Falling Into Infinity   11 tracks
✓ album    Metropolis Pt. 2        12 tracks
✓ album    Six Degrees             2 discs, 13 tracks
✓ cover    5 album covers
✓ playlist riptide.m3u

Downloaded 55 tracks across 5 albums in 6m18s
Saved covers: 5
Saved playlist: riptide.m3u
```

For artists, default completion summarizes groups. A future
`--completion-detail tracks` mode may print every track:

```text
✓ done     ◇ Lossless  Images and Words / 01 Pull Me Under
```

### Symbols

Unicode symbols are enabled by default only for interactive terminals that can
render them. ASCII and no-symbol modes must be available.

| Meaning | Unicode | ASCII | No symbol |
| --- | --- | --- | --- |
| Active/download | `●` | `*` | empty |
| Queued | `○` | `.` | empty |
| Non-transfer phase | `◆` | `>` | empty |
| Artifact phase | `◇` | `>` | empty |
| Done | `✓` | `v` | empty |
| Existing/skipped | `•` | `=` | empty |
| Failed | `✕` | `!` | empty |

### Quality Badges

Quality badges include a symbol plus text. The symbol is a scan aid only; text
is always present in default mode.

| Quality | Badge |
| --- | --- |
| Audio low | `• Low` |
| Audio high | `▪ High` |
| Audio lossless | `◇ Lossless` |
| Audio hi-res lossless | `◆ Hi-Res` |
| Audio hi-res with metrics | `◆ Hi-Res 24/96` |
| Video low | `▣ 360p` |
| Video medium | `▣ 720p` |
| Video high | `▣ 1080p` |
| Provider-specific | `◇ {label}` |
| Unknown | `◇ Unknown` |

The badge is derived from `StreamVariant.quality`, `bit_depth`, and
`sample_rate`, not from the requested quality. If a local backend has no stream
quality, it may emit `QualityBadge::unknown()` or omit the badge in a typed
`Option<QualityBadge>`.

### Color

Color is enabled by default in interactive terminals and disabled when output is
redirected.

| Field | Color |
| --- | --- |
| `dl` / active transfer | cyan |
| `queued`, `resolve` | dim |
| `tag`, `cover`, `lyrics`, `persist` | yellow |
| `done` | green |
| `exists` | yellow/dim |
| `failed` | red |
| Lossless quality | cyan |
| Hi-res quality | yellow/bold |
| Low quality | dim |

Controls:

```text
--color auto    # default
--color always
--color never

--symbols auto  # default
--symbols unicode
--symbols ascii
--symbols none
```

## Crate Boundaries

### `riptide-core`

Keep provider-neutral catalog and stream types only. Do not add terminal UI,
`indicatif`, or renderer concepts.

Small domain helpers may be added if they are broadly useful and pure:

```rust
impl TrackNumber {
    pub fn two_digit(self) -> String;
}
```

This helper is optional. The progress crate can format track numbers without
changing core.

### New Crate: `riptide-progress`

Owns provider-neutral progress/event and row-model types. It must not depend on
`indicatif`, `clap`, `tokio`, `reqwest`, `riptide-provider-tidal`, or terminal
color crates.

Dependencies:

```toml
riptide-core = { path = "../riptide-core" }
time = { workspace = true }
thiserror = { workspace = true }
indexmap = { workspace = true }  # insertion-ordered map for stable row display order
```

`serde` is intentionally absent. No type in `riptide-progress` needs serialization in this slice. Add it only if a concrete use case (e.g. IPC, logging) arises.

Candidate modules:

```text
src/lib.rs
src/id.rs          ID newtypes and ProgressIdAllocator
src/event.rs       typed event model
src/sink.rs        ProgressSink trait (owned here; impls live in riptide-cli)
src/label.rs       pure label and quality badge builders
src/state.rs       event reducer into renderable progress state
src/row.rs         provider-neutral row models + DownloadPhase→RowSymbol mapping
src/summary.rs     completion counters and summary text model
```

Public API candidates:

```rust
/// ID newtypes must derive `Clone, Copy, PartialEq, Eq, Hash, Debug` so they
/// can be used as `HashMap` keys and passed across async tasks without cloning.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct OperationId(u64);
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct GroupId(u64);
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ItemId(u64);

/// Public enums that cross crate boundaries carry `#[non_exhaustive]` so adding
/// variants as new providers are added is not a semver break. This does force
/// wildcard arms outside `riptide-progress`; renderers must map unknown future
/// variants to neutral fallback text/symbols rather than panicking.
#[non_exhaustive]
pub enum OperationKind {
    Track,
    Video,
    Album,
    Playlist,
    Artist,
    Mix,
    Collection,
    LocalTransfer,
    MetadataEnrichment,
}

/// A group inside an artist operation uses the same `OperationKind` enum
/// (e.g. `OperationKind::Album`). Group kind and operation kind are the same
/// type by design: a group is always a "sub-operation" of the same shape.
///
/// `Artifact` is intentionally absent — artifacts have their own event
/// lifecycle (`ArtifactPhaseChanged` / `ArtifactFinished`) and are never
/// queued as items via `ItemQueued`.
#[non_exhaustive]
pub enum ItemKind {
    Track,
    Video,
    LocalFile,
}

#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum ArtifactKind {
    Cover,
    Lyrics,
    Playlist,
    Metadata,
    Other,
}

/// Terminal states (`Done`, `Exists`, `Failed`) are intentionally absent.
/// Those are expressed through `ItemOutcome` in `ItemFinished`. The phase
/// enum ends at the last in-progress step. The renderer transitions from an
/// active row to a completion row when it receives `ItemFinished`, not when
/// it observes a terminal phase value.
#[non_exhaustive]
pub enum DownloadPhase {
    Queued,
    ResolvingMetadata,
    ResolvingStream,
    ResolvingMedia,
    Downloading,
    Copying,
    PostProcessing,
    Tagging,
    Persisting,
    WritingLyrics,
    WritingCover,
    WritingPlaylist,
}

pub struct DisplayLabel {
    pub title: String,
    pub artist: Option<String>,
    pub group: Option<String>,
    /// Uses `riptide_core::TrackNumber` rather than a raw integer to preserve
    /// the core newtype semantics across the boundary.
    pub track_number: Option<riptide_core::TrackNumber>,
}

pub struct QualityBadge {
    pub symbol_kind: QualitySymbol,
    pub text: String,
    pub detail: Option<String>,
}

#[non_exhaustive]
pub enum QualitySymbol {
    Low,
    High,
    Lossless,
    HiRes,
    Video,
    Unknown,
}

#[non_exhaustive]
pub enum TransferMode {
    RemoteStream,
    LocalCopy,
}

pub struct TransferProgress {
    pub mode: TransferMode,
    pub bytes_done: u64,
    pub bytes_total: Option<u64>,
    /// Always known once a segment-based transfer is in progress; not `Option`.
    /// `segments_total` is `Option` because live streams may not report a
    /// segment count upfront.
    pub segments_done: u64,
    pub segments_total: Option<u64>,
}

/// Passed to `ProgressState::new`. Not a `DownloadEvent` variant — removing it
/// from the event enum makes the lifecycle unambiguous: the state is constructed
/// with this data, then subsequent events flow through `reduce`. There is no
/// valid reducer path for "start the operation again".
pub struct OperationConfig {
    pub id: OperationId,
    pub kind: OperationKind,
    pub label: DisplayLabel,
    pub total_groups: Option<u64>,
    pub total_items: Option<u64>,
}

/// Events that flow through `ProgressState::reduce`. `OperationStarted`
/// (renamed `OperationConfig`) is not in this enum.
#[non_exhaustive]
pub enum DownloadEvent {
    /// Used when paginated providers discover totals after the operation row
    /// has already been announced. `None` means "no change to this field";
    /// it does not clear an existing total. Totals are monotonic best-known
    /// values — a second update may raise but must not lower a prior total.
    OperationTotalsUpdated {
        id: OperationId,
        total_groups: Option<u64>,
        total_items: Option<u64>,
    },
    /// `kind` reuses `OperationKind` — a group is a sub-operation of the same
    /// shape (e.g. `OperationKind::Album` inside an artist operation).
    GroupStarted {
        operation_id: OperationId,
        group_id: GroupId,
        kind: OperationKind,
        label: DisplayLabel,
        total_items: Option<u64>,
    },
    /// Used when a group's total item count is discovered after `GroupStarted`.
    /// `None` means "no change"; does not clear an existing total.
    GroupTotalsUpdated {
        group_id: GroupId,
        total_items: Option<u64>,
    },
    ItemQueued {
        operation_id: OperationId,
        group_id: Option<GroupId>,
        item_id: ItemId,
        kind: ItemKind,
        label: DisplayLabel,
        /// Optional because the resolved stream quality is usually unknown when
        /// the item is queued. The common flow is `quality: None` followed by
        /// `ItemQualityResolved` after stream resolution.
        quality: Option<QualityBadge>,
    },
    ItemQualityResolved {
        item_id: ItemId,
        quality: QualityBadge,
    },
    ItemPhaseChanged {
        item_id: ItemId,
        phase: DownloadPhase,
    },
    /// Renamed from `TransferProgress` to avoid collision with the struct of
    /// the same name. Consumers match on `DownloadEvent::TransferUpdate`.
    TransferUpdate {
        item_id: ItemId,
        progress: TransferProgress,
    },
    /// The reducer matches artifact events on `ArtifactKey`, which includes
    /// `item_id` to handle per-track artifacts. A single album group produces
    /// one `.lrc` file per track (all `ArtifactKind::Lyrics`), so
    /// `(operation_id, group_id, kind)` alone would collide. For group-level
    /// artifacts (album cover, M3U) `item_id` is `None`.
    ArtifactPhaseChanged {
        operation_id: OperationId,
        group_id: Option<GroupId>,
        /// `Some` for per-track artifacts (lyrics, track art).
        /// `None` for group-level artifacts (album cover, playlist file).
        item_id: Option<ItemId>,
        kind: ArtifactKind,
        label: String,
        phase: DownloadPhase,
    },
    ArtifactFinished {
        operation_id: OperationId,
        group_id: Option<GroupId>,
        item_id: Option<ItemId>,
        kind: ArtifactKind,
        label: String,
        outcome: ArtifactOutcome,
    },
    /// Emitted once the output path is known (after template expansion).
    /// Used by `--progress-detail path|both` renderers; default title mode
    /// ignores it. Not in `DisplayLabel`; lives only in `ItemState.output_path`
    /// and is never shown unless the CLI renderer option requests it.
    ItemOutputResolved {
        item_id: ItemId,
        path: std::path::PathBuf,
    },
    ItemFinished {
        item_id: ItemId,
        outcome: ItemOutcome,
    },
    GroupFinished {
        group_id: GroupId,
        outcome: GroupOutcome,
    },
    /// No `summary` field. The reducer derives `ProgressSummary` from
    /// accumulated `ProgressState` when this event is processed, rather than
    /// accepting a caller-supplied summary that could diverge from reduced state.
    /// The CLI reads `state.summary()` after the reducer handles this event.
    OperationFinished {
        id: OperationId,
    },
}

#[non_exhaustive]
pub enum ItemOutcome {
    Downloaded,
    Copied,
    Enriched,
    Exists,
    Skipped { reason: String },
    Failed { message: String },
}

#[non_exhaustive]
pub enum ArtifactOutcome {
    Written,
    Skipped { reason: String },
    Failed { message: String },
}

#[non_exhaustive]
pub enum GroupOutcome {
    Done,
    Partial { failed: u64 },
    Failed { message: String },
}

pub struct ArtifactSummary {
    pub kind: ArtifactKind,
    pub label: String,
    pub count: u64,
}

pub struct ProgressSummary {
    pub downloaded: u64,
    pub copied: u64,
    pub enriched: u64,
    pub skipped_existing: u64,
    /// Items (tracks, videos, local files) that failed.
    pub failed: u64,
    /// Artifact writes (cover, lyrics, playlist) that failed. Kept separate so
    /// the CLI can report "3 tracks failed, 1 cover failed" without ambiguity.
    pub artifacts_failed: u64,
    pub groups_done: u64,
    /// `Option` only to handle operations aborted before all groups were
    /// announced. For a completed operation the value is always `Some`.
    pub groups_total: Option<u64>,
    /// Same rationale as `groups_total`.
    pub items_total: Option<u64>,
    pub elapsed: Option<time::Duration>,
    pub artifacts: Vec<ArtifactSummary>,
}
```

Rules:

- `DisplayLabel` is human identity. It is not a path.
- Paths stay out of default event labels. If needed for verbose output, add a
  separate `OutputPath` field on events or renderer state, not inside
  `DisplayLabel`.
- Events model capabilities as optional events. A backend that has no cover art
  simply never emits cover events.
- A local file backend emits `Copying` and `Copied`, not `Downloading` and
  `Downloaded`, unless it really performs remote transfer.
- A metadata-only backend emits `MetadataEnrichment`, `Tagging`, and `Enriched`
  without stream or transfer events.
- All event reducers are pure. They take prior state plus an event and return
  new state or mutate a provided state struct without I/O.
- `OperationConfig` (the constructor) and `GroupStarted` may be created before
  totals are known. Late totals must use `OperationTotalsUpdated` and
  `GroupTotalsUpdated`.
- `ItemQueued.quality` is optional by design. Resolved stream quality should be
  attached with `ItemQualityResolved` as soon as a `StreamVariant` is available.
- Artifact rows become completion rows only through `ArtifactFinished`.
- Public enum wildcard handling must be tested in renderers: unknown future
  variants map to neutral fallback labels.

ID allocation:

```rust
pub struct ProgressIdAllocator {
    next_operation: AtomicU64,
    next_group: AtomicU64,
    next_item: AtomicU64,
}

impl ProgressIdAllocator {
    pub fn next_operation_id(&self) -> OperationId;
    pub fn next_group_id(&self) -> GroupId;
    pub fn next_item_id(&self) -> ItemId;
}
```

IDs are unique per process within their own ID namespace. A `GroupId` and an
`ItemId` may have the same numeric value; their newtypes prevent accidental
mixing. Tests should not assume stable numeric values unless they allocate them
explicitly.

Pure helper functions:

```rust
pub fn label_for_track(track: &riptide_core::Track) -> DisplayLabel;
pub fn label_for_video(video: &riptide_core::Video) -> DisplayLabel;
pub fn label_for_album(album: &riptide_core::Album) -> DisplayLabel;
pub fn quality_badge_for_stream(stream: &riptide_core::StreamVariant) -> Option<QualityBadge>;
pub fn format_track_number(number: Option<riptide_core::TrackNumber>) -> Option<String>;
```

`DownloadPhase → RowSymbol` mapping lives in `src/row.rs` as a pure function
alongside the row builders. It must not move into `riptide-cli` — the CLI
layer only picks the display character for a given `RowSymbol`, it does not
decide which symbol a phase maps to.

### `ProgressState`

Lives in `src/state.rs`. The pure reducer takes `(&mut ProgressState,
DownloadEvent) -> Result<(), ProgressError>`. Row builders in `src/row.rs` take
`&ProgressState` and return:

- `OverallRow` — the top-level operation row
- `Vec<GroupRow>` — one per active group (empty for single-group operations)
- `Vec<ActiveRow>` — items and artifacts interleaved, ordered by insertion
- `Vec<CompletionRow>` — finished items, artifacts, and groups

Active artifact rows are interleaved with item rows in `Vec<ActiveRow>` so the
renderer does not need to merge two separate lists. For a single-album command
`Vec<GroupRow>` is empty and the album row is the `OverallRow`; for an artist
command the artist row is `OverallRow` and each album is a `GroupRow`.

```rust
/// Construct with `OperationConfig`; reduce all subsequent `DownloadEvent`s.
/// `OperationConfig` is not a `DownloadEvent` variant, so the reducer never
/// receives a "start again" signal. `DuplicateOperation` is not a valid error.
pub struct ProgressState {
    pub operation: OperationState,
    /// Ordered by insertion so display order matches announcement order.
    pub groups: IndexMap<GroupId, GroupState>,
    /// Ordered by insertion for consistent row ordering.
    pub items: IndexMap<ItemId, ItemState>,
    /// Artifact rows keyed by `(operation_id, group_id, kind)`.
    pub artifacts: IndexMap<ArtifactKey, ArtifactState>,
    pub completed: Vec<CompletedRecord>,
}

pub struct OperationState {
    pub id: OperationId,
    pub kind: OperationKind,
    pub label: DisplayLabel,
    pub total_groups: Option<u64>,
    pub total_items: Option<u64>,
}

pub struct GroupState {
    pub id: GroupId,
    pub operation_id: OperationId,
    pub kind: OperationKind,
    pub label: DisplayLabel,
    pub items_done: u64,
    pub total_items: Option<u64>,
}

pub struct ItemState {
    pub id: ItemId,
    pub operation_id: OperationId,
    pub group_id: Option<GroupId>,
    pub kind: ItemKind,
    pub label: DisplayLabel,
    pub quality: Option<QualityBadge>,
    pub phase: DownloadPhase,
    pub transfer: Option<TransferProgress>,
    /// Set by `ItemOutputResolved`. `None` until the output path is known.
    /// Renderers only expose this when `--progress-detail path|both` is active.
    pub output_path: Option<std::path::PathBuf>,
}

/// Used as an `IndexMap` key and appears in `ProgressError::UnknownArtifact`.
/// `item_id` disambiguates per-track artifacts of the same kind within a group
/// (e.g. multiple `.lrc` files). Group-level artifacts use `item_id: None`.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct ArtifactKey {
    pub operation_id: OperationId,
    pub group_id: Option<GroupId>,
    pub item_id: Option<ItemId>,
    pub kind: ArtifactKind,
}

/// `key` is intentionally absent — it is already the `IndexMap` key in
/// `ProgressState.artifacts`. Callers use `.iter()` to get `(&key, &state)`.
pub struct ArtifactState {
    pub label: String,
    pub phase: DownloadPhase,
}

/// Moved here from active state when `ItemFinished` is received.
pub enum CompletedRecord {
    Item {
        item_id: ItemId,
        group_id: Option<GroupId>,
        label: DisplayLabel,
        quality: Option<QualityBadge>,
        outcome: ItemOutcome,
    },
    Artifact {
        operation_id: OperationId,
        group_id: Option<GroupId>,
        item_id: Option<ItemId>,
        kind: ArtifactKind,
        label: String,
        outcome: ArtifactOutcome,
    },
    Group {
        group_id: GroupId,
        kind: OperationKind,
        label: DisplayLabel,
        outcome: GroupOutcome,
        /// Snapshot of final counts from `GroupState` at finish time, used to
        /// render "Images and Words  8 tracks" without re-walking item state.
        items_done: u64,
        items_total: Option<u64>,
    },
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ProgressError {
    /// Returned when `OperationTotalsUpdated` or `OperationFinished` carries an
    /// ID that does not match the state's single operation.
    #[error("unknown operation `{0:?}`")]
    UnknownOperation(OperationId),
    #[error("group `{0:?}` was already started")]
    DuplicateGroup(GroupId),
    #[error("unknown group `{0:?}`")]
    UnknownGroup(GroupId),
    #[error("item `{0:?}` was already queued")]
    DuplicateItem(ItemId),
    #[error("unknown item `{0:?}`")]
    UnknownItem(ItemId),
    #[error("artifact `{0:?}` is not active")]
    UnknownArtifact(ArtifactKey),
    /// `reason` is a static label (e.g. `"TransferUpdate before ItemQueued"`);
    /// `item_id` carries runtime context so the error is actionable in tests.
    #[error("event order violation for item {item_id:?}: {reason}")]
    InvalidTransition { item_id: ItemId, reason: &'static str },
}
```

`ProgressState` exposes a pure method:

```rust
impl ProgressState {
    pub fn new(config: OperationConfig) -> Self;
    /// Derives a `ProgressSummary` from accumulated completed records.
    /// Called by the CLI after the reducer processes `OperationFinished`.
    /// The CLI measures `elapsed` via `Instant::now()` at construction time.
    pub fn summary(&self, elapsed: Option<time::Duration>) -> ProgressSummary;
}
```

`IndexMap` is from the `indexmap` crate (already widely used in Rust
ecosystems). Add it as a workspace dependency. It preserves insertion order
(needed for consistent row display) while providing `O(1)` lookup by ID
(needed for high-frequency `TransferUpdate` events).

`ProgressState` is scoped to one operation. The CLI creates a new state for
each input URL/resource. If a future command wants a single dashboard for
multiple top-level operations, add an outer coordinator rather than making one
`ProgressState` hold many unrelated operations.

### `riptide-media`

Keep low-level transfer callback support. Do not depend on `riptide-progress`.
The media crate should remain usable in non-riptide contexts.

Current `DownloadProgress` stays local to bytes/segments:

```rust
pub trait DownloadProgress: Send + Sync {
    fn on_segment_start(&self, segment_index: usize, total_segments: usize, content_length: Option<u64>);
    fn on_bytes(&self, bytes_downloaded: u64);
    fn on_segment_complete(&self, segment_index: usize, total_segments: usize);
}
```

The orchestration layer creates an adapter from this callback to
`riptide-progress::DownloadEvent::TransferUpdate`.

### `riptide-metadata`

No progress renderer and no event dependency required. Metadata functions should
stay focused on pure metadata construction and blocking tag I/O.

The caller emits:

- `ItemPhaseChanged { phase: Tagging }` before `write_tags`.
- `ArtifactPhaseChanged { kind: Lyrics, phase: WritingLyrics }` before writing
  lyrics.
- `ArtifactPhaseChanged { kind: Cover, phase: WritingCover }` before writing
  cover sidecars.
- `ArtifactPhaseChanged { kind: Playlist, phase: WritingPlaylist }` before
  writing M3U.
- `ArtifactFinished` after each artifact succeeds, is skipped, or fails.

### `riptide-provider-tidal`

No terminal rendering and no progress renderer dependency.

TIDAL continues to provide:

- catalog entities
- stream variants
- lyrics
- cover URL helpers

The CLI or future orchestration crate converts those into progress events.

### `riptide-cli`

Owns terminal rendering. It depends on `riptide-progress` and `indicatif`.

Candidate modules:

```text
src/progress/mod.rs
src/progress/options.rs
src/progress/symbols.rs
src/progress/colors.rs
src/progress/indicatif.rs
src/progress/plain.rs
```

CLI options:

```text
--progress auto|always|never
--progress-detail title|path|both
--completion-detail auto|groups|tracks
--color auto|always|never
--symbols auto|unicode|ascii|none
```

Defaults:

- `progress = auto`
- `progress_detail = title`
- `completion_detail = auto`
- `color = auto`
- `symbols = auto`

`completion_detail = auto` means:

- single track: print track completion
- album/playlist/mix: print track completion lines
- artist: print group completion lines

The renderer must be built from row state, not from raw provider types.

## Interfaces

### Progress Sink

`ProgressSink` is defined in `riptide-progress` (`src/sink.rs`). Concrete
implementations (`IndicatifProgressSink`, `PlainProgressSink`,
`RecordingProgressSink`) live in `riptide-cli`. This means the orchestration
layer depends only on `riptide-progress`, never on `riptide-cli`.

```rust
// riptide-progress::sink
pub trait ProgressSink: Send + Sync {
    fn emit(&self, event: DownloadEvent);
}
```

Implementations:

- `NoopProgressSink` — in `riptide-progress` (zero-cost stand-in for tests/libs)
- `IndicatifProgressSink` — in `riptide-cli`
- `PlainProgressSink` — in `riptide-cli`
- `RecordingProgressSink` — in `riptide-cli` (or a `#[cfg(test)]` module)

This keeps libraries testable and avoids terminal scraping in tests.

Concurrent calls:

- `ProgressSink::emit` may be called from multiple Tokio tasks.
- Sink implementations must serialize reducer access internally with a mutex or
  channel.
- Ordering is guaranteed only for events emitted by the same item task in
  program order. Cross-item ordering is best-effort and should not be asserted
  in tests except where the orchestrator emits events sequentially before
  spawning tasks.
- `RecordingProgressSink` stores the received event sequence for tests and
  exposes helpers for per-item filtering.

### Media Adapter

The CLI adapter bridges `riptide-media::DownloadProgress` to high-level events:

```rust
pub struct MediaTransferProgress<'a> {
    pub item_id: ItemId,
    pub sink: &'a dyn ProgressSink,
    /// Private: internal accumulator managed by the `DownloadProgress` impl.
    /// Callers have no reason to read or reset it directly.
    bytes_done: AtomicU64,
}

impl riptide_media::DownloadProgress for MediaTransferProgress<'_> {
    fn on_segment_start(...);
    fn on_bytes(...);
    fn on_segment_complete(...);
}
```

For one segment with `Content-Length`, emit `bytes_total = Some(length)`.
For segmented streams without known aggregate bytes, emit `bytes_total = None`
and segment counts.

### Row Model

`riptide-progress` owns render-neutral rows:

```rust
/// Top-level operation row (e.g. the `● artist …` or `● album …` line for a
/// single-album command). `symbol` covers active/done/failed; `kind` provides
/// the text label ("track", "album", "artist", …).
pub struct OverallRow {
    pub kind: OperationKind,
    pub symbol: RowSymbol,
    pub label: String,
    pub groups: Option<CountPair>,
    pub items: Option<CountPair>,
    pub bytes: Option<BytePair>,
}

/// Intermediate group row, rendered indented beneath the overall row for
/// multi-group operations (e.g. the `● album  5/8 tracks  Images and Words`
/// lines shown under an artist operation). Not used for single-group commands.
pub struct GroupRow {
    pub kind: OperationKind,
    pub symbol: RowSymbol,
    pub label: String,
    pub items: Option<CountPair>,
    pub bytes: Option<BytePair>,
}

pub struct ItemRow {
    pub symbol: RowSymbol,
    pub phase: DownloadPhase,
    pub label: String,
    pub quality: Option<QualityBadge>,
    pub transfer: Option<TransferProgress>,
    /// Populated when `ItemOutputResolved` has been received. Renderers expose
    /// this only when `--progress-detail path|both` is active.
    pub output_path: Option<std::path::PathBuf>,
}

/// Active artifact row, e.g. `◇ cover  [  ]  cover.jpg` shown beneath album rows.
/// Built from `ArtifactState` by the row builder.
pub struct ArtifactRow {
    pub symbol: RowSymbol,
    pub kind: ArtifactKind,
    /// Only artifact-relevant phases (`WritingCover`, `WritingLyrics`,
    /// `WritingPlaylist`) should appear here. Renderers must handle unknown
    /// variants via wildcard fallback (required by `#[non_exhaustive]`).
    pub phase: DownloadPhase,
    pub label: String,
}

/// Active rows returned by the row builder: items and artifacts share the same
/// ordered display list beneath the overall row.
#[non_exhaustive]
pub enum ActiveRow {
    Item(ItemRow),
    Artifact(ArtifactRow),
}

/// Row builder signature updated to return `Vec<ActiveRow>` instead of `Vec<ItemRow>`.

/// Mirrors `CompletedRecord` — each variant carries only the fields relevant
/// to its completion kind, preventing invalid kind/outcome combinations that a
/// flat struct would allow.
#[non_exhaustive]
pub enum CompletionRow {
    Item {
        symbol: RowSymbol,
        outcome: ItemOutcome,
        label: String,
        quality: Option<QualityBadge>,
        /// Present when `outcome` is `Failed`; omitted otherwise.
        detail: Option<String>,
    },
    Artifact {
        symbol: RowSymbol,
        kind: ArtifactKind,
        outcome: ArtifactOutcome,
        label: String,
    },
    Group {
        symbol: RowSymbol,
        kind: OperationKind,
        outcome: GroupOutcome,
        label: String,
        /// Item counts snapshot; used to render "Images and Words  8 tracks".
        items: Option<CountPair>,
    },
}

#[non_exhaustive]
pub enum RowSymbol {
    Active,
    Queued,
    Phase,
    Artifact,
    Done,
    Exists,
    Failed,
}

pub struct CountPair {
    pub done: u64,
    pub total: Option<u64>,
    pub unit: CountUnit,
}

#[non_exhaustive]
pub enum CountUnit {
    Files,
    Tracks,
    Videos,
    Items,
    Albums,
    Groups,
}

pub struct BytePair {
    pub done: u64,
    pub total: Option<u64>,
}
```

`detail` is used for errors or artifact filenames. It must not contain song
paths in default title mode.

## Implementation Plan

### Phase 1: Event Model and Pure Formatting

1. Add `crates/riptide-progress` to the workspace.
2. Add `indexmap` to workspace dependencies.
3. Define IDs, phases, operation/item/artifact kinds, quality badges, labels,
   transfer progress, events, outcomes, summaries, and row models.
4. Implement `ProgressIdAllocator`.
5. Implement pure label builders for track, video, album, playlist, artist, and
   generic collection.
6. Implement quality badge derivation from `StreamVariant`.
7. Implement a pure reducer from `DownloadEvent` to `ProgressState`.
8. Implement pure row builders from `ProgressState` and display options.

Tests:

- Track labels render `01 Title` for one-digit track numbers.
- Track labels omit the number when absent.
- Album labels include album artist and title.
- Quality badges map lossless, hi-res, video, provider-specific, and unknown
  cases.
- Reducer counts downloaded, copied, enriched, skipped, and failed outcomes.
- Reducer rejects duplicate IDs and unknown item/group/artifact updates with
  `ProgressError`.
- Late operation and group totals update row counts.
- `ItemQualityResolved` updates queued item quality after stream resolution.
- `ArtifactFinished` produces artifact completion rows.
- Default completion rows omit song paths.
- Artist `completion_detail = auto` returns group rows, not every track.

### Phase 2: CLI Renderer Options

1. Add CLI enums for `--progress-detail`, `--completion-detail`, `--color`, and
   `--symbols`.
2. Implement symbol resolution: auto/unicode/ascii/none.
3. Implement color resolution: auto/always/never.
4. Move current progress UI code from `main.rs` into `progress/indicatif.rs`.
5. Add `plain.rs` for non-interactive output.

Tests:

- CLI parses all new option values.
- `--symbols ascii` uses ASCII status markers.
- `--symbols none` produces rows without marker prefixes.
- `--color never` disables style sequences in plain output.

### Phase 3: Wire Current Download Command to Events

1. Build an operation event before expanding each input.
2. Emit group events for album/playlist/mix/artist groupings where supported.
3. Emit item queued events before spawning each item task.
4. Emit `ItemQualityResolved` after stream resolution.
5. Emit item phase events at existing boundaries:
   - resolving stream
   - resolving media
   - downloading
   - post-processing
   - tagging
   - persisting
   - lyrics
   - cover
6. Replace `TrackProgress` with an adapter that emits transfer events.
7. Emit artifact phase and finish events for cover, lyrics, and M3U.
8. Emit item outcomes for done, exists, skipped, and failed.
9. Emit operation summary at the end.

Tests:

- Recording sink sees events in the expected order for a successful track.
- Existing file emits `Exists` and advances summary without transfer events.
- Failed stream emits `Failed` with context and no false `Done`.
- Stream resolution emits `ItemQualityResolved` before transfer events.
- Cover-disabled config emits no cover artifact events.
- Lyrics-disabled config emits no lyrics artifact events.
- M3U-disabled config emits no playlist artifact events.

### Phase 4: Indicatif Rendering

1. Render one overall row per input operation.
2. Render active item rows underneath.
3. Use title-first item labels by default.
4. Show byte progress and speed during transfer.
5. Show segment progress only when byte total is unavailable.
6. Clear or finish active rows according to the row model.
7. Print completion rows after the live area.
8. Print final summary.

Tests:

- Snapshot tests for single track, album, and artist row models.
- Unit tests for row truncation with fixed terminal widths.
- Integration test with a mock transfer verifies byte progress updates before
  completion.
- Non-TTY mode produces line-oriented plain output.

### Phase 5: Future Backend Hardening

Add tests using fake backends or direct events, not a real provider:

- Local copy backend:
  - emits `Copying` with bytes
  - finishes as `Copied`
  - no stream events required
- Metadata-only backend:
  - emits `Tagging`
  - finishes as `Enriched`
  - no transfer progress required
- No-cover backend:
  - completes without cover events
  - summary does not claim saved covers
- Provider-specific quality:
  - renders the provider label without panicking

This phase ensures the API is not accidentally TIDAL-shaped.

## Performance Requirements

- No string formatting or row rebuilding per chunk. `TransferUpdate` handling
  must update only numeric counters (`bytes_done`, `segments_done`); label and
  quality strings are built once and reused. The event object itself may
  allocate (it is a value type), but no downstream string work may occur.
- The media adapter stores byte counters in atomics; it never formats strings
  or rebuilds row models in its `on_bytes` callback.
- Throttle terminal redraw through `indicatif` draw target; do not force-refresh
  on every byte event.
- Build labels and quality badges once per item after stream resolution.
- Use ordered maps/vectors for user-visible row ordering.
- Keep completed row storage bounded by operation scope. For very large artist
  downloads, default to group summaries rather than retaining every track row.

## Error Handling

- Libraries emit structured outcomes; the CLI decides display text.
- Failed item rows include short error messages, not debug chains.
- Full error chains remain available from the command error path or verbose
  mode.
- If any item fails and `--skip-errors` is not active, the command exits
  non-zero after emitting the failed event.
- If `--skip-errors` is active, the command completes with a summary that
  includes the failed count.

## Config and CLI UX

No config file keys are added in this slice. The new controls are CLI-only at
first to avoid unwired config knobs.

Later, if users want persistent defaults, add a `[progress]` config section in
the same slice that wires it into the runtime.

## Review Checklist

- Event types do not mention TIDAL.
- Renderer code does not live outside `riptide-cli`.
- Provider crates do not depend on `riptide-progress`.
- `riptide-media` remains usable without `riptide-progress`.
- Labels are title-first by default.
- Completed song rows do not show paths by default.
- Two-digit track numbers are covered by tests.
- Quality badges are derived from resolved stream quality.
- Cover, lyrics, and playlist artifacts are optional events.
- Local copy and metadata-only fake flows are covered by tests.
- Non-TTY output is readable and color-free by default.
- `DownloadPhase` has no terminal states; final state lives in `ItemOutcome`.
- `DownloadEvent::TransferUpdate` (not `TransferProgress`) is used in all match arms.
- Public cross-crate enums carry `#[non_exhaustive]`, and renderers include
  wildcard fallback behavior.
- ID newtypes derive `Clone, Copy, PartialEq, Eq, Hash, Debug`.
- `ArtifactKey` and `ArtifactKind` derive `Clone, PartialEq, Eq, Hash, Debug`.
- `DisplayLabel.track_number` is `Option<riptide_core::TrackNumber>`, not `Option<u32>`.
- `TransferProgress.segments_done` is `u64`, not `Option<u64>`.
- `MediaTransferProgress.bytes_done` is private.
- `ProgressState.operation` is `OperationState` (not `Option`); construct via `ProgressState::new(OperationStarted)`.
- `ProgressState` uses `IndexMap` for items, groups, and artifacts.
- `DownloadPhase → RowSymbol` mapping lives in `src/row.rs`, not in `riptide-cli`.
- `OverallRow` has no `phase` field; `symbol` and `kind` cover operation-level status.
- `CompletionRow` is an enum (`Item | Artifact | Group`); `CompletionKind` and `CompletionOutcome` do not exist.
- `ProgressSink` is defined in `riptide-progress`; impls live in `riptide-cli`.
- `ProgressIdAllocator` lives in `src/id.rs` in `riptide-progress`.
- `ProgressError::InvalidTransition` carries an `item_id` for runtime context.
- `ProgressSummary` has separate `failed` (items) and `artifacts_failed` fields.
- Reducers return `Result<(), ProgressError>` and reject invalid event order.
- `OperationConfig` is the constructor input; `OperationStarted` does not exist as a `DownloadEvent` variant.
- `ItemKind` has no `Artifact` variant; artifacts use their own event path.
- `ArtifactState` has no `key` field; key is the `IndexMap` key.
- `GroupRow` exists for intermediate group rows in multi-group operations.
- `serde` is not a dependency of `riptide-progress`.
- Artifact lifecycle has both phase and finish events.
- `ArtifactKey` includes `item_id` to distinguish per-track artifacts of the same kind.
- `ItemOutputResolved` event exists; renderers expose `output_path` only under `--progress-detail path|both`.
- `CompletedRecord::Group` carries `items_done`/`items_total` for count rendering.
- Active row builder returns `Vec<ActiveRow>` with `ActiveRow::Item` and `ActiveRow::Artifact` variants.
- Row builder returns `OverallRow` + `Vec<GroupRow>` + `Vec<ActiveRow>` + `Vec<CompletionRow>`.
- `OperationTotalsUpdated`/`GroupTotalsUpdated`: `None` means "no change", not "clear".
- `OperationFinished` has no `summary` field; summary is derived via `state.summary()`.
- Quality and totals can be updated after start/queue events.
- Sink implementations serialize concurrent event delivery internally.
- `TransferUpdate` handling updates only numeric counters; no string work per chunk.

## Completion Criteria

- `riptide-progress` exists with documented public types.
- Download command uses progress events internally.
- Interactive output matches the approved single track, album, and artist UX
  shapes.
- `--color` and `--symbols` controls work.
- Default completion output omits song paths.
- Tests cover pure formatting, reducers, CLI option parsing, and at least one
  mock download event flow.
- `cargo fmt`, `cargo clippy --workspace --all-targets`, and
  `cargo test --workspace` pass.
