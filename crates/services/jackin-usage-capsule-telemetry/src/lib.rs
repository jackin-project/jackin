//! jackin-usage-capsule-telemetry: in-container telemetry lifecycle.
//!
//! **Architecture Invariant:** T3.
//! Entry point: [`init`] — claim Capsule identity and start export.
//!
//! OTLP export, bounded startup lifecycle, telemetry-level state, and
//! panic handling for the in-container Capsule daemon and multiplexer.

pub mod logging;
mod telemetry;

pub use telemetry::{FlushGuard, init, otlp_active, session_context, shutdown};
