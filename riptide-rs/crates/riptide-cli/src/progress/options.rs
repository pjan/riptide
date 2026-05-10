//! CLI option types for progress rendering.

use clap::ValueEnum;

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
