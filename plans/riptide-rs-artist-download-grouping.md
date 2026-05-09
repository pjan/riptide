# riptide-rs Artist Download Grouping Plan

## Status

Revised draft — incorporates review feedback on overengineering, provider
coupling, and state duplication.

## Goal

When downloading an artist URL, the CLI should show album groups under the
overall artist progress row. Tracks should belong to their album group, group
progress should update as tracks finish, and completion output should default to
album summaries for artist downloads.

This is progress/UI wiring only. It should not change downloaded files,
templates, metadata, cover behavior, or M3U ordering.

The design must work across providers through provider-neutral capabilities. The
artist grouping flow must not depend on TIDAL-specific methods.

## Current Behavior

The core progress model already supports groups:

- `DownloadEvent::GroupStarted`, `GroupTotalsUpdated`, and `GroupFinished` in
  `riptide-progress/src/event.rs`
- `GroupState` and `CompletedRecord::Group` in
  `riptide-progress/src/state.rs`
- `build_group_rows` and grouped completion rows in
  `riptide-progress/src/row.rs`

The CLI does not use those events for artist downloads:

- `expand_artist_download_entities` fetches albums, then flattens tracks into
  `ExpansionResult.items`.
- `run_download` sets `OperationConfig.total_groups = None` for every operation.
- `run_download` emits every `ItemQueued` with `group_id: None`.
- `run_scheduled_downloads` only tracks item completion. It does not know which
  album group an item belongs to, so it cannot emit `GroupFinished`.
- `IndicatifSink` creates bars only for items. It ignores group events and does
  not render `build_group_rows` output.
- `RenderOpts.progress_detail` and `RenderOpts.completion_detail` are currently
  carried through but effectively unused in the interactive sink.

## Design

### 1. Preserve Groups In Expansion (Dual-Field Approach)

Keep the existing flat `items` field for non-artist operations. Add a parallel
`groups` field populated only by artist expansion. The two fields are mutually
exclusive — when `groups` is non-empty, `items` is empty and vice versa.

This avoids wrapping every code path (album, playlist, track, collection, mix)
in an `ExpandedGroup` when those paths don't use groups.

```rust
#[derive(Default)]
struct ExpansionResult {
    /// Flat items for non-grouped operations (album, playlist, track, mix).
    items: Vec<(CatalogEntity, Option<Album>)>,
    /// Album groups for artist operations. When non-empty, `items` is empty.
    groups: Vec<ExpandedGroup>,
    summary: DownloadSummary,
}

impl ExpansionResult {
    fn is_grouped(&self) -> bool { !self.groups.is_empty() }

    fn total_items(&self) -> usize {
        self.items.len() + self.groups.iter().map(|g| g.items.len()).sum::<usize>()
    }

    fn assert_valid_shape(&self) {
        debug_assert!(self.items.is_empty() || self.groups.is_empty());
    }
}

struct ExpandedGroup {
    album: Album,
    label: DisplayLabel,
    items: Vec<CatalogEntity>,
}
```

Rules:

- Single track input: populates `items` only, one entry, no groups.
- Album input: populates `items` only, all album tracks with
  `parent_album = Some(album)`.
- Playlist, mix, and collection input: populates `items` only.
- Artist input: populates `groups` only, one `ExpandedGroup` per album.
  `items` stays empty.
- Video input is out of scope. The downloader is audio-only; video references
  should continue to produce no download items and should not create groups.
- Empty albums from artist expansion are omitted from `groups`.
- `DownloadSummary.failed_expansions` behavior stays as-is for album child
  fetch failures.

The `run_download` loop calls `expansion.assert_valid_shape()` and branches
once on `expansion.is_grouped()`:

```rust
expansion.assert_valid_shape();
if expansion.is_grouped() {
    // grouped path: emit GroupStarted per group, queue items with group_id
} else {
    // existing flat item queueing, unchanged
}
```

This means **zero changes** to the album/playlist/track/collection/mix code
paths.

Code pointers:

- `expand_download_entities` function
- `expand_artist_download_entities` function
- `run_download` operation setup and item queueing

### 2. Provider-Neutral Artist Expansion

The current `expand_artist_download_entities` calls
`provider.artist_albums_for_selection()`, which is a method on `TidalProvider`
— not on a provider-neutral capability. That would not work for Apple Music or
future providers.

The fix: use `CollectionProvider::children()` which already handles artists.
`TidalClient::children` for `ResourceKind::Artist` returns
`Vec<CatalogEntity::Album>`. Any provider implementing `CollectionProvider` for
artist references can work the same way.

Use the narrowest capability at this boundary: `&dyn CollectionProvider`, not
`&TidalProvider` and not `&dyn Provider`. The expansion code only needs child
listing, and keeping that dependency narrow makes the design friendlier to
future providers that may not stream or fetch lyrics.

```rust
async fn expand_artist_download_entities(
    provider: &dyn CollectionProvider,
    reference: &MediaRef,
    singles_mode: SinglesMode,
    skip_errors: bool,
) -> Result<ExpansionResult> {
    let albums: Vec<Album> = provider
        .children(reference)
        .await
        .context("failed to fetch artist albums")?
        .into_iter()
        .filter_map(|e| match e {
            CatalogEntity::Album(a) => Some(a),
            _ => None,
        })
        .filter(|a| !should_skip_by_singles_mode(a, singles_mode))
        .collect();

    let mut result = ExpansionResult::default();
    for album in albums {
        let children = match provider.children(&album.id).await {
            Ok(children) => children,
            Err(error) if skip_errors => {
                result.summary.failed_expansions += 1;
                result.summary.failure_messages.push(format!(
                    "failed to fetch album items for {} ({}): {error:#}",
                    album.title,
                    media_ref_label(&album.id)
                ));
                continue;
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "failed to fetch album items for {} ({})",
                        album.title,
                        media_ref_label(&album.id)
                    )
                });
            }
        };
        let tracks: Vec<CatalogEntity> = children
            .into_iter()
            .filter(|e| matches!(e, CatalogEntity::Track(_)))
            .collect();
        if tracks.is_empty() {
            continue;
        }
        result.groups.push(ExpandedGroup {
            label: label_for_album(&album),
            album,
            items: tracks,
        });
    }
    Ok(result)
}
```

This is slightly less efficient for TIDAL than the current
`artist_albums_for_selection` path (which can request only albums or only
singles in a single API call), but it works for any provider. The
`should_skip_by_singles_mode` filter runs client-side after fetching the
provider's artist children and before fetching album tracks, so filtered-out
albums do not trigger child-item requests.

This may introduce extra artist-album listing work for TIDAL compared with its
provider-specific filtered endpoints. That is an accepted tradeoff for this
small provider-neutral fix. A later provider-extension slice can add typed
selection hints without putting TIDAL methods into CLI orchestration.

Release-type filtering keeps the existing `AlbumType::Unknown` semantics:

- `SinglesMode::Include`: include unknown release types.
- `SinglesMode::Exclude`: include unknown release types, because they are not
  known singles or EPs.
- `SinglesMode::Only`: skip unknown release types, because they are not known
  singles or EPs.

Note: `expand_download_entities` should also accept `&dyn CollectionProvider`
instead of `&TidalProvider`. The album/playlist/collection paths already use
`provider.children(reference)`, which is the same trait method.

### 3. Schedule Items With Group Metadata

Extend `ScheduledDownload` and `DownloadTaskFailure` to carry an optional
`GroupId`.

```rust
struct ScheduledDownload {
    index: usize,
    entity: CatalogEntity,
    item_ref: MediaRef,
    parent_album: Option<Album>,
    item_id: ItemId,
    operation_id: OperationId,
    group_id: Option<GroupId>,
    item_label: DisplayLabel,
}

struct DownloadTaskOutput {
    index: usize,
    group_id: Option<GroupId>,
    completion: ItemCompletion,
}

struct DownloadTaskFailure {
    item_id: ItemId,
    group_id: Option<GroupId>,
    item_label: DisplayLabel,
    item_ref: MediaRef,
    error: anyhow::Error,
}
```

`DownloadTaskOutput` needs `group_id` so the success path in
`run_scheduled_downloads` can call
`tracker.record_item_finished(output.group_id, false)`.

Queueing rules in `run_download`:

- For the grouped path (artist operations):

```rust
let total_groups = Some(expansion.groups.len() as u64);
let total_items: u64 = expansion.groups.iter().map(|g| g.items.len() as u64).sum();
```

- Allocate one `GroupId` per `ExpandedGroup` and emit `GroupStarted`:

```rust
for group in &expansion.groups {
    let group_id = alloc.next_group_id();
    sink.emit(DownloadEvent::GroupStarted {
        operation_id: op_id,
        group_id,
        kind: OperationKind::Album,
        label: group.label.clone(),
        total_items: Some(group.items.len() as u64),
    });
    for entity in &group.items {
        // queue with:
        // - group_id = Some(group_id)
        // - parent_album = Some(group.album.clone())
    }
}
```

- For the flat path (non-artist operations): keep `group_id = None` for all
  item events so current album/playlist display is unchanged.
- The grouped path must preserve `parent_album = Some(group.album.clone())` in
  each `ScheduledDownload`. `download_entity` uses this album context for output
  templates, metadata, cover selection, and M3U titles.

### 4. Finish Groups Under Concurrency

Downloads complete out of order, so group completion must be count-based.

Add a small pure tracker in the CLI:

```rust
struct GroupCompletionTracker {
    groups: HashMap<GroupId, GroupCompletionState>,
}

struct GroupCompletionState {
    total: u64,
    finished: u64,
    failed: u64,
}
```

Pure methods:

```rust
impl GroupCompletionTracker {
    fn from_groups(groups: &[(GroupId, usize)]) -> Self;
    fn record_item_finished(
        &mut self,
        group_id: Option<GroupId>,
        failed: bool,
    ) -> Option<(GroupId, GroupOutcome)>;
}
```

Behavior:

- If `group_id` is `None`, return `None`.
- Increment `finished` for every downloaded/skipped/failed item.
- Increment `failed` only for item failures.
- When `finished == total`, return:
  - `GroupOutcome::Done` when `failed == 0`
  - `GroupOutcome::Partial { failed }` when `failed > 0`
- Emit `DownloadEvent::GroupFinished` exactly once per group.

Progress summary semantics must be fixed in the same slice. Today,
`ProgressState::summary()` counts `ItemOutcome::Failed` records and also adds
the `failed` value from `GroupOutcome::Partial`, which would double-count track
failures for grouped artist downloads. The intended semantics are:

- `ProgressSummary.failed` counts failed items only.
- `GroupOutcome::Partial { failed }` is group-status detail for completion rows
  and group bars; it must not increment `ProgressSummary.failed`.
- `ProgressSummary.groups_done` still counts finished groups.

Add a regression test in `riptide-progress` for an item failure inside a partial
group: summary `failed` must be `1`, not `2`.

**Design note:** `ProgressState` independently tracks `GroupState.items_done`
via `reduce()`. This tracker intentionally mirrors that count on the CLI side
because the CLI orchestrator (not the sink) is responsible for emitting
`GroupFinished` events. The sink is behind `Arc<dyn ProgressSink>` and receives
events — it does not emit them back. Keeping emission in the orchestrator
avoids circular event flow.

Integrate into `run_scheduled_downloads` by passing a mutable tracker. On every
item completion (success, skip, or `--skip-errors` failure), call the tracker.
If it returns an outcome, emit `GroupFinished`.

For non-`--skip-errors` failures, aborting the operation is acceptable. No
group finish event is needed because the operation exits with an error.

### 5. Render Active Group Rows In Indicatif

Use a stable two-section layout for artist downloads:

```text
artist [████░░░░░░░░░░░░░░░░] 4/20 tracks Dream Theater

  album [████████░░░░░░░░░░░░] 4/8 tracks Images and Words
  album [░░░░░░░░░░░░░░░░░░░░] 0/12 tracks Awake

  dl    [██████░░░░░░░░░░░░░░] 2.41/8.20 MiB ◇ 01 Pull Me Under
  tag                                      ◇ 03 Take the Time
```

Album rows are persistent summaries. Active track rows are a separate work
queue below the album summaries; they are not nested under album rows in the
first implementation. This stays compact and stable under concurrent downloads.

Add group bars to `IndicatifSink`.

Proposed addition to sink state:

```rust
struct SinkInner {
    ...
    group_bars: HashMap<GroupId, ProgressBar>,
    /// Last persistent album-summary bar.
    last_group_bar: Option<ProgressBar>,
    /// Last active work-queue item bar.
    last_item_bar: Option<ProgressBar>,
}
```

Event handling:

- On `GroupStarted`: create a persistent album-summary bar using
  `multi.insert_after(anchor, bar)` where `anchor` is the previous group bar (or
  the overall bar for the first group). Set its length to
  `total_items.unwrap_or(0)`. Start in a queued/dim state with `0/N` count.
  Track the bar in `last_group_bar` so subsequent group bars remain together
  directly under the overall row.

- All group bars are created eagerly at operation start (not lazily on first
  download). This gives the user an immediate overview of scope — e.g. all 5
  album rows appear immediately for an artist with 5 albums, even before any
  downloads begin.

- Item bars form a separate active work queue below all group bars. Insert the
  first item bar after `last_group_bar.unwrap_or(overall_bar.clone())`; insert
  later item bars after `last_item_bar` to keep active rows stable. Completed
  item bars may finish and disappear/settle according to current item behavior,
  while album rows remain visible and update counts.

- For grouped artist downloads, `ItemQueued` should update `ProgressState` but
  should **not** create an item progress bar. Otherwise every track in the
  artist catalog appears immediately and the active queue is no longer compact.
  Create the item bar lazily when the item first starts work, using the first
  `ItemPhaseChanged` event for that `item_id`. Flat non-grouped operations keep
  the existing eager `ItemQueued` bar behavior.

- Prefix should be a group/album phase prefix, for example `• album` or
  `▸ album`, dim while active.

- Message should use the group label and a count column:

```text
artist [██████░░░░░░░░░░░░░░░░] 12/45 tracks Dream Theater
  album [████████░░░░░░░░░░░░░] 4/8 tracks Images and Words
  album [███░░░░░░░░░░░░░░░░░░] 2/11 tracks Awake
```

- On `ItemFinished`: after reducing state, if the item has a `group_id`, update
  the parent group bar position using the group's `items_done`.

- On `GroupFinished`: finish the group bar in place with a stable status
  message. Include compact outcome detail when useful: `done`, `7 ok, 1 failed`,
  or `failed: <message>`. Group rows remain visible during the operation; this
  is the persistent album summary.

Important ordering detail:

Currently `IndicatifSink::emit` handles many events before calling
`inner.state.reduce(event)`. For group count updates, reduce first for events
that need derived state (`ItemFinished`, `GroupFinished`) and then read
`inner.state.groups` or `inner.state.completed`. This keeps rendering aligned
with the pure state machine. In particular, `GroupFinished` removes the group
from `state.groups` and appends a `CompletedRecord::Group`, so rendering that
event must read the just-added completed group record, not `state.groups`.

Code pointers:

- `IndicatifSink::emit` method
- `build_group_rows` function in `riptide-progress/src/row.rs`
- `GroupState` in `riptide-progress/src/state.rs`

### 6. Completion Detail Behavior

Wire `RenderOpts.completion_detail` for interactive output.

Rules:

- `CompletionDetail::Auto`:
  - `OperationKind::Artist`: print group summaries, not every track.
  - Other operation kinds: keep current track/item completion behavior.
- `CompletionDetail::Groups`: print only group summaries when groups exist.
- `CompletionDetail::Tracks`: print item completion rows.

This can be implemented in `IndicatifSink` by storing `completion_detail` in
`SinkInner` and choosing which completion rows are printed on
`OperationFinished`.

Completion detail is in scope for this implementation. Keep runtime behavior
simple: active item bars are the work queue, group bars are persistent album
summaries, and `OperationFinished` uses `completion_detail` to decide which
completion rows to print. Do not print every completed track again when `Auto`
selects group summaries.

Code pointers:

- `RenderOpts` in `IndicatifSink`
- `CompletionDetail` in `progress/options.rs`
- `build_completion_rows` function in `riptide-progress/src/row.rs`

### 7. Plain Sink Behavior

The plain sink already prints `GroupFinished`, but does not print
`GroupStarted`. Add lightweight output for grouped artist downloads:

```text
group: Images and Words (8 tracks)
queued: Pull Me Under
...
group done
```

Do not print high-frequency transfer updates.

Code pointer: `progress/plain_sink.rs`

## Testing Plan

### Pure CLI Unit Tests

Add tests near the existing CLI tests.

1. Artist expansion preserves album groups.

Use synthetic `Album` and `CatalogEntity::Track` values where possible. If the
provider dependency makes this hard, test a pure helper that converts album +
children into `ExpandedGroup`.

Expected:

- two albums produce two groups
- each group's `items.len()` equals that album's track count
- group labels use album labels
- `ExpansionResult.items` is empty when `groups` is non-empty

2. Group completion tracker handles out-of-order success.

```rust
let g1 = alloc.next_group_id();
let g2 = alloc.next_group_id();
let mut tracker = GroupCompletionTracker::from_groups(&[(g1, 2), (g2, 1)]);
assert_eq!(tracker.record_item_finished(Some(g1), false), None);
assert_eq!(tracker.record_item_finished(Some(g2), false), Some((g2, GroupOutcome::Done)));
assert_eq!(tracker.record_item_finished(Some(g1), false), Some((g1, GroupOutcome::Done)));
```

3. Group completion tracker handles partial failure.

```rust
let mut tracker = GroupCompletionTracker::from_groups(&[(g1, 2)]);
assert_eq!(tracker.record_item_finished(Some(g1), true), None);
assert_eq!(tracker.record_item_finished(Some(g1), false), Some((g1, GroupOutcome::Partial { failed: 1 })));
```

4. Non-artist operations do not emit nested groups.

Test that `expand_download_entities` for an album input returns non-empty
`items` and empty `groups`.

### Progress Library Tests

Existing `riptide-progress` tests already cover group rows and group completion
rows. Add only if the CLI requires new pure helpers in `riptide-progress`.

Add one summary regression test:

- one grouped item finishes with `ItemOutcome::Failed`
- the group finishes with `GroupOutcome::Partial { failed: 1 }`
- `ProgressState::summary().failed == 1`

This prevents double-counting failures from both item and group records.

### Indicatif Sink Tests

Avoid brittle terminal snapshot tests. Add a narrow unit test around any helper
introduced to decide completion mode:

```rust
fn completion_rows_for_mode(kind: OperationKind, mode: CompletionDetail, rows: &[CompletionRow]) -> Vec<...>
```

Expected:

- `Auto + Artist` chooses group rows
- `Auto + Album` chooses item rows
- `Groups` chooses group rows
- `Tracks` chooses item rows

### Manual Verification

Use an artist URL with at least two albums:

```bash
nix develop -c cargo run -- download 'https://tidal.com/artist/<id>' --path /tmp/riptide-test --progress always
```

Expected active output shape:

```text
artist [████░░░░░░░░░░░░░░░░] 4/20 tracks Dream Theater

  album [████████░░░░░░░░░░░░] 4/8 tracks Images and Words
  album [░░░░░░░░░░░░░░░░░░░░] 0/12 tracks Awake

  dl    [██████░░░░░░░░░░░░░░] 2.41/8.20 MiB ◇ 01 Pull Me Under
  dl    [███░░░░░░░░░░░░░░░░░] 1.20/7.10 MiB ◇ 03 Take the Time
```

Expected completion with default `--completion-detail auto`:

```text
Downloaded 20/20 tracks in 42s
Albums:
  done     8/8 tracks   Images and Words
  done    12/12 tracks  Awake
Saved cover: 2
Saved playlist: riptide.m3u
```

Exact symbols/colors may differ by terminal and selected symbol mode.

## Edge Cases

- Albums with zero downloadable tracks should not create empty active group bars.
- Video references remain audio-only no-ops: no download item, no group, no
  stream resolution.
- If `--skip-errors` is enabled and a track fails, the album group should finish
  as `Partial { failed }` once all other tracks in that album complete.
- If `--skip-errors` is disabled, aborting on the first failure is acceptable;
  no attempt is needed to finalize all active groups.
- Concurrent track downloads from the same album may finish out of order; group
  progress must remain count-based.
- M3U ordering must continue to use the existing global `index` assignment and
  must not be derived from completion order.
- Cover sidecar writes remain album-directory based and should not be duplicated
  by grouping changes.

## Implementation Order

1. Change `expand_download_entities` and `expand_artist_download_entities` to
   accept `&dyn CollectionProvider` instead of `&TidalProvider`.
2. Add `ExpandedGroup` and the dual-field `ExpansionResult` (items + groups),
   including `is_grouped`, `total_items`, and `assert_valid_shape` helpers.
3. Refactor `expand_artist_download_entities` to use
   `CollectionProvider::children` and populate `groups`.
4. Add `GroupCompletionTracker` with focused unit tests.
5. Update `ScheduledDownload`, `DownloadTaskFailure`, `run_download`,
   `run_scheduled_downloads`, and `fill_download_queue` to carry group IDs and
   album context, and emit group lifecycle events. Add the `groups.is_empty()`
   branch in `run_download`.
6. Fix `ProgressState::summary()` so group partial failures do not double-count
   item failures, with a regression test.
7. Add group bar storage, eager creation on `GroupStarted`, lazy grouped item
   bar creation on first `ItemPhaseChanged`, active work-queue item insertion
   below group summaries, and group updates on `ItemFinished` / `GroupFinished`
   to `IndicatifSink`.
8. Wire `CompletionDetail` selection for interactive completion output.
9. Add plain sink `GroupStarted` output.
10. Run:

```bash
nix develop -c cargo fmt --check --all
nix develop -c cargo clippy --workspace -- -D warnings
nix develop -c cargo test --workspace
```

## Review Checklist

- Artist operation has `total_groups = Some(album_count)`.
- Artist tracks are queued with `group_id = Some(_)`.
- Artist tracks preserve `parent_album = Some(album)` during scheduling.
- Album, playlist, mix, and single-track downloads keep current user-visible
  behavior — the flat items path is unchanged.
- Video references remain excluded from download planning.
- Group completion is correct under concurrent out-of-order finishes.
- Failed items under `--skip-errors` count toward group completion.
- Partial group failures do not double-count item failures in
  `ProgressSummary.failed`.
- Unknown release types preserve current singles-mode behavior: included for
  `include` and `exclude`, skipped for `only`.
- The interactive renderer shows album rows during artist downloads.
- Group bars are created eagerly on `GroupStarted` (queued/dim state).
- Active track bars render as a separate work queue below album summaries, not
  nested between album rows.
- Grouped artist `ItemQueued` events do not create visible item bars; grouped
  item bars appear only when work starts.
- Default artist completion output prefers album groups over all track rows.
- `CompletionDetail::Auto`, `Groups`, and `Tracks` are wired for interactive
  completion output.
- Artist expansion uses provider-neutral `CollectionProvider`; any TIDAL extra
  artist listing work is accepted and documented.
- No M3U ordering regression.
- `expand_download_entities` and `expand_artist_download_entities` accept
  `&dyn CollectionProvider`, not `&TidalProvider`.
