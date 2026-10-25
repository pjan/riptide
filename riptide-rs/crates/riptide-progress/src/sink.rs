//! Progress sink trait and no-op implementation.

use crate::event::DownloadEvent;

/// Receives [`DownloadEvent`]s emitted during a download operation.
///
/// Implementations are defined in `riptide-cli` (indicatif, plain, recording).
/// [`NoopProgressSink`] lives here as a zero-cost stand-in for tests and
/// library callers that do not need rendering.
///
/// `emit` may be called concurrently from multiple Tokio tasks. Implementations
/// must serialize state access internally (typically via a `Mutex` or channel).
/// Ordering is guaranteed only for events emitted by the same task in program
/// order; cross-task ordering is best-effort.
pub trait ProgressSink: Send + Sync {
    /// Emit a single progress event.
    fn emit(&self, event: DownloadEvent);
}

/// No-op sink that discards all events.
pub struct NoopProgressSink;

impl ProgressSink for NoopProgressSink {
    fn emit(&self, _event: DownloadEvent) {}
}
