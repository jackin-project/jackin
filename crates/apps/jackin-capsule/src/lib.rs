//! jackin-capsule: in-container capsule daemon, sessions, and TUI.
//!
//! **Architecture Invariant:** T8.
//! Entry point: [`daemon`] — capsule daemon module the binary runs.

pub mod agent_status;
pub(crate) mod alloc_telemetry;
pub mod attach_context;
/// Library target so integration tests under `tests/` can exercise
/// the protocol, prefix-key parser, VT round-trips, and status bar
/// without spawning a PTY.
pub mod attach_protocol;
pub mod client;
pub(crate) mod client_writer;
pub(crate) mod clipboard;
pub mod config;
pub mod container_context;
pub mod daemon;
pub(crate) mod debug_panic;
pub mod exec;
pub mod exit_assess;
pub mod firewall;
pub mod git_context;
pub mod mcp_server;
pub mod output;
/// Shrink-only dhat allocation ceilings for the `perf` ratchet family.
pub mod perf_budgets;
pub mod pid1;
pub mod pr_context;
pub mod process_isolation;
mod process_telemetry;
pub mod protocol;
pub mod pull_request;
pub mod runtime_setup;
pub mod services;
pub mod session;
pub mod socket;
pub mod sudo_provision;
pub mod util;

pub(crate) mod support {
    #[cfg(test)]
    use std::sync::{Arc, OnceLock};
    #[cfg(test)]
    use tokio::sync::{Mutex, OwnedMutexGuard};

    #[cfg(test)]
    static TELEMETRY_TEST_LOCK: OnceLock<Arc<Mutex<()>>> = OnceLock::new();

    #[cfg(test)]
    fn telemetry_test_lock() -> Arc<Mutex<()>> {
        Arc::clone(TELEMETRY_TEST_LOCK.get_or_init(|| Arc::new(Mutex::new(()))))
    }

    #[cfg(test)]
    pub(crate) fn telemetry_test_guard() -> OwnedMutexGuard<()> {
        telemetry_test_lock().blocking_lock_owned()
    }

    #[cfg(test)]
    pub(crate) async fn telemetry_test_guard_async() -> OwnedMutexGuard<()> {
        telemetry_test_lock().lock_owned().await
    }
}

/// Terminal-rendering code — all UI paint/layout lives here.
pub mod tui;
pub mod wordlist;

// Telemetry-level state lives in jackin-usage.
pub mod logging {
    pub use jackin_usage::logging::*;
}
pub mod telemetry {
    pub use jackin_usage::telemetry::*;
}
pub mod token_monitor {
    pub use jackin_usage::token_monitor::*;
}
pub mod usage {
    pub use jackin_usage_provider_core::*;
}
