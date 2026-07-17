//! jackin-telemetry: schema authority and governed OpenTelemetry facade.
//!
//! **Architecture Invariant:** T0.
//! Entry point: [`OperationGuard`] — bounded operations backed by the closed registry.

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub mod cache;
mod error;
pub mod event;
pub mod health;
pub mod identity;
pub mod launch;
pub mod limits;
pub mod metric;
pub mod operation;
pub mod privacy;
pub mod process;
pub mod propagation;
mod retry;
pub mod schema;
pub mod spawn;
pub mod stream;
pub mod ui;
mod validation;

pub use error::{ResultTelemetryExt, record_error, record_recovered_degradation};
pub use event::{
    Attr, EventDef, FieldSet, Rejection, Severity, Value, emit_event, emit_event_display,
};
pub use health::{FacadeHealth, Signal, facade_health, record_export_rejection};
pub use metric::{
    Counter, Gauge, Histogram, InstrumentDef, InstrumentKind, MeterInstallError, MeterReservation,
    UpDownCounter, counter, gauge, histogram, install, reserve_meter, up_down_counter,
};
pub use operation::{
    OperationGuard, SpanDef, autonomous_cycle_operation, autonomous_root_operation, operation,
    operation_or_disabled, operation_with_remote_parent, root_operation,
};
pub use retry::record_retry_scheduled;

/// The only tracing target accepted for governed product telemetry.
pub const TELEMETRY_TARGET: &str = "jackin_telemetry";
