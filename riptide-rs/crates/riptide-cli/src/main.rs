//! Minimal riptide-rs command-line entry point.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::{fs, path::PathBuf};

/// riptide-rs command-line interface.
#[derive(Debug, Parser)]
#[command(version, about = "Rust implementation of riptide")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

/// Top-level commands.
#[derive(Debug, Subcommand)]
enum Command {
    /// Validate a configuration file.
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
}

/// Configuration commands.
#[derive(Debug, Subcommand)]
enum ConfigCommand {
    /// Check whether a config file parses and validates.
    Check {
        /// Path to the TOML config file. Required in this slice.
        #[arg(long)]
        path: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Config {
            command: ConfigCommand::Check { path },
        }) => check_config(path),
        None => Ok(()),
    }
}

fn check_config(path: PathBuf) -> Result<()> {
    let contents = fs::read_to_string(&path)
        .with_context(|| format!("failed to load config from {}", path.display()))?;
    riptide_config::ValidatedConfig::from_toml_str(&contents)
        .with_context(|| format!("failed to validate config from {}", path.display()))?;
    println!("config ok: {}", path.display());
    Ok(())
}
