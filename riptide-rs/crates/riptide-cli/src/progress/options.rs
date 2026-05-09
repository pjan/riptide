//! CLI option types for progress rendering.

use clap::ValueEnum;

/// Controls when detail about output paths is shown on active rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum ProgressDetail {
    /// Show media title only (default).
    #[default]
    Title,
    /// Show output path instead of title.
    Path,
    /// Show both title and output path.
    Both,
}

/// Controls what completion lines are printed after a group finishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum CompletionDetail {
    /// Automatic: tracks for track/album/playlist, groups for artist.
    #[default]
    Auto,
    /// Always show group-level summary lines.
    Groups,
    /// Always show individual track lines.
    Tracks,
}

/// Controls Unicode symbol rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum SymbolsMode {
    /// Use Unicode symbols on interactive terminals; ASCII otherwise.
    #[default]
    Auto,
    /// Always use Unicode symbols.
    Unicode,
    /// Always use ASCII symbols.
    Ascii,
    /// No symbols; prefix column is blank.
    None,
}

/// Controls ANSI colour output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum ColorMode {
    /// Enable colours on interactive terminals; disable when redirected.
    #[default]
    Auto,
    /// Always enable colours.
    Always,
    /// Never enable colours.
    Never,
}
