//! jackin-runtime-process-telemetry: telemetry-instrumented process spawn/exec wrappers.
//!
//! **Architecture Invariant:** T1.
//! Entry point: [`process_telemetry::exec_async`] — async exec with OTLP span lifecycle.
//!
//! Wraps `jackin-process` spawn/exec entry points with `jackin-telemetry`
//! operation guards (outcome matrix + exit-code attrs). Split out of
//! `jackin-runtime` (S7 split 39); the old
//! `jackin_runtime::process_telemetry::*` paths keep working through a
//! `pub(crate)` re-export shim.

pub mod process_telemetry;
