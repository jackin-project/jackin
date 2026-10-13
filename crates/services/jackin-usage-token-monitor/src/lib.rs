//! jackin-usage-token-monitor: per-session token-spend monitor.
//!
//! **Architecture Invariant:** T2.
//! Entry point: [`TokenMonitor`] — throttled session polling.
//!
//! Reads provider-specific local `JSONL` / `SQLite` files inside the container and
//! tracks per-session input/output/cache token totals (and cost, from the
//! provider stream or the static pricing table). Polled from the daemon state
//! tick (self-throttled to a 30s/60s cadence); totals are served on demand
//! through the `ClientMsg::TokenUsage` control reply.

pub mod amp;
pub mod claude;
pub mod codex;
pub mod kimi;
pub mod opencode;
pub mod pricing;

mod discover;
mod monitor;
mod record;
mod session;
mod status;
mod totals;

#[cfg(test)]
use jackin_core::Agent;
#[cfg(test)]
use jackin_telemetry::schema;

pub use discover::{
    PROVIDER_WALK_DEPTH, find_provider_files, json_u64, read_file_text, recompute_spend,
};
pub use monitor::TokenMonitor;
pub(crate) use record::record_token_usage;
#[cfg(test)]
pub(crate) use record::{provider_name, token_usage_delta};
pub use session::TokenSession;
pub(crate) use status::PollStatus;
pub use status::ProviderReadDegraded;
pub use totals::{PollReport, SpendAcc, TokenTotals};

#[cfg(test)]
mod tests;
