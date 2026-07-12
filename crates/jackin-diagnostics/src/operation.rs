//! Typed operation facade — the structured telemetry API for jackin❯.
//!
//! Console/file tiers still render through `emit_debug_line` / `emit_compact_line`
//! (bracket prefixes live only at that render boundary). Exported OTLP bodies are
//! the clean message with attributes as dimensions — never a baked-in prefix.
//!
//! Attribute rules: low-cardinality only. Full command strings, full URLs, raw
//! payloads, and container ids are forbidden as attrs — pass redacted or
//! summarized values only. Free-text `body` is redacted before emission.
//!
//! Operation / event names must come from the semconv registry
//! (`otel_events`, `otel_keys`), never as inline literals at call sites.

use tracing::Span;

use crate::logging::{emit_compact_line, emit_debug_line};
use crate::observability::otel_keys;
use crate::redact::redact_text;

const OPERATION_TARGET: &str = "jackin_diagnostics";

/// Severity for [`operation_log`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationLevel {
    Info,
    Debug,
    Warn,
    Error,
}

/// Build an operation span named `name` (a registry const) with stamped attrs.
#[must_use]
pub fn operation_span(name: &'static str, attrs: &[(&'static str, String)]) -> Span {
    let span = tracing::info_span!("operation", otel.name = name,);

    #[cfg(feature = "otlp")]
    {
        use tracing_opentelemetry::OpenTelemetrySpanExt as _;
        for (key, value) in attrs {
            span.set_attribute(*key, value.clone());
        }
    }
    #[cfg(not(feature = "otlp"))]
    {
        let _ = attrs;
    }

    span
}

/// RAII handle for an operation span.
///
/// Holds only a [`Span`] so the guard is `Send` and can cross `.await` points
/// on multi-thread runtimes. Callers that need events under the span across
/// awaits must attach via [`tracing::Instrument`] (see `ShellRunner`); do not
/// store `EnteredSpan` here — it is `!Send`.
#[derive(Debug)]
pub struct OperationGuard {
    span: Span,
}

impl OperationGuard {
    /// The underlying span (for attribute stamping after start / Instrument).
    #[must_use]
    pub fn span(&self) -> &Span {
        &self.span
    }
}

/// Build an operation span handle. Prefer `.instrument(guard.span().clone())`
/// on async work so the span stays current across yields without `!Send` enters.
#[must_use]
pub fn enter_operation(name: &'static str, attrs: &[(&'static str, String)]) -> OperationGuard {
    OperationGuard {
        span: operation_span(name, attrs),
    }
}

/// Record `process.exit_code` on the current span when present.
pub fn operation_record_exit_code(code: Option<i32>) {
    if let Some(code) = code {
        operation_set_i64_attr(
            &Span::current(),
            otel_keys::PROCESS_EXIT_CODE,
            i64::from(code),
        );
    }
}

/// Emit one structured log event (clean body + fixed schema fields) and mirror
/// a console/file line through the existing renderers.
pub fn operation_log(
    level: OperationLevel,
    event_name: &'static str,
    category: &'static str,
    body: &str,
    attrs: &[(&'static str, String)],
) {
    let body = redact_text(body);
    let body = body.as_ref();
    let _ = attrs;

    match level {
        OperationLevel::Info => {
            tracing::info!(
                target: OPERATION_TARGET,
                kind = "operation",
                "event.name" = event_name,
                "jackin.category" = category,
                "event.outcome" = "success",
                "{body}"
            );
            emit_compact_line(category, body);
        }
        OperationLevel::Debug => {
            tracing::debug!(
                target: OPERATION_TARGET,
                kind = "operation",
                "event.name" = event_name,
                "jackin.category" = category,
                "event.outcome" = "success",
                "{body}"
            );
            emit_debug_line(category, body);
        }
        OperationLevel::Warn => {
            tracing::warn!(
                target: OPERATION_TARGET,
                kind = "operation",
                "event.name" = event_name,
                "jackin.category" = category,
                "event.outcome" = "success",
                "{body}"
            );
            emit_compact_line(category, body);
        }
        OperationLevel::Error => {
            operation_error("operation_error", body, attrs);
        }
    }
}

/// ERROR-severity structured event with `error.type`, marks the current span
/// Error, and mirrors a compact console line.
pub fn operation_error(error_type: &'static str, body: &str, attrs: &[(&'static str, String)]) {
    let body = redact_text(body);
    let body = body.as_ref();
    let _ = attrs;

    tracing::error!(
        target: OPERATION_TARGET,
        kind = "operation_error",
        "event.name" = "error",
        "jackin.category" = "error",
        "event.outcome" = "failure",
        error_type = error_type,
        "{body}"
    );

    #[cfg(feature = "otlp")]
    {
        use opentelemetry::trace::Status;
        use tracing_opentelemetry::OpenTelemetrySpanExt as _;
        Span::current().set_status(Status::error(body.to_owned()));
    }

    emit_compact_line("error", body);
}

/// Record a u64 counter add for `name` when a meter provider is installed.
pub fn operation_metric(name: &'static str, value: u64, attrs: &[(&'static str, String)]) {
    #[cfg(feature = "otlp")]
    {
        crate::observability::record_operation_metric(name, value, attrs);
    }
    #[cfg(not(feature = "otlp"))]
    {
        let _ = (name, value, attrs);
    }
}

/// Stamp an i64 attribute on an existing operation span (OTLP builds only).
pub fn operation_set_i64_attr(span: &Span, key: &'static str, value: i64) {
    #[cfg(feature = "otlp")]
    {
        use tracing_opentelemetry::OpenTelemetrySpanExt as _;
        span.set_attribute(key, value);
    }
    #[cfg(not(feature = "otlp"))]
    {
        let _ = (span, key, value);
    }
}

#[allow(dead_code, reason = "documented residual allow; prefer expect when site is lint-true")]
const _FACADE_KEYS: &[&str] = &[
    otel_keys::EVENT_NAME,
    otel_keys::CATEGORY,
    otel_keys::ERROR_TYPE,
    otel_keys::EVENT_OUTCOME,
];

#[cfg(all(test, feature = "otlp"))]
mod tests;
