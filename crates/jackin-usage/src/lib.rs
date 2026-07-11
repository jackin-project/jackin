//! jackin-usage: usage/pricing/telemetry + token monitors for the jackin-capsule daemon.
//!
//! Architecture Invariant: allowed inward dependencies are `jackin-core`,
//! `jackin-protocol`, `jackin-diagnostics`. No dependency on `jackin-capsule`
//! (would be circular), `jackin-tui`, `jackin-console`, or any presentation crate.
//!
//! Logging infrastructure (`logging`, `clog!`, `cdebug!`) lives here so both
//! this crate and `jackin-capsule` can use the macros without a circular dep.
//! `jackin-capsule` re-exports `logging` and the macros from this crate.

pub mod logging;
pub mod output;
pub(crate) mod store_backend;
pub mod telemetry;
pub mod telemetry_store;
pub mod token_monitor;
pub mod usage;
