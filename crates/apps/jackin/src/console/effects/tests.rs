// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn skipped_instance_refresh_is_metric_only() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    tracing::subscriber::with_default(subscriber, record_skipped_instance_refresh);
    export.force_flush();
    assert!(export.finished_spans().is_empty());
}

#[test]
fn substantive_instance_refresh_is_one_autonomous_root() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    tracing::subscriber::with_default(subscriber, || {
        run_instance_refresh_cycle(|| Ok::<_, ()>(())).unwrap();
    });
    export.force_flush();

    let spans = export.finished_spans();
    assert_eq!(spans.len(), 1);
    assert_eq!(
        spans[0].name,
        jackin_telemetry::schema::spans::BACKGROUND_CYCLE
    );
    assert_eq!(spans[0].parent_span_id, "0000000000000000");
    assert!(!export.contains_span_text(jackin_telemetry::schema::attrs::CLI_INVOCATION_ID));
}

#[test]
fn failed_instance_refresh_marks_the_cycle_as_error() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    tracing::subscriber::with_default(subscriber, || {
        assert!(run_instance_refresh_cycle(|| Err::<(), _>(())).is_err());
    });
    export.force_flush();

    let spans = export.finished_spans();
    assert_eq!(spans.len(), 1);
    assert!(spans[0].error);
    assert!(export.contains_span_text("io_error"));
}
