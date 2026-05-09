//! Colour resolution for progress rendering.

use crate::progress::options::ColorMode;
use std::io::IsTerminal;

/// Whether ANSI colours are enabled for the current session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorEnabled(bool);

impl ColorEnabled {
    /// Returns `true` when colours should be emitted.
    pub fn is_on(self) -> bool {
        self.0
    }
}

/// Resolves whether colours are active based on `mode` and the terminal state.
pub fn resolve_color(mode: ColorMode) -> ColorEnabled {
    ColorEnabled(match mode {
        ColorMode::Always => true,
        ColorMode::Never => false,
        ColorMode::Auto => std::io::stderr().is_terminal(),
    })
}
