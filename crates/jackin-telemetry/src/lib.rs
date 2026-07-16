// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Schema authority for jackin❯ OpenTelemetry signals.
//!
//! Architecture invariant: this is a T0 crate with no jackin❯ crate
//! dependencies. Its extension registry is closed, generated from the Weaver
//! sources, and may never define `jackin.*` or `parallax.*` keys.

pub mod event;
pub mod health;
pub mod identity;
pub mod limits;
pub mod metric;
pub mod operation;
pub mod privacy;
pub mod propagation;
pub mod schema;
pub mod spawn;
pub mod ui;

pub use event::{Attr, EventDef, FieldSet, Rejection, Severity, Value, emit_event};
pub use health::{FacadeHealth, facade_health, record_export_rejection};
pub use metric::{
    Counter, Histogram, InstrumentDef, InstrumentKind, UpDownCounter, counter, histogram, install,
    up_down_counter,
};
pub use operation::{
    OperationGuard, SpanDef, operation, operation_or_disabled, operation_with_remote_parent,
    root_operation,
};

/// The only tracing target accepted for governed product telemetry.
pub const TELEMETRY_TARGET: &str = "jackin_telemetry";
