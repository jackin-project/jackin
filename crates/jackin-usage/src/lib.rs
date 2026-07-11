//! jackin-usage: usage totals, telemetry store, and agent handoff paths.
//!
//! **Architecture Invariant:** T3.
//! Entry point: [`UsageTotals`] — usage aggregation surface.

pub mod logging;
pub mod output;
pub(crate) mod store_backend;
pub mod telemetry;
pub mod telemetry_store;
pub mod token_monitor;
pub mod usage;
