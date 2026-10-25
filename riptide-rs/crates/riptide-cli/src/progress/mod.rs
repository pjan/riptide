//! Terminal progress rendering for the download command.
//!
//! The `riptide-progress` crate owns the pure event model and state.
//! This module owns the imperative shell: TUI rendering, CLI option parsing,
//! and the [`MediaTransferProgress`] adapter.

pub mod options;
pub mod transfer;
pub mod tui_sink;

pub use options::ColorMode;
pub use transfer::MediaTransferProgress;
pub use tui_sink::TuiSink;
