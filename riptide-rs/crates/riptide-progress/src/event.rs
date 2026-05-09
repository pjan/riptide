//! Download event model and supporting types.

use crate::id::{GroupId, ItemId, OperationId};
use std::path::PathBuf;

/// Top-level kind of a download operation.
///
/// Used on both [`OperationConfig`] and [`DownloadEvent::GroupStarted`]: a
/// group inside an artist operation reuses this enum (e.g.
/// `OperationKind::Album` inside `OperationKind::Artist`).
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationKind {
    /// Single track.
    Track,
    /// Single video.
    Video,
    /// Album.
    Album,
    /// Playlist.
    Playlist,
    /// Artist discography.
    Artist,
    /// Provider mix.
    Mix,
    /// Generic collection.
    Collection,
    /// Local file transfer.
    LocalTransfer,
    /// Metadata-only enrichment.
    MetadataEnrichment,
}

/// Kind of a single download item.
///
/// `Artifact` is intentionally absent — artifacts have their own event
/// lifecycle (`ArtifactPhaseChanged` / `ArtifactFinished`) and are never
/// queued via `ItemQueued`.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemKind {
    /// Audio track.
    Track,
    /// Video.
    Video,
    /// Local file.
    LocalFile,
}

/// Kind of a sidecar artifact written alongside a media item.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum ArtifactKind {
    /// Album or track cover art.
    Cover,
    /// Lyrics sidecar (.lrc).
    Lyrics,
    /// M3U playlist file.
    Playlist,
    /// Standalone metadata file.
    Metadata,
    /// Any other artifact.
    Other,
}

/// Phase of a single item download.
///
/// Terminal states (`Done`, `Exists`, `Failed`) are intentionally absent —
/// those are expressed via [`ItemOutcome`] in [`DownloadEvent::ItemFinished`].
/// The renderer transitions from an active row to a completion row when it
/// receives `ItemFinished`, not when it sees a terminal phase.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadPhase {
    /// Waiting to start.
    Queued,
    /// Fetching catalog metadata.
    ResolvingMetadata,
    /// Resolving the stream variant.
    ResolvingStream,
    /// Resolving the manifest (segments/URLs).
    ResolvingMedia,
    /// Downloading remote segments.
    Downloading,
    /// Copying a local file.
    Copying,
    /// Running post-processing (remux, extract).
    PostProcessing,
    /// Writing audio/video tags.
    Tagging,
    /// Moving the processed file to its final path.
    Persisting,
    /// Writing a lyrics sidecar.
    WritingLyrics,
    /// Writing a cover sidecar.
    WritingCover,
    /// Writing an M3U playlist.
    WritingPlaylist,
}

/// Human-readable display label for a media item or group.
///
/// This is identity, not a path. Paths stay out of labels; use
/// [`DownloadEvent::ItemOutputResolved`] for path data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayLabel {
    /// Primary title.
    pub title: String,
    /// Artist name, if available.
    pub artist: Option<String>,
    /// Parent group/album title, if available.
    pub group: Option<String>,
    /// Track number, if available.
    pub track_number: Option<riptide_core::TrackNumber>,
}

/// Compact quality badge displayed alongside a media item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QualityBadge {
    /// Symbol variant (scan aid only; text always present).
    pub symbol_kind: QualitySymbol,
    /// Short text label (e.g. `"Lossless"`, `"Hi-Res"`, `"1080p"`).
    pub text: String,
    /// Optional detail (e.g. `"24/96"` for hi-res with known metrics).
    pub detail: Option<String>,
}

/// Symbol shown in the quality column as a visual scan aid.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QualitySymbol {
    /// Low quality.
    Low,
    /// High (lossy) quality.
    High,
    /// Lossless quality.
    Lossless,
    /// Hi-res lossless quality.
    HiRes,
    /// Video quality.
    Video,
    /// Quality could not be determined.
    Unknown,
}

/// Whether a transfer fetches bytes from a remote stream or copies locally.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferMode {
    /// Remote HTTP segment download.
    RemoteStream,
    /// Local file copy.
    LocalCopy,
}

/// Progress snapshot for a transfer currently in flight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferProgress {
    /// Transfer mode.
    pub mode: TransferMode,
    /// Bytes received so far.
    pub bytes_done: u64,
    /// Total expected bytes, if known.
    pub bytes_total: Option<u64>,
    /// Segments completed. Always known once a segment transfer has started.
    pub segments_done: u64,
    /// Total segment count. `None` for live or unknown manifests.
    pub segments_total: Option<u64>,
}

/// Configuration passed to [`ProgressState::new`].
///
/// This is **not** a [`DownloadEvent`] variant. The lifecycle is:
/// 1. Construct [`ProgressState`] with this config.
/// 2. Feed all subsequent events through [`ProgressState::reduce`].
///
/// Removing `OperationStarted` from the event enum makes it impossible to
/// accidentally "re-start" an operation through a sink.
#[derive(Debug, Clone)]
pub struct OperationConfig {
    /// Unique operation ID from [`crate::ProgressIdAllocator`].
    pub id: OperationId,
    /// Operation kind.
    pub kind: OperationKind,
    /// Human-readable operation label.
    pub label: DisplayLabel,
    /// Total group count, if known upfront.
    pub total_groups: Option<u64>,
    /// Total item count, if known upfront.
    pub total_items: Option<u64>,
}

/// Outcome of a single item download.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemOutcome {
    /// Remote segments downloaded successfully.
    Downloaded,
    /// Local file copied successfully.
    Copied,
    /// Metadata-only enrichment completed.
    Enriched,
    /// File already existed; download skipped.
    Exists,
    /// Deliberately skipped for a non-error reason.
    Skipped {
        /// Human-readable reason.
        reason: String,
    },
    /// Download failed.
    Failed {
        /// Short error message for display.
        message: String,
    },
}

/// Outcome of a single artifact write.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactOutcome {
    /// Artifact written successfully.
    Written,
    /// Artifact write skipped.
    Skipped {
        /// Human-readable reason.
        reason: String,
    },
    /// Artifact write failed.
    Failed {
        /// Short error message for display.
        message: String,
    },
}

/// Outcome of a group of items.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupOutcome {
    /// All items completed successfully.
    Done,
    /// Some items failed.
    Partial {
        /// Number of failed items.
        failed: u64,
    },
    /// The whole group failed.
    Failed {
        /// Short error message.
        message: String,
    },
}

/// Events that flow through [`ProgressState::reduce`].
///
/// [`OperationConfig`] is **not** in this enum — it is the constructor
/// argument for [`ProgressState::new`].
#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum DownloadEvent {
    /// Late total update for an operation.
    ///
    /// `None` means "no change to this field"; it does not clear an existing
    /// total. Totals are monotonically non-decreasing best-known values.
    OperationTotalsUpdated {
        /// Operation whose totals changed.
        id: OperationId,
        /// Updated group count, if changed.
        total_groups: Option<u64>,
        /// Updated item count, if changed.
        total_items: Option<u64>,
    },

    /// A new group started within the operation.
    ///
    /// `kind` reuses [`OperationKind`] — a group is a sub-operation of the
    /// same shape (e.g. `OperationKind::Album` inside an artist operation).
    GroupStarted {
        /// Parent operation.
        operation_id: OperationId,
        /// New group ID.
        group_id: GroupId,
        /// Group kind.
        kind: OperationKind,
        /// Group label.
        label: DisplayLabel,
        /// Total items in this group, if known.
        total_items: Option<u64>,
    },

    /// Late item-count update for a group.
    ///
    /// `None` means "no change"; does not clear an existing total.
    GroupTotalsUpdated {
        /// Group whose total changed.
        group_id: GroupId,
        /// Updated item count, if changed.
        total_items: Option<u64>,
    },

    /// An item has been queued for download.
    ItemQueued {
        /// Parent operation.
        operation_id: OperationId,
        /// Parent group, if any.
        group_id: Option<GroupId>,
        /// Item ID.
        item_id: ItemId,
        /// Item kind.
        kind: ItemKind,
        /// Display label.
        label: DisplayLabel,
        /// Stream quality, if already known. Usually `None` at queue time;
        /// updated later via [`DownloadEvent::ItemQualityResolved`].
        quality: Option<QualityBadge>,
    },

    /// The resolved stream quality is now known.
    ///
    /// Emitted after stream resolution, before the transfer starts.
    ItemQualityResolved {
        /// Item whose quality was resolved.
        item_id: ItemId,
        /// Resolved quality badge.
        quality: QualityBadge,
    },

    /// An item's download phase changed.
    ItemPhaseChanged {
        /// Item whose phase changed.
        item_id: ItemId,
        /// New phase.
        phase: DownloadPhase,
    },

    /// Transfer progress update for an item.
    ///
    /// The reducer updates only numeric counters for this event — no string
    /// formatting or row rebuilding. The renderer reads updated state at its
    /// own throttled cadence.
    TransferUpdate {
        /// Item being transferred.
        item_id: ItemId,
        /// Current transfer snapshot.
        progress: TransferProgress,
    },

    /// An artifact (sidecar file) changed phase.
    ///
    /// The reducer keys artifact state on [`ArtifactKey`], which includes
    /// `item_id` to handle per-track artifacts. A single album group can
    /// produce one `.lrc` per track (all `ArtifactKind::Lyrics`); using only
    /// `(operation_id, group_id, kind)` would collide. Group-level artifacts
    /// (album cover, M3U) use `item_id: None`.
    ArtifactPhaseChanged {
        /// Parent operation.
        operation_id: OperationId,
        /// Parent group, if any.
        group_id: Option<GroupId>,
        /// Parent item for per-track artifacts; `None` for group-level artifacts.
        item_id: Option<ItemId>,
        /// Artifact kind.
        kind: ArtifactKind,
        /// Filename or display label.
        label: String,
        /// New phase. Only artifact-relevant phases (`Writing*`) are expected.
        phase: DownloadPhase,
    },

    /// An artifact write completed.
    ArtifactFinished {
        /// Parent operation.
        operation_id: OperationId,
        /// Parent group, if any.
        group_id: Option<GroupId>,
        /// Parent item for per-track artifacts; `None` for group-level artifacts.
        item_id: Option<ItemId>,
        /// Artifact kind.
        kind: ArtifactKind,
        /// Filename or display label.
        label: String,
        /// Write outcome.
        outcome: ArtifactOutcome,
    },

    /// The output path for an item is now known.
    ///
    /// Emitted after template expansion. Renderers expose `output_path` only
    /// under `--progress-detail path|both`; default title mode ignores it.
    ItemOutputResolved {
        /// Item whose path was resolved.
        item_id: ItemId,
        /// Final output path.
        path: PathBuf,
    },

    /// An item's download completed.
    ItemFinished {
        /// Finished item.
        item_id: ItemId,
        /// Outcome.
        outcome: ItemOutcome,
    },

    /// A group completed.
    GroupFinished {
        /// Finished group.
        group_id: GroupId,
        /// Outcome.
        outcome: GroupOutcome,
    },

    /// The operation completed.
    ///
    /// No `summary` field: the reducer derives [`crate::ProgressSummary`] from
    /// accumulated state. Call [`ProgressState::summary`] after reducing this
    /// event.
    OperationFinished {
        /// The operation that finished.
        id: OperationId,
    },
}
