//! jackin-usage: usage totals, usage snapshot store, and agent handoff paths.
//!
//! **Architecture Invariant:** T4.
//! Entry point: [`UsageTotals`] — usage aggregation surface.
//! Host menu-bar / CLI: [`host::HostUsageRuntime`] (Capsule-free).

pub use jackin_usage_coordinator as coordinator;
pub mod host;
pub mod logging;
pub use jackin_usage_output as output;
/// Turso `SQLite` import chokepoint for this crate **and** host-binary usage
/// caches. External callers (host CLI) must open connections only through
/// [`store_backend::connect_local`] so a turso version bump stays one file.
pub use jackin_usage_store_backend as store_backend;
pub mod telemetry;
pub use jackin_usage_token_monitor as token_monitor;
pub mod usage;
pub use jackin_usage_snapshot_store as usage_snapshot_store;

#[cfg(test)]
mod tests;
