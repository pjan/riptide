//! Colour resolution for progress rendering.

use crate::progress::options::ColorMode;
use std::io::IsTerminal;

/// Whether ANSI colours are enabled for the current session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorEnabled(pub bool);

/// Resolves whether colours are active based on `mode` and the terminal state.
pub fn resolve_color(mode: ColorMode) -> ColorEnabled {
    ColorEnabled(match mode {
        ColorMode::Always => true,
        ColorMode::Never => false,
        ColorMode::Auto => std::io::stderr().is_terminal(),
    })
}
