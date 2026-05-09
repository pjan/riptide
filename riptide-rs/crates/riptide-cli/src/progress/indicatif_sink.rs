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
    ProgressSink, ProgressState, QualitySymbol,
};
use std::{
    collections::HashMap,
    io::IsTerminal,
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
pub struct IndicatifSink {
    inner: Mutex<SinkInner>,
}

struct SinkInner {
    state: ProgressState,
    multi: MultiProgress,
    overall_bar: ProgressBar,
    item_bars: HashMap<ItemId, ProgressBar>,
    start: Instant,
    symbols: SymbolSet,
    colors: Colors,
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

        let symbols = resolve_symbols(opts.symbols);
        let colors = Colors::resolve(opts.color);

        Self {
            inner: Mutex::new(SinkInner {
                state: ProgressState::new(config),
                multi,
                overall_bar,
                item_bars: HashMap::new(),
                start: Instant::now(),
                symbols,
                colors,
            }),
        }
    }

    fn lock(&self) -> MutexGuard<'_, SinkInner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl ProgressSink for IndicatifSink {
    #[allow(clippy::too_many_lines)]
    fn emit(&self, event: DownloadEvent) {
        let mut inner = self.lock();
        let symbols = inner.symbols;
        let colors = inner.colors;

        match &event {
            // ── Item lifecycle ────────────────────────────────────────────

            DownloadEvent::ItemQueued { item_id, label, .. } => {
                let item_id = *item_id;
                let bar = inner.multi.add(ProgressBar::new(1));
                bar.set_style(item_style());
                let op_kind = &inner.state.operation.kind;
                bar.set_prefix(colors.dim(&format!(
                    "{} {:<7}",
                    sym(symbols, RowSymbol::Queued),
                    "queued"
                )));
                bar.set_message(format_item_msg(
                    &empty_bytes_col(),
                    &colors.empty_badge(),
                    &item_label(label, op_kind),
                ));
                inner.item_bars.insert(item_id, bar);
            }

            DownloadEvent::ItemQualityResolved { item_id, quality } => {
                let item_id = *item_id;
                if let Some(bar) = inner.item_bars.get(&item_id) {
                    let op_kind = &inner.state.operation.kind;
                    if let Some(item) = inner.state.items.get(&item_id) {
                        let label = item_label(&item.label, op_kind);
                        let badge = colors.quality_badge(quality);
                        bar.set_message(format_item_msg(
                            &empty_bytes_col(),
                            &badge,
                            &label,
                        ));
                    }
                }
            }

            DownloadEvent::ItemPhaseChanged { item_id, phase } => {
                let item_id = *item_id;
                if let Some(bar) = inner.item_bars.get(&item_id) {
                    bar.set_prefix(colors.phase_prefix(symbols, phase));
                    if !matches!(phase, DownloadPhase::Downloading | DownloadPhase::Copying) {
                        bar.set_length(1);
                        bar.set_position(0);
                    }
                }
            }

            DownloadEvent::TransferUpdate { item_id, progress } => {
                let item_id = *item_id;
                if let Some(bar) = inner.item_bars.get(&item_id) {
                    if let Some(total) = progress.bytes_total {
                        bar.set_length(total);
                        bar.set_position(progress.bytes_done);
                    } else if let Some(total) = progress.segments_total {
                        bar.set_length(total);
                        bar.set_position(progress.segments_done);
                    }
                    if let Some(item) = inner.state.items.get(&item_id) {
                        let op_kind = &inner.state.operation.kind;
                        let label = item_label(&item.label, op_kind);
                        let badge = item
                            .quality
                            .as_ref()
                            .map(|q| colors.quality_badge(q))
                            .unwrap_or_default();
                        let bytes = colors.cyan(&format_bytes_padded(
                            progress.bytes_done,
                            progress.bytes_total,
                        ));
                        bar.set_message(format_item_msg(&bytes, &badge, &label));
                    }
                }
            }

            DownloadEvent::ItemFinished { item_id, outcome } => {
                let item_id = *item_id;
                if let Some(bar) = inner.item_bars.remove(&item_id) {
                    let msg = if let Some(item) = inner.state.items.get(&item_id) {
                        let op_kind = &inner.state.operation.kind;
                        let label = item_label(&item.label, op_kind);
                        let badge = item
                            .quality
                            .as_ref()
                            .map(|q| colors.quality_badge(q))
                            .unwrap_or_else(|| colors.empty_badge());
                        let detail = match outcome {
                            ItemOutcome::Failed { message } => {
                                format!("  {}", colors.red(message))
                            }
                            _ => String::new(),
                        };
                        format_item_msg(
                            &empty_bytes_col(),
                            &badge,
                            &format!("{label}{detail}"),
                        )
                    } else {
                        String::new()
                    };
                    bar.set_prefix(colors.outcome_prefix(symbols, outcome));
                    bar.set_length(1);
                    bar.set_position(1);
                    bar.finish_with_message(msg);
                }
                inner.overall_bar.inc(1);
            }

            // ── Artifacts: silent during download ─────────────────────────
            DownloadEvent::ArtifactPhaseChanged { .. }
            | DownloadEvent::ArtifactFinished { .. } => {}

            // ── Operation finish ──────────────────────────────────────────
            DownloadEvent::OperationFinished { .. } => {
                let elapsed_secs = inner.start.elapsed().as_secs();
                let elapsed = time::Duration::new(
                    i64::try_from(elapsed_secs).unwrap_or(i64::MAX),
                    0,
                );
                let _ = inner.state.reduce(event.clone());
                let summary = inner.state.summary(Some(elapsed));

                inner.overall_bar.finish();

                let total = summary.downloaded + summary.copied + summary.enriched;
                if total > 0 {
                    eprintln!(
                        "\nDownloaded {total} track(s) in {elapsed_secs}s"
                    );
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
                    eprintln!(
                        "{}",
                        colors.red(&format!("{} failed", summary.failed))
                    );
                }
                return;
            }

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

// ── ANSI colour helpers ──────────────────────────────────────────────────────
//
// Spec colour mapping:
//   dl / active transfer → cyan
//   queued, resolve      → dim
//   tag, cover, lyrics, persist → yellow
//   done                 → green
//   exists               → yellow/dim
//   failed               → red
//   Lossless quality     → cyan
//   Hi-res quality       → yellow/bold
//   Low quality          → dim

#[derive(Clone, Copy)]
struct Colors {
    enabled: bool,
}

impl Colors {
    fn resolve(mode: ColorMode) -> Self {
        let enabled = match mode {
            ColorMode::Always => true,
            ColorMode::Never => false,
            ColorMode::Auto => std::io::stderr().is_terminal(),
        };
        Self { enabled }
    }

    fn green(self, s: &str) -> String {
        self.wrap(s, "32")
    }
    fn cyan(self, s: &str) -> String {
        self.wrap(s, "36")
    }
    fn yellow(self, s: &str) -> String {
        self.wrap(s, "33")
    }
    fn red(self, s: &str) -> String {
        self.wrap(s, "31")
    }
    fn dim(self, s: &str) -> String {
        self.wrap(s, "2")
    }
    fn yellow_bold(self, s: &str) -> String {
        self.wrap2(s, "33", "1")
    }

    fn wrap(self, s: &str, code: &str) -> String {
        if self.enabled {
            format!("\x1b[{code}m{s}\x1b[0m")
        } else {
            s.to_owned()
        }
    }

    fn wrap2(self, s: &str, code1: &str, code2: &str) -> String {
        if self.enabled {
            format!("\x1b[{code1};{code2}m{s}\x1b[0m")
        } else {
            s.to_owned()
        }
    }

    /// Colours a phase prefix (symbol + phase label) according to the spec.
    fn phase_prefix(self, symbols: SymbolSet, phase: &DownloadPhase) -> String {
        let raw = format!(
            "{} {:<7}",
            sym(symbols, phase_to_symbol(phase)),
            phase_label(phase)
        );
        match phase {
            DownloadPhase::Downloading | DownloadPhase::Copying => self.cyan(&raw),
            DownloadPhase::Queued
            | DownloadPhase::ResolvingMetadata
            | DownloadPhase::ResolvingStream
            | DownloadPhase::ResolvingMedia => self.dim(&raw),
            DownloadPhase::Tagging
            | DownloadPhase::Persisting
            | DownloadPhase::WritingCover
            | DownloadPhase::WritingLyrics
            | DownloadPhase::WritingPlaylist
            | DownloadPhase::PostProcessing => self.yellow(&raw),
            _ => raw,
        }
    }

    /// Colours an outcome prefix.
    fn outcome_prefix(self, symbols: SymbolSet, outcome: &ItemOutcome) -> String {
        let raw = format!(
            "{} {:<7}",
            sym(symbols, outcome_symbol(outcome)),
            outcome_label(outcome)
        );
        match outcome {
            ItemOutcome::Downloaded | ItemOutcome::Copied | ItemOutcome::Enriched => {
                self.green(&raw)
            }
            ItemOutcome::Exists | ItemOutcome::Skipped { .. } => self.dim(&self.yellow(&raw)),
            ItemOutcome::Failed { .. } => self.red(&raw),
            _ => raw,
        }
    }

    /// Colours a quality badge per the spec, padded to `BADGE_COL` width
    /// **before** ANSI wrapping so columns stay aligned.
    fn quality_badge(self, badge: &riptide_progress::QualityBadge) -> String {
        let raw = format!("{} {}", badge.symbol_char(), badge.text);
        let padded = format!("{raw:<BADGE_COL$}");
        match badge.symbol_kind {
            QualitySymbol::Lossless => self.cyan(&padded),
            QualitySymbol::HiRes => self.yellow_bold(&padded),
            QualitySymbol::Low => self.dim(&padded),
            _ => padded,
        }
    }

    /// Returns an empty badge column of `BADGE_COL` spaces.
    fn empty_badge(self) -> String {
        " ".repeat(BADGE_COL)
    }
}

// ── Formatting helpers ──────────────────────────────────────────────────────

/// Assembles the item message from pre-padded, pre-coloured columns.
///
/// Each column is padded to its visual width **before** ANSI codes are applied
/// so that escape sequences don't throw off alignment.
fn format_item_msg(bytes: &str, badge: &str, label: &str) -> String {
    format!("{bytes} {badge} {label}")
}

/// Formats byte progress compactly as `3.5/24.0 MiB` and right-pads to
/// `BYTES_COL` visual width **before** any ANSI wrapping.
fn format_bytes_padded(done: u64, total: Option<u64>) -> String {
    let raw = match total {
        Some(t) => compact_bytes(done, t),
        None => format!("{}", HumanBytes(done)),
    };
    format!("{raw:>BYTES_COL$}")
}

/// Returns an empty string of `BYTES_COL` spaces (used when no transfer is active).
fn empty_bytes_col() -> String {
    " ".repeat(BYTES_COL)
}

/// Visual width of the bytes column.
const BYTES_COL: usize = 18;

/// Visual width of the quality badge column.
const BADGE_COL: usize = 14;

/// Formats `done/total` using a shared unit: `3.5/24.0 MiB`.
fn compact_bytes(done: u64, total: u64) -> String {
    // Pick the unit from the larger value (total).
    let (divisor, unit) = byte_unit(total);
    let d = done as f64 / divisor;
    let t = total as f64 / divisor;
    format!("{d:.2}/{t:.2} {unit}")
}

fn byte_unit(bytes: u64) -> (f64, &'static str) {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * 1024;
    const GIB: u64 = 1024 * 1024 * 1024;
    if bytes >= GIB {
        (GIB as f64, "GiB")
    } else if bytes >= MIB {
        (MIB as f64, "MiB")
    } else if bytes >= KIB {
        (KIB as f64, "KiB")
    } else {
        (1.0, "B")
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
        match self.symbol_kind {
            QualitySymbol::Low => "•",
            QualitySymbol::High => "▪",
            QualitySymbol::Lossless => "◇",
            QualitySymbol::HiRes => "◆",
            QualitySymbol::Video => "▣",
            QualitySymbol::Unknown | _ => "◇",
        }
    }
}
