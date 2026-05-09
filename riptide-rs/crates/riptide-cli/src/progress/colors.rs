//! Colour resolution for progress rendering.
//!
//! Reserved for Phase 4 colour-aware rendering. Currently unused but wired
//! through the CLI options so the flag is accepted.

use crate::progress::options::ColorMode;
use std::io::IsTerminal;

/// Whether ANSI colours are enabled for the current session.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorEnabled(pub bool);

/// Resolves whether colours are active based on `mode` and the terminal state.
#[allow(dead_code)]
pub fn resolve_color(mode: ColorMode) -> ColorEnabled {
    ColorEnabled(match mode {
        ColorMode::Always => true,
        ColorMode::Never => false,
        ColorMode::Auto => std::io::stderr().is_terminal(),
    })
}
