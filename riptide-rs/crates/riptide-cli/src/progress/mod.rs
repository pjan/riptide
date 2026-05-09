//! Terminal progress rendering for the download command.
//!
//! The `riptide-progress` crate owns the pure event model and state.
//! This module owns the imperative shell: indicatif bars, plain-text output,
//! CLI option parsing, and the [`MediaTransferProgress`] adapter.

pub mod colors;
pub mod indicatif_sink;
pub mod options;
pub mod plain_sink;
pub mod symbols;
pub mod transfer;

pub use indicatif_sink::IndicatifSink;
pub use options::{ColorMode, CompletionDetail, ProgressDetail, SymbolsMode};
pub use plain_sink::PlainSink;
pub use transfer::MediaTransferProgress;
