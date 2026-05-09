//! Indicatif-based interactive progress sink.

use crate::progress::{
    colors::resolve_color,
    options::{ColorMode, CompletionDetail, ProgressDetail, SymbolsMode},
    symbols::{resolve_symbols, symbol_for, SymbolSet},
};
use indicatif::{HumanBytes, MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};
use riptide_progress::{
    row::{phase_to_symbol, RowSymbol},
    ArtifactKind, ArtifactOutcome, DownloadEvent, DownloadPhase, ItemId, ItemOutcome,
    OperationConfig, ProgressSink, ProgressState,
};
use std::{
    collections::HashMap,
    sync::{Mutex, MutexGuard, PoisonError},
    time::Instant,
};

/// Rendering options passed to [`IndicatifSink::new`].
// Fields are read when --progress-detail and --completion-detail rendering
// paths are implemented (Phase 4 completion work).
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
/// Each active item gets one `ProgressBar`. When `ItemFinished` arrives the
/// bar is cleared and a plain completion line is printed immediately via the
/// `MultiProgress`. Artifact events print plain lines; no bars are created for
/// artifacts. `OperationFinished` prints the summary and clears the overall bar.
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
        let multi =
            MultiProgress::with_draw_target(ProgressDrawTarget::stderr_with_hz(10));
        let total = config.total_items.unwrap_or(0);
        let overall_bar = multi.add(ProgressBar::new(total));
        overall_bar.set_style(overall_style());
        overall_bar.set_message(format_op_label(&config));
        overall_bar.set_prefix("overall");
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
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

impl ProgressSink for IndicatifSink {
    #[allow(clippy::too_many_lines)]
    fn emit(&self, event: DownloadEvent) {
        let symbols = resolve_symbols(self.opts.symbols);
        let _color = resolve_color(self.opts.color);
        let mut inner = self.lock();

        // Visual updates happen BEFORE state reduction so we can read the
        // current item/artifact state from inner.state when needed.
        match &event {
            // ── Item lifecycle ────────────────────────────────────────────
            DownloadEvent::ItemQueued {
                item_id,
                label,
                ..
            } => {
                let item_id = *item_id;
                let bar = inner.multi.add(ProgressBar::new(1));
                bar.set_style(item_style());
                bar.set_prefix(format!(
                    "{} queued ",
                    sym(symbols, RowSymbol::Queued)
                ));
                let op_kind = &inner.state.operation.kind;
                bar.set_message(item_label_str(label, op_kind));
                inner.item_bars.insert(item_id, bar);
            }

            DownloadEvent::ItemQualityResolved { item_id, quality } => {
                let item_id = *item_id;
                if let Some(bar) = inner.item_bars.get(&item_id) {
                    // Read the item label from current state (already there
                    // because ItemQueued was reduced earlier).
                    let op_kind = &inner.state.operation.kind;
                    let label_str = inner
                        .state
                        .items
                        .get(&item_id)
                        .map(|s| item_label_str(&s.label, op_kind))
                        .unwrap_or_default();
                    bar.set_message(format!(
                        "{:<14} {label_str}",
                        badge_str(quality)
                    ));
                }
            }

            DownloadEvent::ItemPhaseChanged { item_id, phase } => {
                let item_id = *item_id;
                if let Some(bar) = inner.item_bars.get(&item_id) {
                    bar.set_prefix(format!(
                        "{} {:<8}",
                        sym(symbols, phase_to_symbol(phase)),
                        phase_label(phase)
                    ));
                    // For non-transfer phases reset bar progress.
                    if !matches!(phase, DownloadPhase::Downloading | DownloadPhase::Copying) {
                        bar.set_length(1);
                        bar.set_position(0);
                    }
                }
            }

            // TransferUpdate is emitted at segment boundaries only (not per byte).
            // We update both bar position and the message byte display here.
            DownloadEvent::TransferUpdate { item_id, progress } => {
                let item_id = *item_id;
                if let Some(bar) = inner.item_bars.get(&item_id) {
                    // Update bar position (numeric — no string work).
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
                        let label_str = item_label_str(&item.label, op_kind);
                        let badge = item
                            .quality
                            .as_ref()
                            .map(badge_str)
                            .unwrap_or_default();
                        let bytes_str = if let Some(total) = progress.bytes_total {
                            format!(
                                "{}/{}",
                                HumanBytes(progress.bytes_done),
                                HumanBytes(total)
                            )
                        } else {
                            format!("{}", HumanBytes(progress.bytes_done))
                        };
                        bar.set_message(format!("{bytes_str:>22} {badge:<14} {label_str}"));
                    }
                }
            }

            DownloadEvent::ItemFinished { item_id, outcome } => {
                let item_id = *item_id;
                if let Some(bar) = inner.item_bars.remove(&item_id) {
                    // Clear the live bar so it doesn't linger.
                    bar.finish_and_clear();
                }
                // Print a plain completion line immediately.
                if let Some(item) = inner.state.items.get(&item_id) {
                    let s = sym(symbols, outcome_symbol(outcome));
                    let op_kind = &inner.state.operation.kind;
                    let label_str = item_label_str(&item.label, op_kind);
                    let badge = item
                        .quality
                        .as_ref()
                        .map(badge_str)
                        .unwrap_or_default();
                    let detail = if let ItemOutcome::Failed { message } = outcome {
                        format!("  {message}")
                    } else {
                        String::new()
                    };
                    inner
                        .multi
                        .println(format!("{s} {badge:<14} {label_str}{detail}"))
                        .ok();
                }
                inner.overall_bar.inc(1);
            }

            // ── Artifact lifecycle ────────────────────────────────────────
            // No progress bars for artifacts — just print a line on finish.
            DownloadEvent::ArtifactFinished {
                kind,
                label,
                outcome,
                ..
            } => {
                match outcome {
                    ArtifactOutcome::Written => {
                        let s = sym(symbols, RowSymbol::Done);
                        let k = artifact_kind_label(kind);
                        inner.multi.println(format!("{s} {k:<10}{label}")).ok();
                    }
                    ArtifactOutcome::Failed { message } => {
                        let s = sym(symbols, RowSymbol::Failed);
                        let k = artifact_kind_label(kind);
                        inner
                            .multi
                            .println(format!("{s} {k:<10}failed: {message}"))
                            .ok();
                    }
                    _ => {}
                }
            }

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

                inner.overall_bar.finish_and_clear();

                let total = summary.downloaded + summary.copied + summary.enriched;
                if total > 0 {
                    inner
                        .multi
                        .println(format!("\nDownloaded {total} track(s) in {elapsed_secs}s"))
                        .ok();
                }
                for artifact in &summary.artifacts {
                    let k = artifact_kind_label(&artifact.kind);
                    if artifact.count > 1 {
                        inner
                            .multi
                            .println(format!("Saved {k}: {}", artifact.count))
                            .ok();
                    } else if artifact.count == 1 {
                        inner
                            .multi
                            .println(format!("Saved {k}: {}", artifact.label))
                            .ok();
                    }
                }
                if summary.failed > 0 {
                    inner
                        .multi
                        .println(format!("{} failed", summary.failed))
                        .ok();
                }
                // Skip normal reduce at the bottom — we already reduced above.
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

// ── Formatting helpers ────────────────────────────────────────────────────────

fn format_op_label(config: &OperationConfig) -> String {
    match &config.label.artist {
        Some(artist) => format!("{artist} - {}", config.label.title),
        None => config.label.title.clone(),
    }
}

/// Formats an item label. Suppresses the artist prefix when:
/// - The item is under a collection operation (album, playlist, artist, …)
///   so the overall row already provides context, **or**
/// - The item carries album/group context itself (`label.group.is_some()`).
///
/// The artist prefix is only shown for standalone single-track/video downloads.
fn item_label_str(
    label: &riptide_progress::DisplayLabel,
    operation_kind: &riptide_progress::OperationKind,
) -> String {
    let num = label
        .track_number
        .map(|n| format!("{:02} ", n.get()))
        .unwrap_or_default();
    let titled = format!("{num}{}", label.title);

    let is_collection = is_collection_operation(operation_kind);
    if is_collection || label.artist.is_none() || label.group.is_some() {
        titled
    } else {
        format!("{} - {titled}", label.artist.as_deref().unwrap_or(""))
    }
}

/// Returns `true` for operation kinds where the overall row already provides
/// enough context that individual items don't need an artist prefix.
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
        DownloadPhase::ResolvingMetadata | DownloadPhase::ResolvingStream
        | DownloadPhase::ResolvingMedia => "resolve",
        DownloadPhase::Downloading => "dl",
        DownloadPhase::Copying => "copy",
        DownloadPhase::PostProcessing => "process",
        DownloadPhase::Tagging => "tag",
        DownloadPhase::Persisting => "persist",
        DownloadPhase::WritingLyrics => "lyrics",
        DownloadPhase::WritingCover => "cover",
        DownloadPhase::WritingPlaylist => "playlist",
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

// ── indicatif style helpers ────────────────────────────────────────────────

fn overall_style() -> ProgressStyle {
    ProgressStyle::with_template(
        "{prefix:>12.bold} [{bar:30.cyan/blue}] {pos:>3}/{len:<3} {msg}",
    )
    .unwrap_or_else(|_| ProgressStyle::default_bar())
}

/// Item bar template. Uses `{msg}` only — byte info is written into the
/// message manually so we control what is shown and when.
fn item_style() -> ProgressStyle {
    ProgressStyle::with_template("{prefix:>14} [{bar:30.cyan/blue}] {msg}")
        .unwrap_or_else(|_| ProgressStyle::default_bar())
}

// ── QualityBadge display helper ───────────────────────────────────────────

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
