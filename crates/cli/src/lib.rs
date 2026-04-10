//! Codescope CLI library — exposes the clap `Cli` struct and command handlers
//! for testing and for future programmatic use.

pub mod cli_def;
pub mod commands;
pub mod db;

pub use cli_def::{Cli, Commands, HistoryAction};
