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
use crate::observability::otel_keys;
use crate::redact::redact_text;
use crate::registry::{self, Outcome};

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
        span.set_attribute(otel_keys::COMPONENT, "host".to_owned());
        if let Some(run) = crate::active_run() {
            span.set_attribute(otel_keys::RUN_ID, run.run_id().to_owned());
        }
        for (key, value) in attrs {
            span.set_attribute(*key, value.clone());
        }
    }
    #[cfg(not(feature = "otlp"))]
    {
        // Attr attachment requires the OTLP OpenTelemetrySpanExt path.
        let _unused_without_otlp = attrs.len();
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

    #[cfg_attr(
        not(feature = "otlp"),
        expect(
            clippy::unused_self,
            reason = "body is otlp-gated; self used when otlp is on"
        )
    )]
    fn record_completion(&self, outcome: Outcome, error_type: Option<&'static str>) {
        #[cfg(feature = "otlp")]
        {
            use opentelemetry::trace::Status;
            use tracing_opentelemetry::OpenTelemetrySpanExt as _;
            self.span
                .set_attribute(otel_keys::EVENT_OUTCOME, outcome.as_str().to_owned());
            if let Some(error_type) = error_type {
                self.span
                    .set_attribute(otel_keys::ERROR_TYPE, error_type.to_owned());
            }
            if matches!(outcome, Outcome::Failure | Outcome::Timeout) {
                self.span.set_status(Status::error(outcome.as_str()));
            }
        }
        #[cfg(not(feature = "otlp"))]
        {
            let _ = (outcome, error_type);
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
            otel_keys::PROCESS_EXIT_CODE,
            i64::from(code),
        );
    }
}

/// Stamp caller attributes on the current span (dynamic fields cannot go through
/// the tracing event macro).
fn stamp_attrs_on_current(attrs: &[(&'static str, String)]) {
    #[cfg(feature = "otlp")]
    {
        use tracing_opentelemetry::OpenTelemetrySpanExt as _;
        let span = Span::current();
        for (key, value) in attrs {
            span.set_attribute(*key, value.clone());
        }
    }
    #[cfg(not(feature = "otlp"))]
    {
        let _unused_without_otlp = attrs.len();
    }
}

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
    event_name: &'static str,
    category: &'static str,
    body: &str,
    attrs: &[(&'static str, String)],
    outcome: Option<Outcome>,
) {
    let body = redact_text(body);
    let body = body.as_ref();

    let attr_refs: Vec<(&str, &str)> = attrs.iter().map(|(k, v)| (*k, v.as_str())).collect();
    // Fail-closed: log a compact line and skip structured emit on validation failure.
    if let Err(err) = registry::validate(event_name, &attr_refs, body) {
        // Unregistered free-form facade names (tests / pre-migration) still emit;
        // only hard-fail on prohibited keys / body policy.
        match err {
            registry::RegistryError::ProhibitedKey(_) | registry::RegistryError::BodyPolicy(_) => {
                emit_compact_line("error", &format!("telemetry registry rejected emit: {err}"));
                return;
            }
            _ => {}
        }
    }

    stamp_attrs_on_current(attrs);

    let default_outcome = match level {
        OperationLevel::Info | OperationLevel::Debug => Outcome::Success,
        // Warnings must never export success (contract).
        OperationLevel::Warn => Outcome::Cancelled,
        OperationLevel::Error => Outcome::Failure,
    };
    let outcome = outcome.unwrap_or(default_outcome);

    match level {
        OperationLevel::Info => {
            tracing::event!(
                target: OPERATION_TARGET,
                tracing::Level::INFO,
                "event.name" = event_name,
                "jackin.category" = category,
                "event.outcome" = outcome.as_str(),
                "{body}"
            );
            emit_compact_line(category, body);
        }
        OperationLevel::Debug => {
            tracing::event!(
                target: OPERATION_TARGET,
                tracing::Level::DEBUG,
                "event.name" = event_name,
                "jackin.category" = category,
                "event.outcome" = outcome.as_str(),
                "{body}"
            );
            emit_debug_line(category, body);
        }
        OperationLevel::Warn => {
            tracing::event!(
                target: OPERATION_TARGET,
                tracing::Level::WARN,
                "event.name" = event_name,
                "jackin.category" = category,
                "event.outcome" = outcome.as_str(),
                "{body}"
            );
            emit_compact_line(category, body);
        }
        OperationLevel::Error => {
            operation_error(event_name, "operation_error", body, attrs);
        }
    }
}

/// ERROR-severity structured event with registered `event_name` and `error.type`,
/// marks the current span Error, and mirrors a compact console line.
pub fn operation_error(
    event_name: &'static str,
    error_type: &'static str,
    body: &str,
    attrs: &[(&'static str, String)],
) {
    let body = redact_text(body);
    let body = body.as_ref();

    let mut attr_pairs: Vec<(&str, &str)> = attrs.iter().map(|(k, v)| (*k, v.as_str())).collect();
    attr_pairs.push(("error.type", error_type));
    if let Err(err) = registry::validate(event_name, &attr_pairs, body) {
        match err {
            registry::RegistryError::ProhibitedKey(_) | registry::RegistryError::BodyPolicy(_) => {
                emit_compact_line("error", &format!("telemetry registry rejected emit: {err}"));
                return;
            }
            _ => {}
        }
    }

    stamp_attrs_on_current(attrs);

    let category = registry::lookup(event_name).map_or("error", |d| d.category);

    tracing::event!(
        target: OPERATION_TARGET,
        tracing::Level::ERROR,
        "event.name" = event_name,
        "jackin.category" = category,
        "event.outcome" = Outcome::Failure.as_str(),
        "error.type" = error_type,
        "{body}"
    );

    #[cfg(feature = "otlp")]
    {
        use opentelemetry::trace::Status;
        use tracing_opentelemetry::OpenTelemetrySpanExt as _;
        Span::current().set_status(Status::error(body.to_owned()));
        Span::current().set_attribute(otel_keys::ERROR_TYPE, error_type.to_owned());
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

const _FACADE_KEYS: &[&str] = &[
    otel_keys::EVENT_NAME,
    otel_keys::CATEGORY,
    otel_keys::ERROR_TYPE,
    otel_keys::EVENT_OUTCOME,
];

#[cfg(test)]
mod tests;
