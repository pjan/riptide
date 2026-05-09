//! Media transfer progress adapter.
//!
//! Bridges [`riptide_media::DownloadProgress`] (segment-level callbacks) to
//! [`riptide_progress::DownloadEvent::TransferUpdate`] events.
//!
//! ## Performance contract
//!
//! `on_bytes` updates atomic counters **only** — no string formatting, no row
//! rebuilding, no mutex acquisition. The event is emitted on segment boundaries
//! (`on_segment_start`, `on_segment_complete`) where the frequency is low.
//! Byte-level numeric state is read by the sink at its own throttled cadence.

use riptide_media::DownloadProgress;
use riptide_progress::{DownloadEvent, ItemId, ProgressSink, TransferMode, TransferProgress};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

/// Adapts segment-level callbacks into [`ProgressSink`] transfer events.
pub struct MediaTransferProgress {
    /// ID of the item being transferred.
    pub item_id: ItemId,
    /// Sink to emit events to.
    pub sink: Arc<dyn ProgressSink>,
    /// Running byte total (accumulated across all segments).
    bytes_done: AtomicU64,
    /// Known total bytes (set on single-segment Content-Length).
    bytes_total: AtomicU64,
    /// Segments completed so far.
    segments_done: AtomicU64,
    /// Known total segment count.
    segments_total: AtomicU64,
}

impl MediaTransferProgress {
    /// Creates a new adapter.
    pub fn new(item_id: ItemId, sink: Arc<dyn ProgressSink>) -> Self {
        Self {
            item_id,
            sink,
            bytes_done: AtomicU64::new(0),
            bytes_total: AtomicU64::new(0),
            segments_done: AtomicU64::new(0),
            segments_total: AtomicU64::new(0),
        }
    }

    fn emit_update(&self) {
        let bytes_done = self.bytes_done.load(Ordering::Relaxed);
        let bytes_total_raw = self.bytes_total.load(Ordering::Relaxed);
        let segments_done = self.segments_done.load(Ordering::Relaxed);
        let segments_total_raw = self.segments_total.load(Ordering::Relaxed);

        self.sink.emit(DownloadEvent::TransferUpdate {
            item_id: self.item_id,
            progress: TransferProgress {
                mode: TransferMode::RemoteStream,
                bytes_done,
                bytes_total: (bytes_total_raw > 0).then_some(bytes_total_raw),
                segments_done,
                segments_total: (segments_total_raw > 0).then_some(segments_total_raw),
            },
        });
    }
}

impl DownloadProgress for MediaTransferProgress {
    fn on_segment_start(
        &self,
        segment_index: usize,
        total_segments: usize,
        content_length: Option<u64>,
    ) {
        let total = u64::try_from(total_segments).unwrap_or(u64::MAX);
        self.segments_total.store(total, Ordering::Relaxed);

        // For a single-segment download with a known Content-Length, store the
        // byte total so the bar can show meaningful byte progress.
        if total_segments == 1 {
            self.bytes_total
                .store(content_length.unwrap_or(0), Ordering::Relaxed);
        }

        let current_segment = u64::try_from(segment_index).unwrap_or(0);
        self.segments_done.store(current_segment, Ordering::Relaxed);

        self.emit_update();
    }

    /// Updates byte counter atomically. No string work, no mutex.
    fn on_bytes(&self, bytes_downloaded: u64) {
        self.bytes_done
            .fetch_add(bytes_downloaded, Ordering::Relaxed);
        // No event emission here — the renderer reads counters at its own pace.
    }

    fn on_segment_complete(&self, segment_index: usize, _total_segments: usize) {
        let done = u64::try_from(segment_index.saturating_add(1)).unwrap_or(u64::MAX);
        self.segments_done.store(done, Ordering::Relaxed);
        self.emit_update();
    }
}
