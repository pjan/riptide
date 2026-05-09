//! Symbol resolution: maps [`RowSymbol`] to display characters.

use crate::progress::options::SymbolsMode;
use riptide_progress::RowSymbol;
use std::io::IsTerminal;

/// Resolved symbol set for a rendering session.
#[derive(Debug, Clone, Copy)]
pub struct SymbolSet {
    /// Active / downloading.
    pub active: &'static str,
    /// Queued / waiting.
    pub queued: &'static str,
    /// Non-transfer phase.
    pub phase: &'static str,
    /// Artifact write.
    pub artifact: &'static str,
    /// Completed successfully.
    pub done: &'static str,
    /// Skipped / exists.
    pub exists: &'static str,
    /// Failed.
    pub failed: &'static str,
}

const UNICODE: SymbolSet = SymbolSet {
    active: "●",
    queued: "○",
    phase: "◆",
    artifact: "◇",
    done: "✓",
    exists: "•",
    failed: "✕",
};

const ASCII: SymbolSet = SymbolSet {
    active: "*",
    queued: ".",
    phase: ">",
    artifact: ">",
    done: "v",
    exists: "=",
    failed: "!",
};

const NONE: SymbolSet = SymbolSet {
    active: "",
    queued: "",
    phase: "",
    artifact: "",
    done: "",
    exists: "",
    failed: "",
};

/// Resolves the symbol set to use based on `mode` and the current terminal.
pub fn resolve_symbols(mode: SymbolsMode) -> SymbolSet {
    match mode {
        SymbolsMode::Unicode => UNICODE,
        SymbolsMode::Ascii => ASCII,
        SymbolsMode::None => NONE,
        SymbolsMode::Auto => {
            if std::io::stderr().is_terminal() {
                UNICODE
            } else {
                ASCII
            }
        }
    }
}

/// Returns the display character for a [`RowSymbol`].
pub fn symbol_for(set: SymbolSet, symbol: RowSymbol) -> &'static str {
    match symbol {
        RowSymbol::Active => set.active,
        RowSymbol::Queued => set.queued,
        RowSymbol::Phase => set.phase,
        RowSymbol::Artifact => set.artifact,
        RowSymbol::Done => set.done,
        RowSymbol::Exists => set.exists,
        RowSymbol::Failed => set.failed,
        _ => set.phase, // forward-compatible wildcard
    }
}
