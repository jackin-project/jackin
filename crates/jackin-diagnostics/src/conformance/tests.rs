//! Dossier acceptance checks as permanent conformance tests (plan 044).

use opentelemetry::logs::{AnyValue, Severity};
use opentelemetry_sdk::logs::SdkLogRecord;
use opentelemetry_sdk::trace::SpanData;

use super::{MAX_DEBUG_LOGS, MAX_SPANS, drive_standard_scenario};

fn log_body(record: &SdkLogRecord) -> Option<String> {
    record.body().map(|value| match value {
        AnyValue::String(value) => value.to_string(),
        other => format!("{other:?}"),
    })
}

fn log_attr(record: &SdkLogRecord, key: &str) -> Option<String> {
    record
        .attributes_iter()
        .find(|(name, _)| name.as_str() == key)
        .map(|(_, value)| match value {
            AnyValue::String(value) => value.to_string(),
            other => format!("{other:?}"),
        })
}

#[test]
fn no_bracket_prefix_in_exported_bodies() {
    let _lock = crate::DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let export = drive_standard_scenario();
    let logs = export.logs.get_emitted_logs().unwrap();
    for log in &logs {
        if let Some(body) = log_body(&log.record) {
            assert!(
                !body.contains("[jackin debug") && !body.contains("[jackin-capsule"),
                "exported body must be prefix-free: {body}"
            );
        }
    }
}

#[test]
fn no_token_shaped_values_exported() {
    let _lock = crate::DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let export = drive_standard_scenario();
    let logs = export.logs.get_emitted_logs().unwrap();
    let dump = format!("{logs:?}");
    assert!(
        !dump.contains("abc123FAKE_not_a_real_secret"),
        "synthetic secret must not appear in export: {dump}"
    );
}

#[test]
fn forced_failure_groups() {
    let _lock = crate::DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let export = drive_standard_scenario();
    let logs = export.logs.get_emitted_logs().unwrap();
    let errors: Vec<_> = logs
        .iter()
        .filter(|log| log.record.severity_number() == Some(Severity::Error))
        .collect();
    assert!(
        !errors.is_empty(),
        "expected at least one ERROR log for forced failure"
    );
    let typed = errors.iter().find(|log| {
        log_attr(&log.record, "error_type").as_deref() == Some("conformance_error")
            || log_attr(&log.record, "error.type").as_deref() == Some("conformance_error")
    });
    assert!(typed.is_some(), "ERROR log must carry error.type");

    // Detach is not failure-shaped: taxonomy for session_detach is expected_shutdown.
    let detach = logs.iter().find(|log| {
        log_attr(&log.record, "kind").as_deref() == Some("session_detach")
    });
    assert!(detach.is_some(), "session_detach should be exported");
}

#[test]
fn waterfall_rows_distinct() {
    let _lock = crate::DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let export = drive_standard_scenario();
    let spans = export.spans.get_finished_spans().unwrap();
    let names: std::collections::BTreeSet<_> = spans.iter().map(|s| s.name.to_string()).collect();
    assert!(
        names.len() >= 3,
        "expected ≥3 distinct span names, got {names:?}"
    );
}

#[test]
fn logs_correlate_to_traces() {
    let _lock = crate::DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let export = drive_standard_scenario();
    let spans = export.spans.get_finished_spans().unwrap();
    let logs = export.logs.get_emitted_logs().unwrap();
    assert!(!spans.is_empty(), "scenario must export spans");
    // At least one log carries a span context matching an exported span.
    let correlated = logs.iter().filter_map(|log| log.record.trace_context()).count();
    assert!(
        correlated > 0,
        "expected some logs to carry span context while spans were active"
    );
}

#[test]
fn export_volume_budget() {
    let _lock = crate::DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let export = drive_standard_scenario();
    let logs = export.logs.get_emitted_logs().unwrap();
    let spans = export.spans.get_finished_spans().unwrap();
    // Metrics do not produce log rows; budgets guard residual firehose.
    assert!(
        logs.len() <= MAX_DEBUG_LOGS,
        "log count {} exceeds MAX_DEBUG_LOGS {MAX_DEBUG_LOGS}",
        logs.len()
    );
    assert!(
        spans.len() <= MAX_SPANS,
        "span count {} exceeds MAX_SPANS {MAX_SPANS}",
        spans.len()
    );
}

#[test]
fn screen_dimension_stamped() {
    let _lock = crate::DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let export = drive_standard_scenario();
    let spans = export.spans.get_finished_spans().unwrap();
    let screen_spans: Vec<_> = spans
        .iter()
        .filter(|s| {
            s.attributes
                .iter()
                .any(|kv| kv.key.as_str() == crate::otel_keys::SCREEN_NAME)
        })
        .collect();
    assert!(
        !screen_spans.is_empty(),
        "expected spans stamped with jackin.screen.name"
    );
}

#[test]
fn derived_image_stage_links_to_launch() {
    let _lock = crate::DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let export = drive_standard_scenario();
    let spans = export.spans.get_finished_spans().unwrap();
    let derived = spans
        .iter()
        .find(|s| s.name.as_ref() == "launch.derived_image")
        .expect("derived image stage span");
    assert!(
        !derived.links.is_empty(),
        "derived image stage should carry a link to the launch context: {derived:#?}"
    );
    let _: &SpanData = derived;
}
