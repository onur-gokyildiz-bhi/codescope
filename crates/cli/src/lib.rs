//! Codescope CLI library — exposes the clap `Cli` struct for testing
//! and for future programmatic use.

pub mod cli_def;

pub use cli_def::{Cli, Commands, HistoryAction};
