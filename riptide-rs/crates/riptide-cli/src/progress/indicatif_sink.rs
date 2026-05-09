//! Indicatif-based interactive progress sink.
//!
//! Each active item gets one [`ProgressBar`] that transitions in-place from
//! active to completed via `finish_with_message`. Artifact events are silent
//! during download; only the final summary reports them.

use crate::progress::{
    options::{ColorMode, CompletionDetail, ProgressDetail, SymbolsMode},
    symbols::{resolve_symbols, symbol_for, SymbolSet},
};
use indicatif::{HumanBytes, MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};
use riptide_progress::{
    row::{phase_to_symbol, RowSymbol},
    ArtifactKind, DownloadEvent, DownloadPhase, ItemId, ItemOutcome, OperationConfig,
    ProgressSink, ProgressState,
};
use std::{
    collections::HashMap,
    sync::{Mutex, MutexGuard, PoisonError},
    time::Instant,
};

/// Rendering options passed to [`IndicatifSink::new`].
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub struct RenderOpts {
    /// Detail level for active rows.
    pub progress_detail: ProgressDetail,
    /// Detail level for completion output.
    pub completion_detail: CompletionDetail,
    /// Symbol rendering mode.
    pub symbols: SymbolsMode,
    /// Colour rendering mode.
    pub color: ColorMode,
}

impl Default for RenderOpts {
    fn default() -> Self {
        Self {
            progress_detail: ProgressDetail::Title,
            completion_detail: CompletionDetail::Auto,
            symbols: SymbolsMode::Auto,
            color: ColorMode::Auto,
        }
    }
}

/// Interactive indicatif sink.
///
/// Item bars stay in place when they complete (`finish_with_message`). The
/// overall bar also stays. Artifact events are not shown during download;
/// they appear only in the final summary.
pub struct IndicatifSink {
    inner: Mutex<SinkInner>,
    opts: RenderOpts,
}

struct SinkInner {
    state: ProgressState,
    multi: MultiProgress,
    overall_bar: ProgressBar,
    item_bars: HashMap<ItemId, ProgressBar>,
    start: Instant,
}

impl IndicatifSink {
    /// Creates a new indicatif sink for the given operation.
    pub fn new(config: OperationConfig, opts: RenderOpts) -> Self {
        let multi = MultiProgress::with_draw_target(ProgressDrawTarget::stderr_with_hz(10));
        let total = config.total_items.unwrap_or(0);
        let overall_bar = multi.add(ProgressBar::new(total));
        overall_bar.set_style(overall_style());
        overall_bar.set_prefix("overall");
        overall_bar.set_message(format_op_label(&config));
        Self {
            inner: Mutex::new(SinkInner {
                state: ProgressState::new(config),
                multi,
                overall_bar,
                item_bars: HashMap::new(),
                start: Instant::now(),
            }),
            opts,
        }
    }

    fn lock(&self) -> MutexGuard<'_, SinkInner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl ProgressSink for IndicatifSink {
    #[allow(clippy::too_many_lines)]
    fn emit(&self, event: DownloadEvent) {
        let symbols = resolve_symbols(self.opts.symbols);
        let mut inner = self.lock();

        // Visual updates happen BEFORE state reduction so we can still read
        // current item state from inner.state.items.
        match &event {
            // ── Item lifecycle ────────────────────────────────────────────

            DownloadEvent::ItemQueued { item_id, label, .. } => {
                let item_id = *item_id;
                let bar = inner.multi.add(ProgressBar::new(1));
                bar.set_style(item_style());
                let op_kind = &inner.state.operation.kind;
                bar.set_prefix(format!(
                    "{} {:<7}",
                    sym(symbols, RowSymbol::Queued),
                    "queued"
                ));
                bar.set_message(format_item_msg("", "", &item_label(label, op_kind)));
                inner.item_bars.insert(item_id, bar);
            }

            DownloadEvent::ItemQualityResolved { item_id, quality } => {
                let item_id = *item_id;
                if let Some(bar) = inner.item_bars.get(&item_id) {
                    let op_kind = &inner.state.operation.kind;
                    if let Some(item) = inner.state.items.get(&item_id) {
                        let label = item_label(&item.label, op_kind);
                        let badge = badge_str(quality);
                        bar.set_message(format_item_msg("", &badge, &label));
                    }
                }
            }

            DownloadEvent::ItemPhaseChanged { item_id, phase } => {
                let item_id = *item_id;
                if let Some(bar) = inner.item_bars.get(&item_id) {
                    bar.set_prefix(format!(
                        "{} {:<7}",
                        sym(symbols, phase_to_symbol(phase)),
                        phase_label(phase)
                    ));
                    if !matches!(phase, DownloadPhase::Downloading | DownloadPhase::Copying) {
                        bar.set_length(1);
                        bar.set_position(0);
                    }
                }
            }

            DownloadEvent::TransferUpdate { item_id, progress } => {
                let item_id = *item_id;
                if let Some(bar) = inner.item_bars.get(&item_id) {
                    // Update bar position.
                    if let Some(total) = progress.bytes_total {
                        bar.set_length(total);
                        bar.set_position(progress.bytes_done);
                    } else if let Some(total) = progress.segments_total {
                        bar.set_length(total);
                        bar.set_position(progress.segments_done);
                    }
                    // Update message with byte info (segment-boundary only).
                    if let Some(item) = inner.state.items.get(&item_id) {
                        let op_kind = &inner.state.operation.kind;
                        let label = item_label(&item.label, op_kind);
                        let badge = item
                            .quality
                            .as_ref()
                            .map(badge_str)
                            .unwrap_or_default();
                        let bytes = format_bytes(progress.bytes_done, progress.bytes_total);
                        bar.set_message(format_item_msg(&bytes, &badge, &label));
                    }
                }
            }

            DownloadEvent::ItemFinished { item_id, outcome } => {
                let item_id = *item_id;
                if let Some(bar) = inner.item_bars.remove(&item_id) {
                    // Build the final message with quality badge + label.
                    let msg = if let Some(item) = inner.state.items.get(&item_id) {
                        let op_kind = &inner.state.operation.kind;
                        let label = item_label(&item.label, op_kind);
                        let badge = item
                            .quality
                            .as_ref()
                            .map(badge_str)
                            .unwrap_or_default();
                        let detail = match outcome {
                            ItemOutcome::Failed { message } => format!("  {message}"),
                            _ => String::new(),
                        };
                        format_item_msg("", &badge, &format!("{label}{detail}"))
                    } else {
                        String::new()
                    };
                    // Update prefix to done/exists/failed symbol.
                    bar.set_prefix(format!(
                        "{} {:<7}",
                        sym(symbols, outcome_symbol(outcome)),
                        outcome_label(outcome),
                    ));
                    // Fill the bar and keep it visible.
                    bar.set_length(1);
                    bar.set_position(1);
                    bar.finish_with_message(msg);
                }
                inner.overall_bar.inc(1);
            }

            // ── Artifacts: silent during download ─────────────────────────
            // Artifact events are reduced into state but produce no visual
            // output. The summary printed on OperationFinished reports them.
            DownloadEvent::ArtifactPhaseChanged { .. } | DownloadEvent::ArtifactFinished { .. } => {}

            // ── Operation finish ──────────────────────────────────────────
            DownloadEvent::OperationFinished { .. } => {
                let elapsed_secs = inner.start.elapsed().as_secs();
                let elapsed = time::Duration::new(
                    i64::try_from(elapsed_secs).unwrap_or(i64::MAX),
                    0,
                );

                // Reduce first so summary counts are accurate.
                let _ = inner.state.reduce(event.clone());
                let summary = inner.state.summary(Some(elapsed));

                // Finish the overall bar in place (keep it visible).
                inner.overall_bar.finish();

                // Print summary below all the finished bars.
                let total = summary.downloaded + summary.copied + summary.enriched;
                if total > 0 {
                    eprintln!("\nDownloaded {total} track(s) in {elapsed_secs}s");
                }
                for artifact in &summary.artifacts {
                    let k = artifact_kind_label(&artifact.kind);
                    if artifact.count > 1 {
                        eprintln!("Saved {k}: {}", artifact.count);
                    } else if artifact.count == 1 {
                        eprintln!("Saved {k}: {}", artifact.label);
                    }
                }
                if summary.failed > 0 {
                    eprintln!("{} failed", summary.failed);
                }
                // Skip the normal reduce at the bottom — we already reduced.
                return;
            }

            // ── Administrative ────────────────────────────────────────────
            DownloadEvent::OperationTotalsUpdated { total_items, .. } => {
                if let Some(n) = total_items {
                    inner.overall_bar.set_length(*n);
                }
            }

            _ => {}
        }

        let _ = inner.state.reduce(event);
    }
}

// ── Message formatting ────────────────────────────────────────────────────────
//
// Every item bar message has three fixed-width columns:
//   {bytes:>22} {badge:<14} {label}
//
// Bytes is empty for non-transfer phases, keeping the badge/label aligned.

fn format_item_msg(bytes: &str, badge: &str, label: &str) -> String {
    format!("{bytes:>22} {badge:<14} {label}")
}

fn format_bytes(done: u64, total: Option<u64>) -> String {
    match total {
        Some(t) => format!("{}/{}", HumanBytes(done), HumanBytes(t)),
        None => format!("{}", HumanBytes(done)),
    }
}

fn format_op_label(config: &OperationConfig) -> String {
    match &config.label.artist {
        Some(artist) => format!("{artist} - {}", config.label.title),
        None => config.label.title.clone(),
    }
}

fn item_label(
    label: &riptide_progress::DisplayLabel,
    operation_kind: &riptide_progress::OperationKind,
) -> String {
    let num = label
        .track_number
        .map(|n| format!("{:02} ", n.get()))
        .unwrap_or_default();
    let titled = format!("{num}{}", label.title);
    if is_collection_operation(operation_kind) || label.artist.is_none() || label.group.is_some() {
        titled
    } else {
        format!("{} - {titled}", label.artist.as_deref().unwrap_or(""))
    }
}

fn is_collection_operation(kind: &riptide_progress::OperationKind) -> bool {
    use riptide_progress::OperationKind;
    !matches!(kind, OperationKind::Track | OperationKind::Video)
}

fn badge_str(badge: &riptide_progress::QualityBadge) -> String {
    format!("{} {}", badge.symbol_char(), badge.text)
}

fn phase_label(phase: &DownloadPhase) -> &'static str {
    match phase {
        DownloadPhase::Queued => "queued",
        DownloadPhase::ResolvingMetadata
        | DownloadPhase::ResolvingStream
        | DownloadPhase::ResolvingMedia => "resolve",
        DownloadPhase::Downloading => "dl",
        DownloadPhase::Copying => "copy",
        DownloadPhase::PostProcessing => "process",
        DownloadPhase::Tagging => "tag",
        DownloadPhase::Persisting => "persist",
        DownloadPhase::WritingLyrics => "lyrics",
        DownloadPhase::WritingCover => "cover",
        DownloadPhase::WritingPlaylist => "m3u",
        _ => "…",
    }
}

fn outcome_symbol(outcome: &ItemOutcome) -> RowSymbol {
    match outcome {
        ItemOutcome::Downloaded | ItemOutcome::Copied | ItemOutcome::Enriched => RowSymbol::Done,
        ItemOutcome::Exists | ItemOutcome::Skipped { .. } => RowSymbol::Exists,
        ItemOutcome::Failed { .. } => RowSymbol::Failed,
        _ => RowSymbol::Done,
    }
}

fn outcome_label(outcome: &ItemOutcome) -> &'static str {
    match outcome {
        ItemOutcome::Downloaded | ItemOutcome::Copied | ItemOutcome::Enriched => "done",
        ItemOutcome::Exists => "exists",
        ItemOutcome::Skipped { .. } => "skip",
        ItemOutcome::Failed { .. } => "failed",
        _ => "done",
    }
}

fn artifact_kind_label(kind: &ArtifactKind) -> &'static str {
    match kind {
        ArtifactKind::Cover => "cover",
        ArtifactKind::Lyrics => "lyrics",
        ArtifactKind::Playlist => "playlist",
        ArtifactKind::Metadata => "metadata",
        ArtifactKind::Other => "artifact",
        _ => "artifact",
    }
}

fn sym(set: SymbolSet, symbol: RowSymbol) -> String {
    symbol_for(set, symbol).to_owned()
}

/// Truncates a label to `MAX_CHARS` characters, preserving the tail.
#[cfg(test)]
pub(crate) fn truncate_label(label: &str) -> String {
    const MAX_CHARS: usize = 72;
    if label.chars().count() <= MAX_CHARS {
        return label.to_owned();
    }
    let tail: String = label
        .chars()
        .rev()
        .take(MAX_CHARS.saturating_sub(1))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("…{tail}")
}

// ── Indicatif style helpers ──────────────────────────────────────────────────

fn overall_style() -> ProgressStyle {
    ProgressStyle::with_template(
        "{prefix:>12.bold} [{bar:30.cyan/blue}] {pos:>3}/{len:<3}   {msg}",
    )
    .unwrap_or_else(|_| ProgressStyle::default_bar())
}

/// Item bars are indented 2 spaces relative to the overall bar.
fn item_style() -> ProgressStyle {
    ProgressStyle::with_template("  {prefix:>12} [{bar:30.cyan/blue}] {msg}")
        .unwrap_or_else(|_| ProgressStyle::default_bar())
}

// ── QualityBadge display helper ──────────────────────────────────────────────

trait QualityBadgeDisplay {
    fn symbol_char(&self) -> &'static str;
}

impl QualityBadgeDisplay for riptide_progress::QualityBadge {
    fn symbol_char(&self) -> &'static str {
        use riptide_progress::QualitySymbol;
        match self.symbol_kind {
            QualitySymbol::Low => "•",
            QualitySymbol::High => "▪",
            QualitySymbol::Lossless => "◇",
            QualitySymbol::HiRes => "◆",
            QualitySymbol::Video => "▣",
            QualitySymbol::Unknown => "◇",
            _ => "◇",
        }
    }
}
