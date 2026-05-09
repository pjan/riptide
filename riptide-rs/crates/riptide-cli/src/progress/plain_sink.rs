//! Plain-text progress sink for non-interactive terminals.

use riptide_progress::{
    ArtifactOutcome, DownloadEvent, GroupOutcome, ItemOutcome, OperationConfig, ProgressSink,
    ProgressState,
};
use std::sync::Mutex;
use std::time::Instant;

/// Plain-text sink. Prints one line per event to stderr. Used when progress
/// bars are disabled or output is redirected.
pub struct PlainSink {
    inner: Mutex<PlainInner>,
}

struct PlainInner {
    state: ProgressState,
    start: Instant,
}

impl PlainSink {
    /// Creates a new plain sink for the given operation.
    pub fn new(config: OperationConfig) -> Self {
        Self {
            inner: Mutex::new(PlainInner {
                state: ProgressState::new(config),
                start: Instant::now(),
            }),
        }
    }
}

impl ProgressSink for PlainSink {
    fn emit(&self, event: DownloadEvent) {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        // Print human-readable lines for key state transitions.
        match &event {
            DownloadEvent::ItemQueued { label, .. } => {
                eprintln!("queued: {}", label.title);
            }
            DownloadEvent::ItemPhaseChanged { phase, .. } => {
                eprintln!("  {:?}", phase);
            }
            DownloadEvent::ItemFinished { outcome, .. } => match outcome {
                ItemOutcome::Downloaded => eprintln!("  done"),
                ItemOutcome::Copied => eprintln!("  copied"),
                ItemOutcome::Enriched => eprintln!("  enriched"),
                ItemOutcome::Exists => eprintln!("  exists (skipped)"),
                ItemOutcome::Skipped { reason } => eprintln!("  skipped: {reason}"),
                ItemOutcome::Failed { message } => eprintln!("  failed: {message}"),
                _ => {}
            },
            DownloadEvent::ArtifactFinished {
                kind,
                label,
                outcome,
                ..
            } => match outcome {
                ArtifactOutcome::Written => eprintln!("  artifact {kind:?}: {label}"),
                ArtifactOutcome::Failed { message } => {
                    eprintln!("  artifact {kind:?} failed: {message}");
                }
                ArtifactOutcome::Skipped { .. } | _ => {}
            },
            DownloadEvent::GroupFinished { outcome, .. } => match outcome {
                GroupOutcome::Done => eprintln!("group done"),
                GroupOutcome::Partial { failed } => eprintln!("group partial ({failed} failed)"),
                GroupOutcome::Failed { message } => eprintln!("group failed: {message}"),
                _ => {}
            },
            DownloadEvent::OperationFinished { .. } => {
                let elapsed_secs = inner.start.elapsed().as_secs();
                let summary = inner.state.summary(Some(time::Duration::new(
                    i64::try_from(elapsed_secs).unwrap_or(i64::MAX),
                    0,
                )));
                let total = summary.downloaded + summary.copied + summary.enriched;
                eprintln!("\nDownloaded {total} item(s) in {elapsed_secs}s");
            }
            // TransferUpdate and other high-frequency events: no output.
            _ => {}
        }

        let _ = inner.state.reduce(event);
    }
}
