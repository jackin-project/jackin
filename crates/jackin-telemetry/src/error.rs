// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Privacy-safe error capture at semantic ownership boundaries.

use crate::{Attr, FieldSet, Rejection, Value, emit_event, event, schema};

/// Record one typed product error without formatting or exporting the error value.
///
/// The operation owner remains responsible for completing its span with the same
/// error type. Keeping that responsibility explicit prevents an error handled by
/// an inner scope from incorrectly poisoning a successful outer operation.
pub fn record_error(error_type: schema::enums::ErrorType) -> Result<(), Rejection> {
    let attrs = [
        Attr {
            key: schema::attrs::OUTCOME,
            value: Value::Str(schema::enums::OutcomeValue::Error.as_str()),
        },
        Attr {
            key: schema::attrs::std_attrs::ERROR_TYPE,
            value: Value::Str(error_type.as_str()),
        },
    ];
    emit_event(&event::ERROR_TYPED, FieldSet::new(&attrs, None))
}

/// Record a handled failure that preserved the outer operation's success.
///
/// This is a governed warning without an error body. It must not be used for a
/// terminal failure; the semantic owner records those through [`record_error`].
pub fn record_recovered_degradation() -> Result<(), Rejection> {
    let attrs = [Attr {
        key: schema::attrs::OUTCOME,
        value: Value::Str(schema::enums::OutcomeValue::Success.as_str()),
    }];
    emit_event(&event::OPERATION_WARN, FieldSet::new(&attrs, None))
}

/// Records an `Err` as governed OpenTelemetry and returns the result unchanged.
///
/// This works for every error type and deliberately never requires `Display` or
/// `Debug`, so credentials, paths, payloads, and raw dependency errors cannot be
/// exported accidentally.
pub trait ResultTelemetryExt<T, E>: Sized {
    #[must_use]
    fn record_telemetry_error(self, error_type: schema::enums::ErrorType) -> Self;
}

impl<T, E> ResultTelemetryExt<T, E> for Result<T, E> {
    fn record_telemetry_error(self, error_type: schema::enums::ErrorType) -> Self {
        if self.is_err() {
            let _event_result = record_error(error_type);
        }
        self
    }
}

#[cfg(test)]
mod tests;
