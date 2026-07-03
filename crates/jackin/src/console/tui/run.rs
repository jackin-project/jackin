//! Console TUI run entry.
//!
//! The public entry points stay here; the event-loop steps live in
//! `run/steps.rs` so the coordinator remains easy to scan.

mod steps;

pub use steps::{ConsoleRunOptions, run_console};
