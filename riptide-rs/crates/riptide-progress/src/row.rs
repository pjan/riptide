//! Render-neutral row models and pure row builders.
//!
//! Row builders take `&ProgressState` and return structured data. They have no
//! I/O and make no formatting decisions (colours, symbols, widths) — those
//! belong in `riptide-cli`.
//!
//! # Hierarchy
//!
//! - [`OverallRow`] — top-level operation row
//! - [`Vec<GroupRow>`] — one per active group (empty for single-group operations)
//! - [`Vec<ActiveRow>`] — items and artifacts interleaved, ordered by insertion
//! - [`Vec<CompletionRow>`] — finished items, artifacts, and groups

use crate::event::{
    ArtifactKind, ArtifactOutcome, DisplayLabel, DownloadPhase, GroupOutcome, ItemOutcome,
    OperationKind, QualityBadge, TransferProgress,
};
use crate::state::{CompletedRecord, ProgressState};
use std::path::PathBuf;

// ── Row types ────────────────────────────────────────────────────────────────

/// Top-level operation row.
///
/// Has no `phase` field — item phases are item-level. `symbol` covers
/// active/done/failed at this level; `kind` provides the text label ("track",
/// "album", "artist", …).
pub struct OverallRow {
    /// Operation kind.
    pub kind: OperationKind,
    /// Status symbol.
    pub symbol: RowSymbol,
    /// Formatted label string.
    pub label: String,
    /// Group progress counts, if applicable.
    pub groups: Option<CountPair>,
    /// Item progress counts.
    pub items: Option<CountPair>,
    /// Byte transfer totals.
    pub bytes: Option<BytePair>,
}

/// Intermediate group row, rendered indented beneath the overall row.
///
/// This is used for multi-group operations (e.g. `● album  5/8 tracks
/// Images and Words` under an artist operation). It is not used for
/// single-group commands.
pub struct GroupRow {
    /// Group kind.
    pub kind: OperationKind,
    /// Status symbol.
    pub symbol: RowSymbol,
    /// Formatted label string.
    pub label: String,
    /// Item progress counts.
    pub items: Option<CountPair>,
    /// Byte transfer totals.
    pub bytes: Option<BytePair>,
}

/// Active item row.
pub struct ItemRow {
    /// Status symbol.
    pub symbol: RowSymbol,
    /// Current phase.
    pub phase: DownloadPhase,
    /// Formatted item label.
    pub label: String,
    /// Quality badge, if resolved.
    pub quality: Option<QualityBadge>,
    /// Transfer snapshot, if in a transfer phase.
    pub transfer: Option<TransferProgress>,
    /// Output path. Populated when `ItemOutputResolved` has been received.
    /// Renderers expose this only under `--progress-detail path|both`.
    pub output_path: Option<PathBuf>,
}

/// Active artifact row.
///
/// Shown beneath album rows, e.g. `◇ cover  [  ]  cover.jpg`.
pub struct ArtifactRow {
    /// Status symbol.
    pub symbol: RowSymbol,
    /// Artifact kind.
    pub kind: ArtifactKind,
    /// Current phase. Only artifact-relevant phases (`Writing*`) are expected;
    /// renderers must handle unknown variants via wildcard fallback (required
    /// by `#[non_exhaustive]` on [`DownloadPhase`]).
    pub phase: DownloadPhase,
    /// Filename or display label.
    pub label: String,
}

/// An active row: either an item or an artifact.
///
/// Items and artifacts are interleaved in the returned `Vec` (ordered by first
/// appearance) so the renderer can display them without merging two lists.
#[non_exhaustive]
pub enum ActiveRow {
    /// A download item row.
    Item(ItemRow),
    /// An artifact row.
    Artifact(ArtifactRow),
}

/// A completion row: item, artifact, or group summary.
///
/// Each variant carries only the fields relevant to its kind, preventing
/// invalid kind/outcome combinations that a flat struct would allow.
#[non_exhaustive]
pub enum CompletionRow {
    /// A finished item.
    Item {
        /// Status symbol.
        symbol: RowSymbol,
        /// Outcome.
        outcome: ItemOutcome,
        /// Formatted label.
        label: String,
        /// Quality badge, if available.
        quality: Option<QualityBadge>,
        /// Error detail (present when `outcome` is `Failed`).
        detail: Option<String>,
    },
    /// A finished artifact.
    Artifact {
        /// Status symbol.
        symbol: RowSymbol,
        /// Artifact kind.
        kind: ArtifactKind,
        /// Outcome.
        outcome: ArtifactOutcome,
        /// Filename or display label.
        label: String,
    },
    /// A finished group.
    Group {
        /// Status symbol.
        symbol: RowSymbol,
        /// Group kind.
        kind: OperationKind,
        /// Outcome.
        outcome: GroupOutcome,
        /// Formatted label.
        label: String,
        /// Item counts snapshot for rendering "Images and Words  8 tracks".
        items: Option<CountPair>,
    },
}

/// Symbol shown on an active or completed row.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowSymbol {
    /// Actively downloading/copying.
    Active,
    /// Waiting in queue.
    Queued,
    /// In a non-transfer phase (tag, persist, resolve…).
    Phase,
    /// Artifact write in progress.
    Artifact,
    /// Completed successfully.
    Done,
    /// Skipped (already exists).
    Exists,
    /// Failed.
    Failed,
}

/// A count shown as `done/total unit`.
pub struct CountPair {
    /// Completed count.
    pub done: u64,
    /// Total count. `None` when not yet known.
    pub total: Option<u64>,
    /// Unit label.
    pub unit: CountUnit,
}

/// Unit label for a [`CountPair`].
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CountUnit {
    /// Generic files.
    Files,
    /// Audio tracks.
    Tracks,
    /// Mixed items.
    Items,
    /// Albums.
    Albums,
    /// Groups.
    Groups,
}

/// Byte progress shown as `done/total`.
pub struct BytePair {
    /// Bytes transferred.
    pub done: u64,
    /// Total expected bytes. `None` when not yet known.
    pub total: Option<u64>,
}

// ── Pure row builders ────────────────────────────────────────────────────────

/// Builds the top-level operation row from current state.
pub fn build_overall_row(state: &ProgressState) -> OverallRow {
    let symbol = if state.is_finished() {
        RowSymbol::Done
    } else {
        RowSymbol::Active
    };

    let label = format_operation_label(&state.operation.label);

    // Count completed items
    let done_items = state
        .completed
        .iter()
        .filter(|r| matches!(r, CompletedRecord::Item { .. }))
        .count() as u64;
    let total_items = state.operation.total_items;
    let unit = operation_count_unit(&state.operation.kind);

    let items = Some(CountPair {
        done: done_items,
        total: total_items,
        unit,
    });

    // Aggregate byte progress from active items
    let bytes = aggregate_bytes(state);

    // Group counts (only meaningful for multi-group operations)
    let groups = if !state.groups.is_empty() || state.operation.total_groups.is_some() {
        let done_groups = state
            .completed
            .iter()
            .filter(|r| matches!(r, CompletedRecord::Group { .. }))
            .count() as u64;
        Some(CountPair {
            done: done_groups,
            total: state.operation.total_groups,
            unit: CountUnit::Albums,
        })
    } else {
        None
    };

    OverallRow {
        kind: state.operation.kind.clone(),
        symbol,
        label,
        groups,
        items,
        bytes,
    }
}

/// Builds one row per active group (empty for single-group operations).
pub fn build_group_rows(state: &ProgressState) -> Vec<GroupRow> {
    state
        .groups
        .values()
        .map(|group| {
            let symbol = RowSymbol::Active;
            let label = format_operation_label(&group.label);
            let unit = operation_count_unit(&group.kind);
            let items = Some(CountPair {
                done: group.items_done,
                total: group.total_items,
                unit,
            });
            GroupRow {
                kind: group.kind.clone(),
                symbol,
                label,
                items,
                bytes: None,
            }
        })
        .collect()
}

/// Builds the active row list: items and artifacts interleaved in insertion order.
///
/// The renderer does not need to merge two separate lists.
pub fn build_active_rows(state: &ProgressState) -> Vec<ActiveRow> {
    // Collect item rows in insertion order.
    let mut rows: Vec<ActiveRow> = state
        .items
        .values()
        .map(|item| {
            let is_in_group = item.group_id.is_some();
            ActiveRow::Item(ItemRow {
                symbol: phase_to_symbol(&item.phase),
                phase: item.phase.clone(),
                label: format_item_label(&item.label, is_in_group),
                quality: item.quality.clone(),
                transfer: item.transfer.clone(),
                output_path: item.output_path.clone(),
            })
        })
        .collect();

    // Append artifact rows in insertion order.
    for (_, artifact) in &state.artifacts {
        rows.push(ActiveRow::Artifact(ArtifactRow {
            symbol: RowSymbol::Artifact,
            kind: artifact_kind_from_phase(&artifact.phase),
            phase: artifact.phase.clone(),
            label: artifact.label.clone(),
        }));
    }

    rows
}

/// Builds completion rows from finished items, artifacts, and groups.
pub fn build_completion_rows(state: &ProgressState) -> Vec<CompletionRow> {
    state
        .completed
        .iter()
        .map(|record| match record {
            CompletedRecord::Item {
                label,
                quality,
                outcome,
                ..
            } => {
                let symbol = outcome_to_symbol(outcome);
                let detail = if let ItemOutcome::Failed { message } = outcome {
                    Some(message.clone())
                } else {
                    None
                };
                CompletionRow::Item {
                    symbol,
                    outcome: outcome.clone(),
                    label: format_item_label(label, false),
                    quality: quality.clone(),
                    detail,
                }
            }
            CompletedRecord::Artifact {
                kind,
                label,
                outcome,
                ..
            } => {
                let symbol = artifact_outcome_to_symbol(outcome);
                CompletionRow::Artifact {
                    symbol,
                    kind: kind.clone(),
                    outcome: outcome.clone(),
                    label: label.clone(),
                }
            }
            CompletedRecord::Group {
                kind,
                label,
                outcome,
                items_done,
                items_total,
                ..
            } => {
                let symbol = group_outcome_to_symbol(outcome);
                let unit = operation_count_unit(kind);
                let items = Some(CountPair {
                    done: *items_done,
                    total: *items_total,
                    unit,
                });
                CompletionRow::Group {
                    symbol,
                    kind: kind.clone(),
                    outcome: outcome.clone(),
                    label: format_operation_label(label),
                    items,
                }
            }
        })
        .collect()
}

// ── Phase/outcome → symbol mapping ──────────────────────────────────────────

/// Maps a [`DownloadPhase`] to the appropriate [`RowSymbol`].
///
/// This mapping lives in `riptide-progress` (pure, no I/O). `riptide-cli` only
/// decides which display character corresponds to a given `RowSymbol`.
pub const fn phase_to_symbol(phase: &DownloadPhase) -> RowSymbol {
    match phase {
        DownloadPhase::Queued => RowSymbol::Queued,
        DownloadPhase::Downloading | DownloadPhase::Copying => RowSymbol::Active,
        DownloadPhase::WritingCover
        | DownloadPhase::WritingLyrics
        | DownloadPhase::WritingPlaylist => RowSymbol::Artifact,
        _ => RowSymbol::Phase,
    }
}

const fn outcome_to_symbol(outcome: &ItemOutcome) -> RowSymbol {
    match outcome {
        ItemOutcome::Downloaded | ItemOutcome::Copied | ItemOutcome::Enriched => RowSymbol::Done,
        ItemOutcome::Exists | ItemOutcome::Skipped { .. } => RowSymbol::Exists,
        ItemOutcome::Failed { .. } => RowSymbol::Failed,
    }
}

const fn artifact_outcome_to_symbol(outcome: &ArtifactOutcome) -> RowSymbol {
    match outcome {
        ArtifactOutcome::Written => RowSymbol::Done,
        ArtifactOutcome::Skipped { .. } => RowSymbol::Exists,
        ArtifactOutcome::Failed { .. } => RowSymbol::Failed,
    }
}

const fn group_outcome_to_symbol(outcome: &GroupOutcome) -> RowSymbol {
    match outcome {
        GroupOutcome::Done => RowSymbol::Done,
        GroupOutcome::Partial { .. } => RowSymbol::Exists,
        GroupOutcome::Failed { .. } => RowSymbol::Failed,
    }
}

const fn artifact_kind_from_phase(phase: &DownloadPhase) -> ArtifactKind {
    match phase {
        DownloadPhase::WritingCover => ArtifactKind::Cover,
        DownloadPhase::WritingLyrics => ArtifactKind::Lyrics,
        DownloadPhase::WritingPlaylist => ArtifactKind::Playlist,
        _ => ArtifactKind::Other,
    }
}

// ── Label formatting helpers ─────────────────────────────────────────────────

/// Formats a [`DisplayLabel`] for an item row.
///
/// The artist prefix is omitted when:
/// - `is_in_group` is `true` (the group row already provides context), **or**
/// - `label.group` is `Some` (the item itself carries album/group context —
///   common for TIDAL tracks that always have an album summary attached).
///
/// Always prefixes a two-digit track number when available.
fn format_item_label(label: &DisplayLabel, is_in_group: bool) -> String {
    let number_prefix = label
        .track_number
        .map(|n| format!("{:02} ", n.get()))
        .unwrap_or_default();
    let titled = format!("{number_prefix}{}", label.title);

    if is_in_group || label.artist.is_none() || label.group.is_some() {
        titled
    } else {
        format!("{} - {titled}", label.artist.as_deref().unwrap_or(""))
    }
}

/// Formats a [`DisplayLabel`] for an operation or group row.
fn format_operation_label(label: &DisplayLabel) -> String {
    label.artist.as_ref().map_or_else(
        || label.title.clone(),
        |artist| format!("{artist} - {}", label.title),
    )
}

// ── Utility helpers ──────────────────────────────────────────────────────────

const fn operation_count_unit(kind: &OperationKind) -> CountUnit {
    match kind {
        OperationKind::Track
        | OperationKind::Album
        | OperationKind::Playlist
        | OperationKind::Mix => CountUnit::Tracks,
        OperationKind::Artist | OperationKind::Collection => CountUnit::Albums,
        OperationKind::LocalTransfer => CountUnit::Files,
        OperationKind::MetadataEnrichment => CountUnit::Items,
    }
}

fn aggregate_bytes(state: &ProgressState) -> Option<BytePair> {
    let mut any = false;
    let mut done = 0u64;
    let mut total: Option<u64> = None;

    for item in state.items.values() {
        if let Some(transfer) = &item.transfer {
            any = true;
            done += transfer.bytes_done;
            if let Some(t) = transfer.bytes_total {
                *total.get_or_insert(0) += t;
            } else {
                total = None; // unknown total invalidates the aggregate
            }
        }
    }

    any.then_some(BytePair { done, total })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::OperationKind;
    use crate::id::ProgressIdAllocator;
    use crate::state::ProgressState;
    use crate::DownloadEvent;
    use crate::OperationConfig;

    #[test]
    fn item_label_with_track_number_in_group() -> Result<(), Box<dyn std::error::Error>> {
        let label = DisplayLabel {
            title: "Pull Me Under".to_owned(),
            artist: Some("Dream Theater".to_owned()),
            group: Some("Images and Words".to_owned()),
            track_number: Some(riptide_core::TrackNumber::new(1)?),
        };
        // In a group: no artist prefix, just "01 Title"
        assert_eq!(format_item_label(&label, true), "01 Pull Me Under");
        Ok(())
    }

    #[test]
    fn item_label_standalone_includes_artist() -> Result<(), Box<dyn std::error::Error>> {
        let label = DisplayLabel {
            title: "Pull Me Under".to_owned(),
            artist: Some("Dream Theater".to_owned()),
            group: None,
            track_number: Some(riptide_core::TrackNumber::new(1)?),
        };
        assert_eq!(
            format_item_label(&label, false),
            "Dream Theater - 01 Pull Me Under"
        );
        Ok(())
    }

    #[test]
    fn item_label_without_number() {
        let label = DisplayLabel {
            title: "Untitled".to_owned(),
            artist: None,
            group: None,
            track_number: None,
        };
        assert_eq!(format_item_label(&label, false), "Untitled");
    }

    #[test]
    fn artist_completion_detail_auto_returns_group_rows() -> Result<(), Box<dyn std::error::Error>>
    {
        let alloc = ProgressIdAllocator::new();
        let config = OperationConfig {
            id: alloc.next_operation_id(),
            kind: OperationKind::Artist,
            label: DisplayLabel {
                title: "Dream Theater".to_owned(),
                artist: None,
                group: None,
                track_number: None,
            },
            total_groups: Some(2),
            total_items: Some(19),
        };
        let op_id = config.id;
        let mut state = ProgressState::new(config);

        let g1 = alloc.next_group_id();
        state.reduce(DownloadEvent::GroupStarted {
            operation_id: op_id,
            group_id: g1,
            kind: OperationKind::Album,
            label: DisplayLabel {
                title: "Images and Words".to_owned(),
                artist: Some("Dream Theater".to_owned()),
                group: None,
                track_number: None,
            },
            total_items: Some(8),
        })?;

        state.reduce(DownloadEvent::GroupFinished {
            group_id: g1,
            outcome: GroupOutcome::Done,
        })?;

        let completion_rows = build_completion_rows(&state);
        assert_eq!(completion_rows.len(), 1);
        assert!(matches!(completion_rows[0], CompletionRow::Group { .. }));
        Ok(())
    }
}
