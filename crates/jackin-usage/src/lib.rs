//! jackin-usage: usage totals, usage snapshot store, and agent handoff paths.
//!
//! **Architecture Invariant:** T3.
//! Entry point: [`UsageTotals`] — usage aggregation surface.
//! Host menu-bar / CLI: [`host::HostUsageRuntime`] (Capsule-free).

pub mod coordinator;
pub mod host;
pub mod logging;
pub mod output;
/// Private SQLite custody boundary. Host callers use typed store operations.
mod store_backend;
pub mod telemetry;
pub mod token_monitor;
pub mod usage;
pub mod usage_snapshot_store;

#[cfg(test)]
mod tests;
