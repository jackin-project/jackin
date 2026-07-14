#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::used_underscore_binding,
    reason = "operation conformance tests force-flush OTel providers"
)]
#![cfg(feature = "otlp")]

//! Export-shape tests for the typed operation facade.

use opentelemetry::logs::{AnyValue, Severity};
use opentelemetry::trace::Status;
use opentelemetry_sdk::logs::SdkLogRecord;
use opentelemetry_sdk::trace::SpanData;

use super::{
    OperationLevel, enter_operation, operation_error, operation_log, operation_log_with_outcome,
    operation_span,
};
use crate::logging::{begin_debug_buffering, drain_debug_buffer_for_test, set_debug_mode};
use crate::observability::otel_events;
use crate::observability::otel_keys;
use crate::observability::{TestExport, test_layers};
use crate::registry::Outcome;

fn log_attr(record: &SdkLogRecord, key: &str) -> Option<String> {
    record
        .attributes_iter()
        .find(|(name, _)| name.as_str() == key)
        .map(|(_, value)| any_value_to_string(value))
}

fn span_attr(span: &SpanData, key: &str) -> Option<String> {
    span.attributes
        .iter()
        .find(|kv| kv.key.as_str() == key)
        .map(|kv| kv.value.to_string())
}

fn any_value_to_string(value: &AnyValue) -> String {
    match value {
        AnyValue::String(value) => value.to_string(),
        AnyValue::Boolean(value) => value.to_string(),
        AnyValue::Int(value) => value.to_string(),
        AnyValue::Double(value) => value.to_string(),
        other => format!("{other:?}"),
    }
}

fn log_body(record: &SdkLogRecord) -> Option<String> {
    record.body().map(any_value_to_string)
}

fn export_after(debug: bool, run_id: &str, emit: impl FnOnce()) -> TestExport {
    let (export, subscriber) = test_layers(debug, run_id);
    tracing::subscriber::with_default(subscriber, emit);
    export.logger_provider.force_flush().unwrap();
    export.tracer_provider.force_flush().unwrap();
    export
}

#[test]
fn operation_log_export_body_is_prefix_free_with_attrs() {
    let _lock = crate::DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // Info level: hermetic under capsule-exported JACKIN_TELEMETRY_LEVEL=info.
    // Debug-tier export is covered by console-mirror + the error/span tests.
    let export = export_after(false, "op-log-run", || {
        let span = operation_span(otel_events::PROCESS_EXECUTE, &[]);
        let _entered = span.enter();
        operation_log(
            OperationLevel::Info,
            "process.execute",
            "process",
            "container inspected",
            &[(otel_keys::COMPONENT, "host".into())],
        );
    });
    let logs = export.logs.get_emitted_logs().unwrap();
    let record = logs
        .iter()
        .find(|log| log_body(&log.record).as_deref() == Some("container inspected"))
        .expect("clean-body log exported");

    let body = log_body(&record.record).expect("body");
    assert_eq!(body, "container inspected");
    assert!(
        !body.contains("[jackin debug"),
        "export body must not carry the console prefix: {body}"
    );
    assert_eq!(
        log_attr(&record.record, "event.name").as_deref(),
        Some("process.execute")
    );
    assert_eq!(
        log_attr(&record.record, "jackin.category").as_deref(),
        Some("process")
    );
    // Dynamic attrs are span-stamped (tracing macro static-field constraint).
    let spans = export.spans.get_finished_spans().unwrap();
    let span = spans
        .iter()
        .find(|s| s.name.as_ref() == otel_events::PROCESS_EXECUTE)
        .expect("process.execute span");
    assert_eq!(
        span_attr(span, otel_keys::COMPONENT).as_deref(),
        Some("host"),
        "caller attrs must survive onto the span"
    );
}

#[test]
fn operation_error_exports_error_severity_and_type() {
    let _lock = crate::DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let export = export_after(false, "op-err-run", || {
        let span = operation_span(otel_events::PROCESS_EXECUTE, &[]);
        let _entered = span.enter();
        operation_error(
            otel_events::PROCESS_EXECUTE,
            "process_spawn_error",
            "failed to spawn",
            &[],
        );
    });

    let logs = export.logs.get_emitted_logs().unwrap();
    let error = logs
        .iter()
        .find(|log| log.record.severity_number() == Some(Severity::Error))
        .expect("ERROR log exported");
    assert_eq!(
        log_attr(&error.record, "error.type").as_deref(),
        Some("process_spawn_error")
    );
    assert_eq!(log_attr(&error.record, "error_type"), None);
    assert_eq!(
        log_attr(&error.record, "event.name").as_deref(),
        Some(otel_events::PROCESS_EXECUTE)
    );

    let spans = export.spans.get_finished_spans().unwrap();
    let span = spans
        .iter()
        .find(|s| s.name.as_ref() == otel_events::PROCESS_EXECUTE)
        .expect("process.execute span");
    assert!(
        matches!(span.status, Status::Error { .. }),
        "current span should be marked Error: {:?}",
        span.status
    );
}

#[test]
fn operation_span_exports_otel_name_and_attrs() {
    let _lock = crate::DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let export = export_after(false, "op-span-run", || {
        let span = operation_span(
            otel_events::PROCESS_EXECUTE,
            &[
                (otel_keys::PROCESS_COMMAND, "echo".into()),
                (otel_keys::PROCESS_ARGS_REDACTED, "hello".into()),
            ],
        );
        let guard = span.enter();
        drop(guard);
    });

    let spans = export.spans.get_finished_spans().unwrap();
    let span = spans
        .iter()
        .find(|s| s.name.as_ref() == otel_events::PROCESS_EXECUTE)
        .expect("process.execute span exported");
    assert_eq!(
        span_attr(span, otel_keys::PROCESS_COMMAND).as_deref(),
        Some("echo")
    );
    assert_eq!(
        span_attr(span, otel_keys::PROCESS_ARGS_REDACTED).as_deref(),
        Some("hello")
    );
    assert_eq!(
        span_attr(span, otel_keys::COMPONENT).as_deref(),
        Some("host"),
        "component is a span attr, never Resource"
    );
}

#[test]
fn operation_span_stamps_run_id_from_active_run() {
    use jackin_core::JackinPaths;

    let _lock = crate::DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let export = export_after(false, "op-run-id", || {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        let run = crate::RunDiagnostics::start(&paths, false, "load").unwrap();
        let _guard = run.activate();
        let span = operation_span(otel_events::PROCESS_EXECUTE, &[]);
        drop(span.enter());
    });
    let spans = export.spans.get_finished_spans().unwrap();
    let span = spans
        .iter()
        .find(|s| s.name.as_ref() == otel_events::PROCESS_EXECUTE)
        .expect("span");
    assert!(
        span_attr(span, otel_keys::RUN_ID).is_some(),
        "parallax.run.id must be stamped from active run: {span:?}"
    );
    assert_eq!(
        span_attr(span, otel_keys::COMPONENT).as_deref(),
        Some("host")
    );
}

#[test]
fn operation_log_console_mirror_carries_prefix() {
    let _lock = crate::DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    set_debug_mode(true);
    begin_debug_buffering();

    operation_log(
        OperationLevel::Debug,
        "process.execute",
        "docker",
        "container inspected",
        &[],
    );

    let lines = drain_debug_buffer_for_test();
    assert!(
        lines
            .iter()
            .any(|line| line.contains("[jackin debug docker]")),
        "console mirror must carry the render-boundary prefix: {lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|line| line.contains("container inspected")),
        "console mirror must include the message body: {lines:?}"
    );
}

#[test]
fn operation_log_warn_outcome_is_not_success() {
    let _lock = crate::DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let export = export_after(false, "op-warn-run", || {
        operation_log(
            OperationLevel::Warn,
            "process.execute",
            "process",
            "slow command",
            &[],
        );
    });
    let logs = export.logs.get_emitted_logs().unwrap();
    let warn = logs
        .iter()
        .find(|log| log_body(&log.record).as_deref() == Some("slow command"))
        .expect("warn log");
    assert_ne!(
        log_attr(&warn.record, "event.outcome").as_deref(),
        Some("success")
    );
    assert_eq!(
        log_attr(&warn.record, "event.outcome").as_deref(),
        Some("cancelled")
    );
}

#[test]
fn operation_guard_drop_without_complete_records_cancelled() {
    let _lock = crate::DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let export = export_after(false, "op-cancel-run", || {
        let guard = enter_operation(otel_events::PROCESS_EXECUTE, &[]);
        drop(guard);
    });
    let spans = export.spans.get_finished_spans().unwrap();
    let span = spans
        .iter()
        .find(|s| s.name.as_ref() == otel_events::PROCESS_EXECUTE)
        .expect("span");
    assert_eq!(
        span_attr(span, otel_keys::EVENT_OUTCOME).as_deref(),
        Some("cancelled")
    );
}

#[test]
fn operation_guard_complete_records_outcome() {
    let _lock = crate::DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let export = export_after(false, "op-complete-run", || {
        let guard = enter_operation(otel_events::PROCESS_EXECUTE, &[]);
        guard.complete(Outcome::Success, None);
    });
    let spans = export.spans.get_finished_spans().unwrap();
    let span = spans
        .iter()
        .find(|s| s.name.as_ref() == otel_events::PROCESS_EXECUTE)
        .expect("span");
    assert_eq!(
        span_attr(span, otel_keys::EVENT_OUTCOME).as_deref(),
        Some("success")
    );
}

#[test]
fn operation_log_with_explicit_failure_outcome() {
    let _lock = crate::DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let export = export_after(false, "op-fail-out", || {
        operation_log_with_outcome(
            OperationLevel::Info,
            "process.execute",
            "process",
            "failed step",
            &[],
            Some(Outcome::Failure),
        );
    });
    let logs = export.logs.get_emitted_logs().unwrap();
    let log = logs
        .iter()
        .find(|log| log_body(&log.record).as_deref() == Some("failed step"))
        .expect("log");
    assert_eq!(
        log_attr(&log.record, "event.outcome").as_deref(),
        Some("failure")
    );
}
