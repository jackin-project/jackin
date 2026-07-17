// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Privacy-safe error capture at semantic ownership boundaries.

use crate::{Attr, FieldSet, Rejection, Value, emit_event, event, schema};

/// An error whose semantic owner has already emitted its typed telemetry.
///
/// The original error remains available through the standard error source
/// chain for operator-facing reporting. Re-owning this carrier is idempotent:
/// the first semantic owner and its bounded type remain authoritative.
pub struct TelemetryError {
    source: Box<dyn std::any::Any + Send + Sync>,
    display: fn(&dyn std::any::Any, &mut std::fmt::Formatter<'_>) -> std::fmt::Result,
    debug: fn(&dyn std::any::Any, &mut std::fmt::Formatter<'_>) -> std::fmt::Result,
    error_type: schema::enums::ErrorType,
}

impl TelemetryError {
    #[must_use]
    pub const fn error_type(&self) -> schema::enums::ErrorType {
        self.error_type
    }

    #[must_use]
    pub fn downcast_ref<E: 'static>(&self) -> Option<&E> {
        self.source.downcast_ref()
    }
}

impl std::fmt::Debug for TelemetryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        (self.debug)(self.source.as_ref(), formatter)
    }
}

impl std::fmt::Display for TelemetryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        (self.display)(self.source.as_ref(), formatter)
    }
}

impl std::error::Error for TelemetryError {}

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
/// This is a governed warning without an error body. Its fixed bounded error
/// type makes every recovered failure observable without formatting the source
/// error. It must not be used for a terminal failure; the semantic owner records
/// those through [`record_error`].
pub fn record_recovered_degradation() -> Result<(), Rejection> {
    let attrs = [
        Attr {
            key: schema::attrs::OUTCOME,
            value: Value::Str(schema::enums::OutcomeValue::Success.as_str()),
        },
        Attr {
            key: schema::attrs::std_attrs::ERROR_TYPE,
            value: Value::Str(schema::enums::ErrorType::RecoveredDegradation.as_str()),
        },
    ];
    emit_event(&event::OPERATION_WARN, FieldSet::new(&attrs, None))
}

/// Records an `Err` as governed OpenTelemetry and returns an owned carrier.
///
/// The source error is never formatted during telemetry capture. Applying this
/// method again to a propagated [`TelemetryError`] preserves the first owner
/// without emitting a duplicate event.
pub trait ResultTelemetryExt<T, E> {
    fn record_telemetry_error(
        self,
        error_type: schema::enums::ErrorType,
    ) -> Result<T, TelemetryError>;
}

impl<T, E> ResultTelemetryExt<T, E> for Result<T, E>
where
    E: std::fmt::Debug + std::fmt::Display + Send + Sync + 'static,
{
    fn record_telemetry_error(
        self,
        error_type: schema::enums::ErrorType,
    ) -> Result<T, TelemetryError> {
        self.map_err(|error| {
            let source: Box<dyn std::any::Any + Send + Sync> = Box::new(error);
            match source.downcast::<TelemetryError>() {
                Ok(owned) => *owned,
                Err(source) => {
                    let _event_result = record_error(error_type);
                    TelemetryError {
                        source,
                        display: display_source::<E>,
                        debug: debug_source::<E>,
                        error_type,
                    }
                }
            }
        })
    }
}

fn display_source<E: std::fmt::Display + 'static>(
    source: &dyn std::any::Any,
    formatter: &mut std::fmt::Formatter<'_>,
) -> std::fmt::Result {
    let Some(source) = source.downcast_ref::<E>() else {
        return Err(std::fmt::Error);
    };
    source.fmt(formatter)
}

fn debug_source<E: std::fmt::Debug + 'static>(
    source: &dyn std::any::Any,
    formatter: &mut std::fmt::Formatter<'_>,
) -> std::fmt::Result {
    let Some(source) = source.downcast_ref::<E>() else {
        return Err(std::fmt::Error);
    };
    source.fmt(formatter)
}

#[cfg(test)]
mod tests;
