// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Schema authority for jackin❯ OpenTelemetry signals.
//!
//! Architecture invariant: this is a T0 crate with no jackin❯ crate
//! dependencies. Its extension registry is closed, generated from the Weaver
//! sources, and may never define `jackin.*` or `parallax.*` keys.

pub mod event;
pub mod health;
pub mod limits;
pub mod metric;
pub mod operation;
pub mod privacy;
pub mod schema;
pub mod spawn;

pub use event::{Attr, EventDef, FieldSet, Rejection, Severity, Value, emit_event};
pub use health::{FacadeHealth, facade_health};
pub use metric::{Counter, Histogram, InstrumentDef, InstrumentKind, counter, histogram, install};
pub use operation::{OperationGuard, SpanDef, operation};

/// The only tracing target accepted for governed product telemetry.
pub const TELEMETRY_TARGET: &str = "jackin_telemetry";
