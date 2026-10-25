//! Completion summary types.

use crate::event::ArtifactKind;

/// Per-artifact-kind count for the final summary.
#[derive(Debug, Clone)]
pub struct ArtifactSummary {
    /// Artifact kind.
    pub kind: ArtifactKind,
    /// Human-readable aggregate label (e.g. `"5 album covers"`, `"riptide.m3u"`).
    pub label: String,
    /// Number of artifacts of this kind that were written.
    pub count: u64,
}

/// Aggregated completion summary for an operation.
#[derive(Debug, Clone, Default)]
pub struct ProgressSummary {
    /// Items downloaded from a remote stream.
    pub downloaded: u64,
    /// Items copied locally.
    pub copied: u64,
    /// Items enriched with metadata only.
    pub enriched: u64,
    /// Items skipped because they already existed or were explicitly skipped.
    pub skipped_existing: u64,
    /// Items (tracks, local files) that failed.
    pub failed: u64,
    /// Artifact writes (cover, lyrics, playlist) that failed. Kept separate so
    /// the CLI can report "3 tracks failed, 1 cover failed" without ambiguity.
    pub artifacts_failed: u64,
    /// Number of groups that finished.
    pub groups_done: u64,
    /// Total group count. `None` only for operations aborted before totals were
    /// announced; always `Some` for a normally completed operation.
    pub groups_total: Option<u64>,
    /// Total item count. Same rationale as `groups_total`.
    pub items_total: Option<u64>,
    /// Wall-clock duration of the operation, measured by the caller.
    pub elapsed: Option<time::Duration>,
    /// Per-kind artifact counts for written artifacts.
    pub artifacts: Vec<ArtifactSummary>,
    /// Total bytes transferred across all finished items.
    pub total_bytes: u64,
    /// Display labels (titles) of items that finished with `Failed`.
    pub failed_labels: Vec<String>,
}
