// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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
//! Dynamic attributes are stamped on the current span (tracing macros require
//! static field names). Registry validation runs fail-closed before emit.
//!
//! Operation / event names must come from the event registry
//! (`registry::lookup` / `otel_events`), never as inline literals at call sites.

use std::sync::atomic::{AtomicBool, Ordering};

use tracing::Span;

use crate::logging::{emit_compact_line, emit_debug_line};
use crate::redact::redact_text;
use crate::registry::Outcome;

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

    {
        use tracing_opentelemetry::OpenTelemetrySpanExt as _;
        for (key, value) in attrs {
            span.set_attribute(*key, value.clone());
        }
    }
    span
}

/// RAII handle for an operation span.
///
/// Holds only a [`Span`] so the guard is `Send` and can cross `.await` points
/// on multi-thread runtimes. Callers that need events under the span across
/// awaits must attach via [`tracing::Instrument`] (see `ShellRunner`); do not
/// store `EnteredSpan` here — it is `!Send`.
///
/// Call [`OperationGuard::complete`] with a registered outcome before drop.
/// Drop without completion records `cancelled` so `?`-exits never look like
/// success.
#[derive(Debug)]
pub struct OperationGuard {
    span: Span,
    completed: AtomicBool,
}

impl OperationGuard {
    /// The underlying span (for attribute stamping after start / Instrument).
    #[must_use]
    pub fn span(&self) -> &Span {
        &self.span
    }

    /// Record completion outcome (and optional `error.type`) then mark done.
    /// Drop after this is a no-op for outcome recording; the span ends normally.
    pub fn complete(self, outcome: Outcome, error_type: Option<&'static str>) {
        self.record_completion(outcome, error_type);
        self.completed.store(true, Ordering::SeqCst);
    }

    fn record_completion(&self, outcome: Outcome, error_type: Option<&'static str>) {
        {
            use opentelemetry::trace::Status;
            use tracing_opentelemetry::OpenTelemetrySpanExt as _;
            self.span.set_attribute(
                jackin_telemetry::schema::attrs::OUTCOME,
                outcome.as_str().to_owned(),
            );
            if let Some(error_type) = error_type {
                self.span.set_attribute(
                    jackin_telemetry::schema::attrs::std_attrs::ERROR_TYPE,
                    error_type.to_owned(),
                );
            }
            if matches!(outcome, Outcome::Failure | Outcome::Timeout) {
                self.span.set_status(Status::error(outcome.as_str()));
            }
        }
    }
}

impl Drop for OperationGuard {
    fn drop(&mut self) {
        if !self.completed.load(Ordering::SeqCst) {
            self.record_completion(Outcome::Cancelled, None);
        }
    }
}

/// Build an operation span handle. Prefer `.instrument(guard.span().clone())`
/// on async work so the span stays current across yields without `!Send` enters.
#[must_use]
pub fn enter_operation(name: &'static str, attrs: &[(&'static str, String)]) -> OperationGuard {
    OperationGuard {
        span: operation_span(name, attrs),
        completed: AtomicBool::new(false),
    }
}

/// Record `process.exit_code` on the current span when present.
pub fn operation_record_exit_code(code: Option<i32>) {
    if let Some(code) = code {
        operation_set_i64_attr(
            &Span::current(),
            jackin_telemetry::schema::attrs::std_attrs::PROCESS_EXIT_CODE,
            i64::from(code),
        );
    }
}

/// Stamp caller attributes on the current span (dynamic fields cannot go through
/// the tracing event macro).
/// Emit one structured log event (clean body + fixed schema fields) and mirror
/// a console/file line through the existing renderers.
///
/// `outcome` defaults from level when `None`: Info/Debug → success, Warn →
/// cancelled (never success), Error routes to [`operation_error`].
pub fn operation_log(
    level: OperationLevel,
    event_name: &'static str,
    category: &'static str,
    body: &str,
    attrs: &[(&'static str, String)],
) {
    operation_log_with_outcome(level, event_name, category, body, attrs, None);
}

/// Like [`operation_log`] but with an explicit outcome override.
pub fn operation_log_with_outcome(
    level: OperationLevel,
    _event_name: &'static str,
    category: &'static str,
    body: &str,
    _attrs: &[(&'static str, String)],
    outcome: Option<Outcome>,
) {
    let body = redact_text(body);
    let body = body.as_ref();

    let default_outcome = match level {
        OperationLevel::Info | OperationLevel::Debug => Outcome::Success,
        // Warnings must never export success (contract).
        OperationLevel::Warn => Outcome::Cancelled,
        OperationLevel::Error => Outcome::Failure,
    };
    let outcome = outcome.unwrap_or(default_outcome);

    let attrs = [jackin_telemetry::Attr {
        key: jackin_telemetry::schema::attrs::OUTCOME,
        value: jackin_telemetry::Value::Str(outcome.as_str()),
    }];
    let def = match level {
        OperationLevel::Info => &jackin_telemetry::event::OPERATION_LOG,
        OperationLevel::Debug => &jackin_telemetry::event::DEBUG_LINE,
        OperationLevel::Warn => &jackin_telemetry::event::OPERATION_WARN,
        OperationLevel::Error => &jackin_telemetry::event::ERROR_TYPED,
    };
    let _ = jackin_telemetry::emit_event(def, jackin_telemetry::FieldSet::new(&attrs, Some(body)));
    match level {
        OperationLevel::Debug => emit_debug_line(category, body),
        OperationLevel::Error => emit_compact_line("error", body),
        OperationLevel::Info | OperationLevel::Warn => emit_compact_line(category, body),
    }
}

/// ERROR-severity structured event with registered `event_name` and `error.type`,
/// marks the current span Error, and mirrors a compact console line.
pub fn operation_error(
    _event_name: &'static str,
    error_type: &'static str,
    body: &str,
    _attrs: &[(&'static str, String)],
) {
    let body = redact_text(body);
    let body = body.as_ref();

    let attrs = [
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::OUTCOME,
            value: jackin_telemetry::Value::Str("failure"),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::ERROR_TYPE,
            value: jackin_telemetry::Value::Str(error_type),
        },
    ];
    let _ = jackin_telemetry::emit_event(
        &jackin_telemetry::event::ERROR_TYPED,
        jackin_telemetry::FieldSet::new(&attrs, Some(body)),
    );

    {
        use opentelemetry::trace::Status;
        use tracing_opentelemetry::OpenTelemetrySpanExt as _;
        Span::current().set_status(Status::error(body.to_owned()));
        Span::current().set_attribute(
            jackin_telemetry::schema::attrs::std_attrs::ERROR_TYPE,
            error_type.to_owned(),
        );
    }

    emit_compact_line("error", body);
}

/// Record a u64 counter add for `name` when a meter provider is installed.
pub fn operation_metric(name: &'static str, value: u64, attrs: &[(&'static str, String)]) {
    {
        crate::observability::record_operation_metric(name, value, attrs);
    }
}

/// Stamp an i64 attribute on an existing operation span (OTLP builds only).
pub fn operation_set_i64_attr(span: &Span, key: &'static str, value: i64) {
    {
        use tracing_opentelemetry::OpenTelemetrySpanExt as _;
        span.set_attribute(key, value);
    }
}
