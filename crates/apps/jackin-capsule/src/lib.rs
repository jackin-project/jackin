pub mod agent_status;
pub mod attach_context;
pub mod attach_protocol;
pub mod client;
mod client_writer;
mod clipboard;
pub mod config;
pub mod container_context;
pub mod daemon;
mod debug_panic;
pub mod exec;
pub mod exit_assess;
pub mod firewall;
pub mod git_context;
pub mod mcp_server;
pub mod output;
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

mod support {
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

pub mod tui;
pub mod wordlist;

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

#[cfg(test)]
mod tests;
