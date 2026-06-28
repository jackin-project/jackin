//! Live docker-build output sink.
//!
//! Re-exported from `jackin-diagnostics`. The command runner
//! (`jackin-docker/src/shell_runner.rs`) and the launch cockpit
//! (`jackin-launch/src/tui/run.rs`) both consume this; lifting the
//! state into jackin-diagnostics breaks the old inverted dependency
//! `jackin-docker -> jackin-launch` (P2 cleanup).

pub use jackin_diagnostics::build_log::*;
