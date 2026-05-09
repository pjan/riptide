//! Indicatif-based interactive progress sink.

use crate::progress::{
    colors::{resolve_color, ColorEnabled},
    options::{ColorMode, CompletionDetail, ProgressDetail, SymbolsMode},
    symbols::{resolve_symbols, symbol_for, SymbolSet},
};
use indicatif::{HumanBytes, MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};
use riptide_progress::{
    row::{
        build_active_rows, build_completion_rows, phase_to_symbol, ActiveRow, CompletionRow,
        CountUnit, RowSymbol,
    },
    ArtifactKind, ArtifactOutcome, DownloadEvent, DownloadPhase, GroupId, GroupOutcome,
    ItemId, ItemOutcome, OperationConfig, ProgressSink, ProgressState,
};
use std::{
    collections::HashMap,
    sync::{Mutex, MutexGuard, PoisonError},
    time::Instant,
};

/// Rendering options passed to [`IndicatifSink::new`].
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
/// Holds one `ProgressBar` per active item/artifact. When `OperationFinished`
/// is received it prints all completion rows and the summary.
pub struct IndicatifSink {
    inner: Mutex<SinkInner>,
    opts: RenderOpts,
}

struct SinkInner {
    state: ProgressState,
    multi: MultiProgress,
    overall_bar: ProgressBar,
    item_bars: HashMap<ItemId, ProgressBar>,
    artifact_bars: HashMap<ArtifactKey, ProgressBar>,
    start: Instant,
}

// Lightweight key for artifact bars (mirrors ArtifactKey but without the
// operation_id since there's one state per operation).
#[derive(Clone, PartialEq, Eq, Hash)]
struct ArtifactKey {
    group_id: Option<GroupId>,
    item_id: Option<ItemId>,
    kind: ArtifactKind,
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
                artifact_bars: HashMap::new(),
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

        // Handle bar updates before reducing the state, since some events
        // carry data we need for bar creation (labels, IDs).
        match &event {
            DownloadEvent::ItemQueued {
                item_id,
                label,
                group_id,
                ..
            } => {
                let item_id = *item_id;
                let is_in_group = group_id.is_some();
                let bar = inner.multi.add(ProgressBar::new(1));
                bar.set_style(item_style());
                bar.set_prefix(sym(symbols, RowSymbol::Queued));
                let msg = format_item_label_str(label, is_in_group);
                bar.set_message(msg);
                inner.item_bars.insert(item_id, bar);
            }

            DownloadEvent::ItemQualityResolved { item_id, quality } => {
                let item_id = *item_id;
                if let Some(bar) = inner.item_bars.get(&item_id) {
                    // Prepend quality badge to message. Read existing message
                    // (label part) from state after reducing, but we can build
                    // it here from the event data. We'll update after reduce.
                    let badge = format!("{} {}", quality.symbol_char(), &quality.text);
                    // Update the message prefix only; keep label suffix
                    let current = bar.message();
                    bar.set_message(format!("{badge:<14} {current}"));
                }
            }

            DownloadEvent::ItemPhaseChanged { item_id, phase } => {
                let item_id = *item_id;
                if let Some(bar) = inner.item_bars.get(&item_id) {
                    let prefix = sym(symbols, phase_to_symbol(phase));
                    bar.set_prefix(format!("{prefix} {}", phase_label(phase)));
                    // Reset bar for non-transfer phases
                    if !matches!(phase, DownloadPhase::Downloading | DownloadPhase::Copying) {
                        bar.set_length(1);
                        bar.set_position(0);
                    }
                }
            }

            DownloadEvent::TransferUpdate { item_id, progress } => {
                let item_id = *item_id;
                if let Some(bar) = inner.item_bars.get(&item_id) {
                    // Numeric updates only — no string formatting here.
                    if let Some(total) = progress.bytes_total {
                        bar.set_length(total);
                        bar.set_position(progress.bytes_done);
                    } else if let Some(total) = progress.segments_total {
                        bar.set_length(total);
                        bar.set_position(progress.segments_done);
                    }
                }
            }

            DownloadEvent::ArtifactPhaseChanged {
                group_id,
                item_id,
                kind,
                label,
                phase,
                ..
            } => {
                let key = ArtifactKey {
                    group_id: *group_id,
                    item_id: *item_id,
                    kind: kind.clone(),
                };
                if !inner.artifact_bars.contains_key(&key) {
                    let bar = inner.multi.add(ProgressBar::new(1));
                    bar.set_style(item_style());
                    bar.set_prefix(sym(symbols, RowSymbol::Artifact));
                    bar.set_message(label.clone());
                    inner.artifact_bars.insert(key, bar);
                } else if let Some(bar) = inner.artifact_bars.get(&key) {
                    bar.set_prefix(sym(symbols, phase_to_symbol(phase)));
                    bar.set_message(label.clone());
                }
            }

            DownloadEvent::ArtifactFinished {
                group_id,
                item_id,
                kind,
                label,
                outcome,
                ..
            } => {
                let key = ArtifactKey {
                    group_id: *group_id,
                    item_id: *item_id,
                    kind: kind.clone(),
                };
                if let Some(bar) = inner.artifact_bars.remove(&key) {
                    let (prefix, msg) = match outcome {
                        ArtifactOutcome::Written => {
                            (sym(symbols, RowSymbol::Done), label.clone())
                        }
                        ArtifactOutcome::Skipped { .. } => {
                            (sym(symbols, RowSymbol::Exists), label.clone())
                        }
                        ArtifactOutcome::Failed { message } => {
                            (sym(symbols, RowSymbol::Failed), message.clone())
                        }
                        _ => (sym(symbols, RowSymbol::Phase), label.clone()),
                    };
                    bar.set_prefix(prefix);
                    bar.finish_with_message(msg);
                }
            }

            DownloadEvent::ItemFinished {
                item_id, outcome, ..
            } => {
                let item_id = *item_id;
                if let Some(bar) = inner.item_bars.remove(&item_id) {
                    let (prefix, msg) = item_finish_display(symbols, outcome);
                    bar.set_prefix(prefix);
                    bar.finish_with_message(msg);
                }
                inner.overall_bar.inc(1);
            }

            DownloadEvent::GroupFinished { outcome, .. } => match outcome {
                GroupOutcome::Done | GroupOutcome::Partial { .. } => {}
                GroupOutcome::Failed { message } => {
                    inner
                        .overall_bar
                        .println(format!("group failed: {message}"));
                }
                _ => {}
            },

            DownloadEvent::OperationFinished { .. } => {
                let elapsed_secs = inner.start.elapsed().as_secs();
                let elapsed = time::Duration::new(
                    i64::try_from(elapsed_secs).unwrap_or(i64::MAX),
                    0,
                );
                let summary = inner.state.summary(Some(elapsed));

                inner.overall_bar.finish_and_clear();

                // Print completion rows based on completion_detail setting.
                let completion_rows = build_completion_rows(&inner.state);
                for row in &completion_rows {
                    print_completion_row(row, symbols, &inner.overall_bar);
                }

                // Print final summary.
                let total = summary.downloaded + summary.copied + summary.enriched;
                if total > 0 {
                    inner
                        .overall_bar
                        .println(format!("\nDownloaded {total} track(s) in {elapsed_secs}s"));
                }
                for artifact in &summary.artifacts {
                    let kind_label = artifact_kind_label(&artifact.kind);
                    if artifact.count > 1 {
                        inner.overall_bar.println(format!(
                            "Saved {}: {}",
                            kind_label, artifact.count
                        ));
                    } else if artifact.count == 1 {
                        inner.overall_bar.println(format!(
                            "Saved {kind_label}: {}",
                            artifact.label
                        ));
                    }
                }
                if summary.failed > 0 {
                    inner
                        .overall_bar
                        .println(format!("{} failed", summary.failed));
                }
            }

            // Low-frequency administrative events.
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

fn format_item_label_str(label: &riptide_progress::DisplayLabel, is_in_group: bool) -> String {
    let num = label
        .track_number
        .map(|n| format!("{:02} ", n.get()))
        .unwrap_or_default();
    let titled = format!("{num}{}", label.title);
    if is_in_group || label.artist.is_none() {
        titled
    } else {
        format!(
            "{} - {titled}",
            label.artist.as_deref().unwrap_or("")
        )
    }
}

fn phase_label(phase: &DownloadPhase) -> &'static str {
    match phase {
        DownloadPhase::Queued => "queued",
        DownloadPhase::ResolvingMetadata => "resolve",
        DownloadPhase::ResolvingStream => "resolve",
        DownloadPhase::ResolvingMedia => "resolve",
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

fn item_finish_display(
    symbols: SymbolSet,
    outcome: &ItemOutcome,
) -> (String, String) {
    match outcome {
        ItemOutcome::Downloaded | ItemOutcome::Copied | ItemOutcome::Enriched => {
            (sym(symbols, RowSymbol::Done), "done".to_owned())
        }
        ItemOutcome::Exists => (sym(symbols, RowSymbol::Exists), "exists".to_owned()),
        ItemOutcome::Skipped { reason } => {
            (sym(symbols, RowSymbol::Exists), format!("skipped: {reason}"))
        }
        ItemOutcome::Failed { message } => {
            (sym(symbols, RowSymbol::Failed), message.clone())
        }
        _ => (sym(symbols, RowSymbol::Phase), String::new()),
    }
}

fn print_completion_row(
    row: &CompletionRow,
    symbols: SymbolSet,
    printer: &ProgressBar,
) {
    match row {
        CompletionRow::Item {
            symbol,
            label,
            quality,
            detail,
            ..
        } => {
            let s = sym(symbols, *symbol);
            let q = quality
                .as_ref()
                .map(|b| format!("{} {:<13}", b.symbol_char(), b.text))
                .unwrap_or_else(|| " ".repeat(14));
            let d = detail.as_deref().unwrap_or("");
            let sep = if d.is_empty() { "" } else { "  " };
            printer.println(format!("{s} {q} {label}{sep}{d}"));
        }
        CompletionRow::Artifact {
            symbol,
            kind: _,
            label,
            ..
        } => {
            let s = sym(symbols, *symbol);
            printer.println(format!("{s} {label}"));
        }
        CompletionRow::Group {
            symbol,
            kind: _,
            label,
            items,
            ..
        } => {
            let s = sym(symbols, *symbol);
            let count_str = items.as_ref().map_or(String::new(), |c| {
                let unit = count_unit_label(c.unit);
                format!("  {}/{} {unit}", c.done, c.total.unwrap_or(c.done))
            });
            printer.println(format!("{s} {label}{count_str}"));
        }
        _ => {} // forward-compat wildcard
    }
}

fn count_unit_label(unit: CountUnit) -> &'static str {
    match unit {
        CountUnit::Tracks => "tracks",
        CountUnit::Videos => "videos",
        CountUnit::Albums => "albums",
        CountUnit::Files => "files",
        CountUnit::Items => "items",
        CountUnit::Groups => "groups",
        _ => "items",
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

fn item_style() -> ProgressStyle {
    ProgressStyle::with_template(
        "{prefix:>14} [{bar:30.cyan/blue}] {bytes:>12} {msg}",
    )
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
