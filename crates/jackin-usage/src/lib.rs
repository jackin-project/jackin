//! jackin-usage: usage totals, usage snapshot store, and agent handoff paths.
//!
//! **Architecture Invariant:** T3.
//! Entry point: [`UsageTotals`] — usage aggregation surface.

pub mod logging;
pub mod output;
/// Turso `SQLite` import chokepoint for this crate **and** host-binary usage
/// caches. External callers (host CLI) must open connections only through
/// [`store_backend::connect_local`] so a turso version bump stays one file.
pub mod store_backend;
pub mod telemetry;
pub mod token_monitor;
pub mod usage;
pub mod usage_snapshot_store;
