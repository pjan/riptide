//! TUI progress sink with fixed-width column layout.
//!
//! Renders active downloads as a compact table with quality, target, progress,
//! and (optionally) speed columns. Uses indicatif's [`MultiProgress`] for
//! cursor management but builds all row content via pure formatting functions.

#![allow(
    clippy::cast_precision_loss,
    clippy::match_same_arms,
    clippy::missing_const_for_fn,
    clippy::option_if_let_else,
    clippy::literal_string_with_formatting_args
)]

use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};
use riptide_progress::{
    DownloadEvent, DownloadPhase, ItemId, OperationConfig, OperationKind, ProgressSink,
    ProgressState, QualityBadge, QualitySymbol,
};
use std::{
    collections::HashMap,
    fmt::Write as _,
    io::IsTerminal,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard, PoisonError},
    time::Instant,
};

use crate::progress::options::ColorMode;

// ── Layout ─────────────────────────────────────────────────────────────────

/// Column layout mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LayoutMode {
    /// 100-char layout with 4 columns (quality, target, progress, speed).
    Wide,
    /// 80-char layout with 3 columns (quality, target, progress).
    Narrow,
}

impl LayoutMode {
    /// Detect layout mode from the terminal width.
    ///
    /// Falls back to `Wide` when the terminal width cannot be determined
    /// (e.g. piped output, missing libc support).
    fn detect() -> Self {
        let width = detect_terminal_width().unwrap_or(100);
        if width >= 100 {
            Self::Wide
        } else {
            Self::Narrow
        }
    }

    const fn total_width(self) -> usize {
        match self {
            Self::Wide => 100,
            Self::Narrow => 80,
        }
    }

    const fn col2_width(self) -> usize {
        match self {
            Self::Wide => 42,
            Self::Narrow => 35,
        }
    }

    const fn has_speed(self) -> bool {
        matches!(self, Self::Wide)
    }
}

/// Best-effort terminal width detection.
///
/// Uses `console::Term` to query the actual terminal width, falling back to
/// the `COLUMNS` environment variable. Returns `None` if unavailable.
fn detect_terminal_width() -> Option<usize> {
    let term = console::Term::stderr();
    let (_, width) = term.size_checked()?;
    Some(width as usize)
}

// ── Column constants ───────────────────────────────────────────────────────

const COL1_WIDTH: usize = 9; // quality badge
const COL3_WIDTH: usize = 28; // progress bar or phase
const COL4_WIDTH: usize = 10; // speed
const DIVIDER: &str = " \u{2502} "; // " │ " — 3 chars
const BAR_CHARS: usize = 20;

// Braille spinner frames.
const SPINNER_FRAMES: &[char] = &[
    '\u{280B}', '\u{2819}', '\u{2839}', '\u{2838}', '\u{283C}', '\u{2834}', '\u{2826}', '\u{2827}',
    '\u{2807}', '\u{280F}',
];

// ── Entity icons ───────────────────────────────────────────────────────────

fn entity_icon(kind: &OperationKind) -> &'static str {
    match kind {
        OperationKind::Artist => "\u{2605}",             // ★
        OperationKind::Album => "\u{25CE}",              // ◎
        OperationKind::Track => "\u{266A}",              // ♪
        OperationKind::Playlist => "\u{2630}",           // ☰
        OperationKind::Mix => "\u{2637}",                // ☷
        OperationKind::Collection => "\u{25CE}",         // ◎
        OperationKind::LocalTransfer => "\u{25CE}",      // ◎
        OperationKind::MetadataEnrichment => "\u{25CE}", // ◎
        _ => "\u{25CE}",                                 // ◎
    }
}

// ── Pure formatting functions ──────────────────────────────────────────────

/// Renders a progress bar: `[ ▰▰▰▰▰▱▱▱▱▱▱▱▱▱▱▱▱▱▱▱ ]  35%`
///
/// Output is exactly [`COL3_WIDTH`] (28) characters.
fn format_progress_bar(pct: f64) -> String {
    let clamped = pct.clamp(0.0, 100.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let filled = ((clamped / 100.0) * BAR_CHARS as f64).round() as usize;
    let empty = BAR_CHARS.saturating_sub(filled);

    let mut buf = String::with_capacity(COL3_WIDTH + 4);
    buf.push_str("[ ");
    for _ in 0..filled {
        buf.push('\u{25B0}'); // ▰
    }
    for _ in 0..empty {
        buf.push('\u{25B1}'); // ▱
    }
    let _ = write!(buf, " ]{clamped:>3.0}%");
    debug_assert_eq!(visual_width(&buf), COL3_WIDTH);
    buf
}

/// Renders a phase label with a braille spinner, padded to [`COL3_WIDTH`].
///
/// Output is exactly 28 characters.
fn format_phase(phase: &DownloadPhase, spinner_frame: usize) -> String {
    let spinner = SPINNER_FRAMES[spinner_frame % SPINNER_FRAMES.len()];
    let label = phase_label(phase);
    // spinner(1) + space(1) + label + padding = 28
    let mut buf = String::with_capacity(COL3_WIDTH + 4);
    let _ = write!(buf, "{spinner} {label}");
    let pad = COL3_WIDTH.saturating_sub(visual_width(&buf));
    for _ in 0..pad {
        buf.push(' ');
    }
    buf
}

/// Renders speed as a right-aligned string in [`COL4_WIDTH`] (10) chars.
///
/// Returns `"   -------"` when `rate` is zero or negative.
fn format_speed(rate: f64) -> String {
    if rate <= 0.0 {
        return format!("{:>COL4_WIDTH$}", "-------");
    }
    let mb_s = rate / (1024.0 * 1024.0);
    let text = format!("{mb_s:.1} MB/s");
    format!("{text:>COL4_WIDTH$}")
}

/// Renders a quality badge, padded to [`COL1_WIDTH`] (9) chars.
///
/// Unknown quality is shown as `"-------  "` (7 hyphens + 2 spaces).
fn format_quality(badge: Option<&QualityBadge>) -> String {
    match badge {
        Some(b) => {
            let sym = quality_symbol_char(b);
            let raw = format!("{sym} {}", b.text);
            truncate_pad(&raw, COL1_WIDTH)
        }
        None => format!("{:<COL1_WIDTH$}", "-------"),
    }
}

/// Renders the target column (col2) for an active item.
///
/// Layout depends on the operation kind:
/// - Artist: `[Album Na\u{2026}] TT Track Title`
/// - Album: `TT Track Title`
/// - Track: `Track Title`
/// - Playlist/Mix: `[Artist N\u{2026}] Track Title`
fn format_target(
    label: &riptide_progress::DisplayLabel,
    op_kind: &OperationKind,
    max_width: usize,
) -> String {
    match op_kind {
        OperationKind::Artist => {
            let track_num = track_number_prefix(label);
            let bracket = label.group.as_deref().unwrap_or("");
            format_bracketed_target(bracket, &track_num, &label.title, max_width)
        }
        OperationKind::Album => {
            let track_num = track_number_prefix(label);
            let title = format!("{track_num}{}", label.title);
            truncate_pad(&title, max_width)
        }
        OperationKind::Track => truncate_pad(&label.title, max_width),
        OperationKind::Playlist | OperationKind::Mix => {
            let bracket = label.artist.as_deref().unwrap_or("");
            format_bracketed_target(bracket, "", &label.title, max_width)
        }
        _ => {
            let track_num = track_number_prefix(label);
            let title = format!("{track_num}{}", label.title);
            truncate_pad(&title, max_width)
        }
    }
}

/// Renders the item summary text for the overall/total row in col2.
///
/// Artist: `[1 of 2] Albums // [22 of 24] Tracks`.
/// Album/Playlist/Mix: `[9 of 14] Tracks`.
/// Unknown totals show `?`.
fn format_total_col2(
    op_kind: &OperationKind,
    groups_done: u64,
    total_groups: Option<u64>,
    items_done: u64,
    total_items: Option<u64>,
    max_width: usize,
) -> String {
    let items_total_str = total_items.map_or_else(|| "?".to_owned(), |n| n.to_string());
    let text = match op_kind {
        OperationKind::Artist => {
            let groups_total_str = total_groups.map_or_else(|| "?".to_owned(), |n| n.to_string());
            format!(
                "[{groups_done} of {groups_total_str}] Albums // [{items_done} of {items_total_str}] Tracks"
            )
        }
        _ => format!("[{items_done} of {items_total_str}] Tracks"),
    };
    truncate_pad(&text, max_width)
}

/// Renders a horizontal border of `\u{2500}` characters.
fn format_border(width: usize) -> String {
    "\u{2500}".repeat(width)
}

/// Renders the header line: `Downloading: \u{2605} Artist Name`.
fn format_header(
    op_kind: &OperationKind,
    label: &riptide_progress::DisplayLabel,
    downloading: bool,
) -> String {
    let verb = if downloading {
        "Downloading"
    } else {
        "Download completed"
    };
    let icon = entity_icon(op_kind);
    match op_kind {
        OperationKind::Artist => {
            let name = label.artist.as_deref().unwrap_or(&label.title);
            format!("{verb}: {icon} {name}")
        }
        OperationKind::Album => match &label.artist {
            Some(artist) => format!("{verb}: {icon} [{artist}] {}", label.title),
            None => format!("{verb}: {icon} {}", label.title),
        },
        OperationKind::Track => match &label.artist {
            Some(artist) => format!("{verb}: {icon} [{artist}] {}", label.title),
            None => format!("{verb}: {icon} {}", label.title),
        },
        OperationKind::Playlist | OperationKind::Mix => {
            format!("{verb}: {icon} {}", label.title)
        }
        _ => format!("{verb}: {icon} {}", label.title),
    }
}

/// Converts bytes to megabytes (decimal, 1 MB = `1_000_000` bytes).
fn bytes_to_mb(bytes: u64) -> f64 {
    bytes as f64 / 1_000_000.0
}

/// Formats a duration as `MM:SS` or `H:MM:SS`.
fn format_elapsed(elapsed: &time::Duration) -> String {
    let total_secs = elapsed.whole_seconds().unsigned_abs();
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

/// Renders the completion receipt.
fn format_receipt(
    summary: &riptide_progress::ProgressSummary,
    op_kind: &OperationKind,
    label: &riptide_progress::DisplayLabel,
    output_root: &Path,
    show_failures: bool,
    layout: LayoutMode,
    colors: Colors,
) -> String {
    let width = layout.total_width();
    let header = format_header(op_kind, label, false);
    let border = colors.dim(&format_border(width));

    let elapsed_str = summary
        .elapsed
        .as_ref()
        .map_or_else(|| "00:00".to_owned(), format_elapsed);

    let total_mb = bytes_to_mb(summary.total_bytes);
    let avg_speed = summary.elapsed.as_ref().map_or(0.0, |e| {
        let secs = e.whole_seconds().unsigned_abs() as f64;
        if secs > 0.0 {
            bytes_to_mb(summary.total_bytes) / secs
        } else {
            0.0
        }
    });

    let error_line = if summary.failed > 0 {
        colors.red(&format!(" \u{26A0} {} errors / skipped", summary.failed))
    } else {
        colors.green(" \u{26A0} No errors / skipped")
    };

    let display_path = output_root.display();

    let mut buf = String::with_capacity(512);
    let _ = writeln!(buf, "{header}");
    let _ = writeln!(buf);
    let _ = writeln!(buf, "{border}");
    let _ = writeln!(buf, " \u{23F1} {elapsed_str} (Average {avg_speed:.1} MB/s)");
    let _ = writeln!(buf, " \u{26C1} {total_mb:.1} MB");
    let _ = writeln!(buf, "{error_line}");
    let _ = writeln!(buf, "{border}");
    let _ = write!(buf, " \u{21B3} Saved to: {display_path}");

    if show_failures && summary.failed > 0 && !summary.failed_labels.is_empty() {
        let _ = writeln!(buf);
        let joined = summary.failed_labels.join(", ");
        let _ = write!(
            buf,
            "{}",
            colors.red(&format!(" \u{26A0} Failed: {joined}"))
        );
    }

    buf
}

// ── Colour helpers ─────────────────────────────────────────────────────────

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

    fn cyan(self, s: &str) -> String {
        self.wrap(s, "36")
    }

    fn yellow(self, s: &str) -> String {
        self.wrap(s, "33")
    }

    fn green(self, s: &str) -> String {
        self.wrap(s, "32")
    }

    fn red(self, s: &str) -> String {
        self.wrap(s, "31")
    }

    fn dim(self, s: &str) -> String {
        self.wrap(s, "2")
    }

    fn bold(self, s: &str) -> String {
        self.wrap(s, "1")
    }

    fn wrap(self, s: &str, code: &str) -> String {
        if self.enabled {
            format!("\x1b[{code}m{s}\x1b[0m")
        } else {
            s.to_owned()
        }
    }

    /// Colour a quality badge string.
    fn colour_quality(self, badge: &QualityBadge, text: &str) -> String {
        match badge.symbol_kind {
            QualitySymbol::Lossless => self.cyan(text),
            QualitySymbol::HiRes => self.wrap(text, "33;1"),
            QualitySymbol::Low => self.dim(text),
            _ => text.to_owned(),
        }
    }

    /// Colour the progress bar fill characters.
    fn colour_progress(self, text: &str) -> String {
        self.cyan(text)
    }

    /// Colour the speed column.
    fn colour_speed(self, text: &str) -> String {
        self.cyan(text)
    }

    /// Colour a spinner + phase text.
    fn colour_phase(self, text: &str) -> String {
        self.yellow(text)
    }
}

// ── Internal helpers ───────────────────────────────────────────────────────

fn quality_symbol_char(badge: &QualityBadge) -> &'static str {
    match badge.symbol_kind {
        QualitySymbol::Low => "\u{2022}",         // •
        QualitySymbol::High => "\u{25AA}",        // ▪
        QualitySymbol::Lossless => "\u{25C7}",    // ◇
        QualitySymbol::HiRes => "\u{25C6}",       // ◆
        QualitySymbol::Unknown | _ => "\u{25C7}", // ◇
    }
}

fn phase_label(phase: &DownloadPhase) -> &'static str {
    match phase {
        DownloadPhase::Queued => "Queued...",
        DownloadPhase::ResolvingMetadata => "Resolving Metadata...",
        DownloadPhase::ResolvingStream => "Resolving Stream...",
        DownloadPhase::ResolvingMedia => "Resolving Media...",
        DownloadPhase::Downloading => "Downloading...",
        DownloadPhase::Copying => "Copying...",
        DownloadPhase::PostProcessing => "Post-Processing...",
        DownloadPhase::Tagging => "Tagging Metadata...",
        DownloadPhase::Persisting => "Saving File...",
        DownloadPhase::WritingCover => "Storing Album Cover...",
        DownloadPhase::WritingLyrics => "Storing Lyrics...",
        DownloadPhase::WritingPlaylist => "Writing Playlist...",
        _ => "Processing...",
    }
}

fn track_number_prefix(label: &riptide_progress::DisplayLabel) -> String {
    label
        .track_number
        .map(|n| format!("{:02} ", n.get()))
        .unwrap_or_default()
}

/// Format a bracketed target: `[Bracket C\u{2026}] Title`.
///
/// Truncation order: bracket content first (min 5 chars before \u{2026}), then title.
fn format_bracketed_target(
    bracket: &str,
    track_num: &str,
    title: &str,
    max_width: usize,
) -> String {
    if bracket.is_empty() {
        let full = format!("{track_num}{title}");
        return truncate_pad(&full, max_width);
    }

    // [bracket] track_num title
    // brackets add: [ + ] + space = 3 chars
    let bracket_overhead = 3;
    let title_part = format!("{track_num}{title}");

    let full = format!("[{bracket}] {title_part}");
    let full_len = visual_width(&full);

    if full_len <= max_width {
        return pad_right(&full, max_width);
    }

    // Try truncating bracket content first (min 5 visible chars).
    let min_bracket = 5;
    let title_len = visual_width(&title_part);
    // We need: bracket_overhead + bracket_content + 1(ellipsis if truncated) + 1(space) + title_len <= max_width
    // Actually: [xxxxx…] title_part
    let available_for_bracket = max_width
        .saturating_sub(bracket_overhead)
        .saturating_sub(1) // ellipsis
        .saturating_sub(title_len);

    if available_for_bracket >= min_bracket {
        let truncated = truncate_with_ellipsis(bracket, available_for_bracket);
        let result = format!("[{truncated}] {title_part}");
        return truncate_pad(&result, max_width);
    }

    // Bracket at minimum, truncate title.
    let truncated_bracket = truncate_with_ellipsis(bracket, min_bracket);
    let remaining = max_width
        .saturating_sub(bracket_overhead)
        .saturating_sub(visual_width(&truncated_bracket))
        .saturating_sub(1); // space between ] and title
    let truncated_title = truncate_with_ellipsis(&title_part, remaining);
    let result = format!("[{truncated_bracket}] {truncated_title}");
    truncate_pad(&result, max_width)
}

/// Truncate a string to `max_chars`, appending \u{2026} if truncated.
fn truncate_with_ellipsis(s: &str, max_chars: usize) -> String {
    let len = s.chars().count();
    if len <= max_chars {
        return s.to_owned();
    }
    if max_chars == 0 {
        return String::new();
    }
    let mut result: String = s.chars().take(max_chars.saturating_sub(1)).collect();
    result.push('\u{2026}');
    result
}

/// Truncate and right-pad to exactly `width` visible characters.
fn truncate_pad(s: &str, width: usize) -> String {
    let len = visual_width(s);
    if len > width {
        let truncated = truncate_with_ellipsis(s, width);
        pad_right(&truncated, width)
    } else {
        pad_right(s, width)
    }
}

fn pad_right(s: &str, width: usize) -> String {
    let len = visual_width(s);
    if len >= width {
        s.to_owned()
    } else {
        let mut buf = String::with_capacity(s.len() + width - len);
        buf.push_str(s);
        for _ in 0..(width - len) {
            buf.push(' ');
        }
        buf
    }
}

/// Simple visible width: counts Unicode chars.
///
/// This is a simplification that works for the ASCII + common Unicode symbols
/// used in the UI. Full-width CJK handling is out of scope.
fn visual_width(s: &str) -> usize {
    s.chars().count()
}

// ── Row assembly ───────────────────────────────────────────────────────────

/// Assembles a full item row string from pre-formatted columns.
fn assemble_item_row(
    col1: &str,
    col2: &str,
    col3: &str,
    col4: Option<&str>,
    colors: Colors,
) -> String {
    let div = colors.dim(DIVIDER);
    match col4 {
        Some(speed) => format!(" {col1}{div}{col2}{div}{col3}{div}{speed} "),
        None => format!(" {col1}{div}{col2}{div}{col3} "),
    }
}

/// Assembles the total/overall row string.
#[allow(clippy::too_many_arguments)]
fn assemble_total_row(
    op_kind: &OperationKind,
    groups_done: u64,
    total_groups: Option<u64>,
    items_done: u64,
    items_total: Option<u64>,
    aggregate_rate: f64,
    layout: LayoutMode,
    colors: Colors,
) -> String {
    let col1 = format!("{:<COL1_WIDTH$}", "Total");
    let col2 = format_total_col2(
        op_kind,
        groups_done,
        total_groups,
        items_done,
        items_total,
        layout.col2_width(),
    );

    let pct = match items_total {
        Some(total) if total > 0 => (items_done as f64 / total as f64) * 100.0,
        _ => 0.0,
    };
    let col3 = format_progress_bar(pct);
    let col3_coloured = colors.colour_progress(&col3);

    let col4 = if layout.has_speed() {
        let raw = format_speed(aggregate_rate);
        Some(colors.colour_speed(&raw))
    } else {
        None
    };

    let div = colors.dim(DIVIDER);
    match col4 {
        Some(speed) => format!(" {col1}{div}{col2}{div}{col3_coloured}{div}{speed} "),
        None => format!(" {col1}{div}{col2}{div}{col3_coloured} "),
    }
}

// ── TuiSink ────────────────────────────────────────────────────────────────

/// Rendering options for [`TuiSink`].
#[derive(Debug, Clone)]
pub struct TuiRenderOpts {
    /// Output root directory for the completion receipt.
    pub output_root: PathBuf,
    /// Whether to show individual failure labels in the receipt.
    pub show_failures: bool,
    /// Colour mode.
    pub color: ColorMode,
}

/// Interactive TUI progress sink with fixed-width column layout.
pub struct TuiSink {
    inner: Mutex<TuiInner>,
}

struct TuiInner {
    state: ProgressState,
    layout: LayoutMode,
    multi: MultiProgress,
    #[allow(dead_code)]
    header_bar: ProgressBar,
    #[allow(dead_code)]
    top_border_bar: ProgressBar,
    overall_bar: Option<ProgressBar>,
    #[allow(dead_code)]
    mid_border_bar: Option<ProgressBar>,
    item_bars: HashMap<ItemId, ProgressBar>,
    bottom_border_bar: ProgressBar,
    start: Instant,
    spinner_frame: usize,
    output_root: PathBuf,
    show_failures: bool,
    colors: Colors,
}

impl TuiSink {
    /// Creates a new TUI sink for the given operation.
    pub fn new(config: OperationConfig, opts: TuiRenderOpts) -> Self {
        let layout = LayoutMode::detect();
        let colors = Colors::resolve(opts.color);
        let multi = MultiProgress::with_draw_target(ProgressDrawTarget::stderr_with_hz(10));

        // Header bar (static text, no progress).
        let header_bar = multi.add(ProgressBar::new(0));
        header_bar.set_style(
            ProgressStyle::with_template("{msg}").unwrap_or_else(|_| ProgressStyle::default_bar()),
        );
        let header_text = format_header(&config.kind, &config.label, true);
        header_bar.set_message(colors.bold(&header_text));

        let border_text = colors.dim(&format_border(layout.total_width()));
        let text_style =
            ProgressStyle::with_template("{msg}").unwrap_or_else(|_| ProgressStyle::default_bar());

        // Top border: between header and total/items.
        let top_border_bar = multi.add(ProgressBar::new(0));
        top_border_bar.set_style(text_style.clone());
        top_border_bar.set_message(border_text.clone());

        // Overall/total bar + mid border: shown for multi-item operations (not Track).
        let (overall_bar, mid_border_bar) = if matches!(config.kind, OperationKind::Track) {
            (None, None)
        } else {
            let bar = multi.add(ProgressBar::new(config.total_items.unwrap_or(0)));
            bar.set_style(text_style.clone());
            bar.set_message(assemble_total_row(
                &config.kind,
                0,
                config.total_groups,
                0,
                config.total_items,
                0.0,
                layout,
                colors,
            ));
            let mid = multi.add(ProgressBar::new(0));
            mid.set_style(text_style.clone());
            mid.set_message(border_text.clone());
            (Some(bar), Some(mid))
        };

        // Bottom border: below items. Starts visible, items insert above it.
        let bottom_border_bar = multi.add(ProgressBar::new(0));
        bottom_border_bar.set_style(text_style);
        bottom_border_bar.set_message(border_text);

        Self {
            inner: Mutex::new(TuiInner {
                state: ProgressState::new(config),
                layout,
                multi,
                header_bar,
                top_border_bar,
                overall_bar,
                mid_border_bar,
                item_bars: HashMap::new(),
                bottom_border_bar,
                start: Instant::now(),
                spinner_frame: 0,
                output_root: opts.output_root,
                show_failures: opts.show_failures,
                colors,
            }),
        }
    }

    fn lock(&self) -> MutexGuard<'_, TuiInner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl TuiInner {
    /// Rebuild the message string for an item bar.
    fn rebuild_item_message(&self, item_id: ItemId) -> Option<String> {
        let item = self.state.items.get(&item_id)?;
        let op_kind = &self.state.operation.kind;

        let col1_raw = format_quality(item.quality.as_ref());
        let col1 = match &item.quality {
            Some(badge) => self.colors.colour_quality(badge, &col1_raw),
            None => col1_raw,
        };

        let col2 = format_target(&item.label, op_kind, self.layout.col2_width());

        let col3 = if matches!(
            item.phase,
            DownloadPhase::Downloading | DownloadPhase::Copying
        ) {
            if let Some(ref transfer) = item.transfer {
                if let Some(total) = transfer.bytes_total {
                    if total > 0 {
                        let pct = (transfer.bytes_done as f64 / total as f64) * 100.0;
                        let raw = format_progress_bar(pct);
                        self.colors.colour_progress(&raw)
                    } else {
                        let raw = format_progress_bar(0.0);
                        self.colors.colour_progress(&raw)
                    }
                } else {
                    let raw = format_progress_bar(0.0);
                    self.colors.colour_progress(&raw)
                }
            } else {
                let raw = format_progress_bar(0.0);
                self.colors.colour_progress(&raw)
            }
        } else {
            let raw = format_phase(&item.phase, self.spinner_frame);
            self.colors.colour_phase(&raw)
        };

        let col4 = if self.layout.has_speed() {
            let raw = format_speed(item.current_rate_bytes_per_sec);
            Some(self.colors.colour_speed(&raw))
        } else {
            None
        };

        Some(assemble_item_row(
            &col1,
            &col2,
            &col3,
            col4.as_deref(),
            self.colors,
        ))
    }

    /// Update the overall/total bar.
    fn update_overall_bar(&self) {
        if let Some(ref bar) = self.overall_bar {
            let items_done = self
                .state
                .completed
                .iter()
                .filter(|r| matches!(r, riptide_progress::CompletedRecord::Item { .. }))
                .count() as u64;
            let groups_done = self
                .state
                .completed
                .iter()
                .filter(|r| matches!(r, riptide_progress::CompletedRecord::Group { .. }))
                .count() as u64;
            let rate = self.state.aggregate_rate();
            let msg = assemble_total_row(
                &self.state.operation.kind,
                groups_done,
                self.state.operation.total_groups,
                items_done,
                self.state.operation.total_items,
                rate,
                self.layout,
                self.colors,
            );
            bar.set_message(msg);
        }
    }
}

impl ProgressSink for TuiSink {
    #[allow(clippy::too_many_lines)]
    fn emit(&self, event: DownloadEvent) {
        let mut inner = self.lock();

        match &event {
            DownloadEvent::ItemQueued { .. } => {
                // Reduce state only; bar created on first non-Queued phase.
            }

            DownloadEvent::ItemQualityResolved { item_id, .. } => {
                let item_id = *item_id;
                let _ = inner.state.reduce(event);
                if let Some(msg) = inner.rebuild_item_message(item_id) {
                    if let Some(bar) = inner.item_bars.get(&item_id) {
                        bar.set_message(msg);
                    }
                }
                return;
            }

            DownloadEvent::ItemPhaseChanged { item_id, phase } => {
                let item_id = *item_id;
                let is_queued = matches!(phase, DownloadPhase::Queued);
                let _ = inner.state.reduce(event);

                if !is_queued && !inner.item_bars.contains_key(&item_id) {
                    // Create bar for newly-active item, inserted before the bottom border.
                    let bar = inner
                        .multi
                        .insert_before(&inner.bottom_border_bar, ProgressBar::new(0));
                    bar.set_style(
                        ProgressStyle::with_template("{msg}")
                            .unwrap_or_else(|_| ProgressStyle::default_bar()),
                    );
                    inner.item_bars.insert(item_id, bar);
                }

                inner.spinner_frame = inner.spinner_frame.wrapping_add(1);

                if let Some(msg) = inner.rebuild_item_message(item_id) {
                    if let Some(bar) = inner.item_bars.get(&item_id) {
                        bar.set_message(msg);
                    }
                }
                return;
            }

            DownloadEvent::TransferUpdate { item_id, .. } => {
                let item_id = *item_id;
                let _ = inner.state.reduce(event);
                if let Some(msg) = inner.rebuild_item_message(item_id) {
                    if let Some(bar) = inner.item_bars.get(&item_id) {
                        bar.set_message(msg);
                    }
                }
                return;
            }

            DownloadEvent::ItemFinished {
                item_id,
                outcome: _,
            } => {
                let item_id = *item_id;
                if let Some(bar) = inner.item_bars.remove(&item_id) {
                    bar.finish_and_clear();
                }
                let _ = inner.state.reduce(event);
                inner.update_overall_bar();
                return;
            }

            DownloadEvent::OperationTotalsUpdated { total_items, .. } => {
                if let Some(n) = total_items {
                    if let Some(ref bar) = inner.overall_bar {
                        bar.set_length(*n);
                    }
                }
                let _ = inner.state.reduce(event);
                inner.update_overall_bar();
                return;
            }

            DownloadEvent::OperationFinished { .. } => {
                let elapsed_secs = inner.start.elapsed().as_secs();
                let elapsed =
                    time::Duration::new(i64::try_from(elapsed_secs).unwrap_or(i64::MAX), 0);
                let _ = inner.state.reduce(event);
                let summary = inner.state.summary(Some(elapsed));

                // Finish all bars so indicatif releases them, then clear.
                inner.header_bar.finish_and_clear();
                inner.top_border_bar.finish_and_clear();
                if let Some(ref bar) = inner.overall_bar {
                    bar.finish_and_clear();
                }
                if let Some(ref bar) = inner.mid_border_bar {
                    bar.finish_and_clear();
                }
                for (_, bar) in inner.item_bars.drain() {
                    bar.finish_and_clear();
                }
                inner.bottom_border_bar.finish_and_clear();
                let _ = inner.multi.clear();

                // Print the receipt.
                let receipt = format_receipt(
                    &summary,
                    &inner.state.operation.kind,
                    &inner.state.operation.label,
                    &inner.output_root,
                    inner.show_failures,
                    inner.layout,
                    inner.colors,
                );
                eprintln!("{receipt}");
                return;
            }

            // Group events: reduce state only (no group bars in TUI sink).
            DownloadEvent::GroupStarted { .. }
            | DownloadEvent::GroupFinished { .. }
            | DownloadEvent::GroupTotalsUpdated { .. } => {}

            // Artifacts: silent during download.
            DownloadEvent::ArtifactPhaseChanged { .. } | DownloadEvent::ArtifactFinished { .. } => {
            }

            // Output path: state only.
            DownloadEvent::ItemOutputResolved { .. } => {}

            // Forward-compatible wildcard for future event variants.
            _ => {}
        }

        let _ = inner.state.reduce(event);
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_bar_at_zero_percent() {
        let bar = format_progress_bar(0.0);
        assert_eq!(visual_width(&bar), COL3_WIDTH);
        assert!(bar.contains(" 0%"));
        assert!(bar.starts_with("[ "));
        // All empty chars
        assert_eq!(bar.matches('\u{25B0}').count(), 0);
        assert_eq!(bar.matches('\u{25B1}').count(), BAR_CHARS);
    }

    #[test]
    fn progress_bar_at_fifty_percent() {
        let bar = format_progress_bar(50.0);
        assert_eq!(visual_width(&bar), COL3_WIDTH);
        assert!(bar.contains("50%"));
        assert_eq!(bar.matches('\u{25B0}').count(), 10);
        assert_eq!(bar.matches('\u{25B1}').count(), 10);
    }

    #[test]
    fn progress_bar_at_hundred_percent() {
        let bar = format_progress_bar(100.0);
        assert_eq!(visual_width(&bar), COL3_WIDTH);
        assert!(bar.contains("100%"));
        assert_eq!(bar.matches('\u{25B0}').count(), BAR_CHARS);
        assert_eq!(bar.matches('\u{25B1}').count(), 0);
    }

    #[test]
    fn phase_label_is_padded_to_28_chars() {
        for phase in &[
            DownloadPhase::ResolvingMetadata,
            DownloadPhase::ResolvingStream,
            DownloadPhase::Tagging,
            DownloadPhase::Persisting,
            DownloadPhase::WritingCover,
            DownloadPhase::WritingPlaylist,
        ] {
            let text = format_phase(phase, 0);
            assert_eq!(
                visual_width(&text),
                COL3_WIDTH,
                "phase {:?} produced width {} instead of {COL3_WIDTH}",
                phase,
                visual_width(&text)
            );
        }
    }

    #[test]
    fn speed_format_right_aligned() {
        let speed = format_speed(5.0 * 1024.0 * 1024.0);
        assert_eq!(visual_width(&speed), COL4_WIDTH);
        assert!(speed.contains("5.0 MB/s"));
    }

    #[test]
    fn speed_format_null() {
        let speed = format_speed(0.0);
        assert_eq!(visual_width(&speed), COL4_WIDTH);
        assert!(speed.contains("-------"));
    }

    #[test]
    fn quality_format_unknown_placeholder() {
        let q = format_quality(None);
        assert_eq!(visual_width(&q), COL1_WIDTH);
        assert!(q.starts_with("-------"));
    }

    #[test]
    fn quality_format_known_badge() {
        let badge = QualityBadge {
            symbol_kind: QualitySymbol::Lossless,
            text: "Lossless".to_owned(),
            detail: None,
        };
        let q = format_quality(Some(&badge));
        assert_eq!(visual_width(&q), COL1_WIDTH);
        assert!(q.contains('\u{25C7}')); // ◇
    }

    #[test]
    fn target_truncation_bracket_first() {
        let label = riptide_progress::DisplayLabel {
            title: "Track".to_owned(),
            artist: Some("Dream Theater".to_owned()),
            group: Some("A Very Long Album Name That Should Be Truncated".to_owned()),
            track_number: None,
        };
        let result = format_target(&label, &OperationKind::Artist, 30);
        assert_eq!(visual_width(&result), 30);
        // Bracket content should be truncated, not title.
        assert!(result.contains("Track"));
    }

    #[test]
    fn target_truncation_title_second() {
        let label = riptide_progress::DisplayLabel {
            title: "A Very Long Track Title That Will Need Truncation".to_owned(),
            artist: Some("Artist".to_owned()),
            group: Some("Album".to_owned()),
            track_number: None,
        };
        let result = format_target(&label, &OperationKind::Artist, 20);
        assert_eq!(visual_width(&result), 20);
        // Should contain ellipsis.
        assert!(result.contains('\u{2026}'));
    }

    #[test]
    fn elapsed_format_under_hour() {
        let d = time::Duration::new(312, 0); // 5:12
        assert_eq!(format_elapsed(&d), "05:12");
    }

    #[test]
    fn elapsed_format_over_hour() {
        let d = time::Duration::new(3723, 0); // 1:02:03
        assert_eq!(format_elapsed(&d), "1:02:03");
    }

    #[test]
    fn bytes_to_mb_conversion() {
        assert!((bytes_to_mb(1_000_000) - 1.0).abs() < f64::EPSILON);
        assert!((bytes_to_mb(1_500_000) - 1.5).abs() < f64::EPSILON);
        assert!((bytes_to_mb(0) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn format_border_width() {
        let border = format_border(44);
        assert_eq!(visual_width(&border), 44);
    }

    #[test]
    fn header_artist_mode() {
        let label = riptide_progress::DisplayLabel {
            title: "Foo Fighters".to_owned(),
            artist: Some("Foo Fighters".to_owned()),
            group: None,
            track_number: None,
        };
        let header = format_header(&OperationKind::Artist, &label, true);
        assert!(header.contains("\u{2605}")); // ★
        assert!(header.contains("Foo Fighters"));
        assert!(header.starts_with("Downloading:"));
    }

    #[test]
    fn header_album_mode() {
        let label = riptide_progress::DisplayLabel {
            title: "Images and Words".to_owned(),
            artist: Some("Dream Theater".to_owned()),
            group: None,
            track_number: None,
        };
        let header = format_header(&OperationKind::Album, &label, true);
        assert!(header.contains("\u{25CE}")); // ◎
        assert!(header.contains("[Dream Theater]"));
        assert!(header.contains("Images and Words"));
    }

    #[test]
    fn layout_mode_detect_returns_valid() {
        // Just check it doesn't panic.
        let mode = LayoutMode::detect();
        assert!(mode.total_width() == 80 || mode.total_width() == 100);
    }

    #[test]
    fn total_col2_album_mode() {
        let col2 = format_total_col2(&OperationKind::Album, 0, None, 9, Some(14), 42);
        assert_eq!(visual_width(&col2), 42);
        assert!(col2.starts_with("[9 of 14] Tracks"));
    }

    #[test]
    fn total_col2_artist_mode() {
        let col2 = format_total_col2(&OperationKind::Artist, 1, Some(2), 22, Some(24), 42);
        assert_eq!(visual_width(&col2), 42);
        assert!(col2.starts_with("[1 of 2] Albums // [22 of 24] Tracks"));
    }

    #[test]
    fn total_col2_unknown_counts() {
        let col2 = format_total_col2(&OperationKind::Album, 0, None, 0, None, 42);
        assert_eq!(visual_width(&col2), 42);
        assert!(col2.starts_with("[0 of ?] Tracks"));
    }
}
