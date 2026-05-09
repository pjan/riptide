//! Provider-neutral download progress events, state, and row models.
//!
//! # Architecture
//!
//! This crate is a pure library with no I/O. It models download progress as an
//! event stream that reduces into a renderable [`ProgressState`]. Rendering
//! (indicatif, plain text) lives in `riptide-cli`.
//!
//! ## Lifecycle
//!
//! 1. Construct [`ProgressState::new`] with an [`OperationConfig`].
//! 2. Feed [`DownloadEvent`]s through [`ProgressState::reduce`].
//! 3. Build render-neutral rows with the helpers in [`row`].
//! 4. After [`DownloadEvent::OperationFinished`], call [`ProgressState::summary`].

pub mod event;
pub mod id;
pub mod label;
pub mod row;
pub mod sink;
pub mod state;
pub mod summary;

pub use event::{
    ArtifactKind, ArtifactOutcome, DisplayLabel, DownloadEvent, DownloadPhase, GroupOutcome,
    ItemKind, ItemOutcome, OperationConfig, OperationKind, QualityBadge, QualitySymbol,
    TransferMode, TransferProgress,
};
pub use id::{GroupId, ItemId, OperationId, ProgressIdAllocator};
pub use row::{
    ActiveRow, ArtifactRow, BytePair, CompletionRow, CountPair, CountUnit, GroupRow, ItemRow,
    OverallRow, RowSymbol,
};
pub use sink::{NoopProgressSink, ProgressSink};
pub use state::{
    ArtifactKey, ArtifactState, CompletedRecord, GroupState, ItemState, OperationState,
    ProgressError, ProgressState,
};
pub use summary::{ArtifactSummary, ProgressSummary};
