// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn result_error_helper_exports_one_typed_error_without_raw_value() {
    use jackin_telemetry::ResultTelemetryExt as _;
    use opentelemetry::logs::{AnyValue, Severity};

    #[derive(Debug)]
    struct PrivateError;

    impl std::fmt::Display for PrivateError {
        fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            panic!("telemetry formatted a private error")
        }
    }

    impl std::error::Error for PrivateError {}

    let (export, subscriber) = test_layers(false, "unused");
    tracing::subscriber::with_default(subscriber, || {
        let ok: Result<(), PrivateError> = Ok(());
        assert!(matches!(
            ok.record_telemetry_error(jackin_telemetry::schema::enums::ErrorType::DbError),
            Ok(())
        ));

        let error: Result<(), PrivateError> = Err(PrivateError);
        let owned = error
            .record_telemetry_error(jackin_telemetry::schema::enums::ErrorType::DbError)
            .unwrap_err();
        assert_eq!(
            owned.error_type(),
            jackin_telemetry::schema::enums::ErrorType::DbError
        );
        let reowned = Err::<(), _>(owned)
            .record_telemetry_error(jackin_telemetry::schema::enums::ErrorType::IoError)
            .unwrap_err();
        assert_eq!(
            reowned.error_type(),
            jackin_telemetry::schema::enums::ErrorType::DbError
        );
    });
    export.logger_provider.force_flush().unwrap();

    let logs = export.logs.get_emitted_logs().unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].record.event_name(), Some("error.typed"));
    assert_eq!(logs[0].record.severity_number(), Some(Severity::Error));
    assert_eq!(logs[0].record.body(), None);
    assert_eq!(
        log_attribute(&logs[0].record, "error.type"),
        Some(&AnyValue::String("db_error".into()))
    );
    assert_eq!(
        log_attribute(&logs[0].record, "outcome"),
        Some(&AnyValue::String("error".into()))
    );
}

#[test]
fn recovered_error_helper_exports_one_typed_warning_without_raw_value() {
    use opentelemetry::logs::{AnyValue, Severity};

    let (export, subscriber) = test_layers(false, "unused");
    tracing::subscriber::with_default(subscriber, || {
        jackin_telemetry::record_recovered_degradation().expect("recovered warning");
    });
    export.logger_provider.force_flush().unwrap();

    let logs = export.logs.get_emitted_logs().unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].record.event_name(), Some("operation.warn"));
    assert_eq!(logs[0].record.severity_number(), Some(Severity::Warn));
    assert_eq!(logs[0].record.body(), None);
    assert_eq!(
        log_attribute(&logs[0].record, "error.type"),
        Some(&AnyValue::String("recovered_degradation".into()))
    );
}

#[test]
fn detached_failure_automatically_exports_one_typed_error() {
    use opentelemetry::logs::AnyValue;

    let (export, subscriber) = test_layers(false, "unused");
    let default = tracing::subscriber::set_default(subscriber);
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime")
        .block_on(async {
            jackin_telemetry::spawn::spawn_detached(
                &jackin_telemetry::operation::PROCESS_COMMAND,
                async {},
                |()| {
                    jackin_telemetry::spawn::DetachedCompletion::failure(
                        jackin_telemetry::schema::enums::ErrorType::IoError,
                    )
                },
            )
            .await
            .expect("detached task");
        });
    drop(default);
    export.logger_provider.force_flush().unwrap();
    export.tracer_provider.force_flush().unwrap();

    let logs = export.logs.get_emitted_logs().unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].record.event_name(), Some("error.typed"));
    assert_eq!(logs[0].record.body(), None);
    assert_eq!(
        log_attribute(&logs[0].record, "error.type"),
        Some(&AnyValue::String("io_error".into()))
    );
    let spans = export.spans.get_finished_spans().unwrap();
    assert_eq!(spans.len(), 1);
    let log_context = logs[0].record.trace_context().expect("error trace context");
    assert_eq!(log_context.trace_id, spans[0].span_context.trace_id());
    assert_eq!(log_context.span_id, spans[0].span_context.span_id());
    assert!(spans[0].attributes.iter().any(|attribute| {
        attribute.key.as_str() == "error.type" && attribute.value.as_str() == "io_error"
    }));
}

#[test]
fn governed_event_level_gates_are_exact_and_do_not_infer_span_state() {
    use opentelemetry::trace::Status;

    for (level, expected_logs, expected_spans) in [
        ("error", 1usize, 0usize),
        ("warn", 2usize, 0usize),
        ("info", 3usize, 1usize),
        ("debug", 4usize, 1usize),
        ("trace", 5usize, 1usize),
    ] {
        let (export, subscriber) = test_layers_at(level, "unused");
        tracing::subscriber::with_default(subscriber, || {
            let operation = jackin_telemetry::operation(
                &jackin_telemetry::operation::CLI_COMMAND,
                &cli_command_test_attrs(),
            )
            .unwrap();
            let entered = operation.span().enter();
            emit_severity_matrix();
            drop(entered);
            operation.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);
        });
        export.logger_provider.force_flush().unwrap();
        export.tracer_provider.force_flush().unwrap();
        let logs = export.logs.get_emitted_logs().unwrap();
        let spans = export.spans.get_finished_spans().unwrap();
        assert_eq!(logs.len(), expected_logs, "{level} log gate");
        assert_eq!(spans.len(), expected_spans, "{level} span gate");
        for span in spans {
            assert!(span.events.is_empty(), "{level} duplicate span events");
            assert_eq!(span.status, Status::Unset, "{level} inferred status");
        }
    }
}

#[test]
fn in_memory_layers_hold_global_telemetry_lock_until_export_drop() {
    use std::sync::{TryLockError, mpsc};
    use std::time::Duration;

    let (export, subscriber) = test_layers(false, "first");
    let (contended_tx, contended_rx) = mpsc::channel();
    let (attempt_tx, attempt_rx) = mpsc::channel();
    let (acquired_tx, acquired_rx) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        // Directly observe contention before announcing readiness: a scheduled
        // worker or an elapsed timeout alone does not prove ownership.
        assert!(matches!(
            crate::DIAGNOSTICS_TEST_LOCK.try_lock(),
            Err(TryLockError::WouldBlock)
        ));
        contended_tx.send(()).expect("announce lock contention");
        attempt_rx.recv().expect("begin exporter admission");
        assert!(matches!(
            crate::DIAGNOSTICS_TEST_LOCK.try_lock(),
            Err(TryLockError::WouldBlock)
        ));
        contended_tx
            .send(())
            .expect("announce exporter lifetime contention");
        let (next_export, next_subscriber) = test_layers(false, "second");
        acquired_tx.send(()).expect("announce exporter admission");
        drop(next_subscriber);
        drop(next_export);
    });

    contended_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("worker must observe the held exporter lock");
    drop(subscriber);
    assert!(matches!(
        crate::DIAGNOSTICS_TEST_LOCK.try_lock(),
        Err(TryLockError::WouldBlock)
    ));
    attempt_tx.send(()).expect("begin waiting exporter");
    contended_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("worker must observe contention after subscriber destruction");
    assert!(matches!(
        acquired_rx.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));

    drop(export);
    acquired_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("export destruction must admit the waiting exporter");
    worker.join().expect("exporter contention worker");
}

#[test]
fn governed_unknown_names_and_forged_severity_are_rejected() {
    let (export, subscriber) = test_layers_at("trace", "unused");
    let before = jackin_telemetry::facade_health();
    tracing::subscriber::with_default(subscriber, || {
        tracing::event!(
            name: "unknown.governed.event",
            target: jackin_telemetry::TELEMETRY_TARGET,
            tracing::Level::INFO,
            {}
        );
        tracing::event!(
            name: "session.start",
            target: jackin_telemetry::TELEMETRY_TARGET,
            tracing::Level::WARN,
            {}
        );
        let span = tracing::info_span!(
            target: jackin_telemetry::TELEMETRY_TARGET,
            "unknown.governed.span"
        );
        drop(span);
        tracing::event!(
            name: "overlong.governed.event.xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
            target: jackin_telemetry::TELEMETRY_TARGET,
            tracing::Level::INFO,
            {}
        );
        let span = tracing::info_span!(
            target: jackin_telemetry::TELEMETRY_TARGET,
            "overlong.governed.span.xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
        );
        drop(span);
    });
    export.logger_provider.force_flush().unwrap();
    export.tracer_provider.force_flush().unwrap();
    assert!(export.logs.get_emitted_logs().unwrap().is_empty());
    assert!(export.spans.get_finished_spans().unwrap().is_empty());
    let after = jackin_telemetry::facade_health();
    assert!(after.unknown_name >= before.unknown_name + 2);
    assert!(after.invalid_value > before.invalid_value);
    assert!(after.size_limit >= before.size_limit + 2);
}

#[test]
fn governed_unknown_attribute_is_dropped() {
    let (export, subscriber) = test_layers(false, "unused");
    let before = jackin_telemetry::facade_health().unknown_attribute;
    tracing::subscriber::with_default(subscriber, || {
        tracing::event!(
            name: "session.start",
            target: jackin_telemetry::TELEMETRY_TARGET,
            tracing::Level::INFO,
            "bogus.secret" = "must-not-export"
        );
    });
    export.logger_provider.force_flush().unwrap();
    assert!(export.logs.get_emitted_logs().unwrap().is_empty());
    assert_eq!(
        jackin_telemetry::facade_health().unknown_attribute,
        before + 1
    );
}

#[test]
fn governed_second_line_drops_private_and_oversized_raw_records() {
    let (export, subscriber) = test_layers_at("trace", "unused");
    let before = jackin_telemetry::facade_health();
    let oversized = "x".repeat(jackin_telemetry::limits::MAX_STRING_ATTRIBUTE_BYTES + 1);
    tracing::subscriber::with_default(subscriber, || {
        tracing::event!(
            name: "app.crash",
            target: jackin_telemetry::TELEMETRY_TARGET,
            tracing::Level::ERROR,
            "exception.message" = "token=private-secret"
        );
        tracing::event!(
            name: "app.crash",
            target: jackin_telemetry::TELEMETRY_TARGET,
            tracing::Level::ERROR,
            "service.version" = oversized.as_str()
        );
        tracing::event!(
            name: "app.crash",
            target: jackin_telemetry::TELEMETRY_TARGET,
            tracing::Level::ERROR,
            "service.version" = true
        );
        drop(tracing::info_span!(
            target: jackin_telemetry::TELEMETRY_TARGET,
            "telemetry.validate",
            "session.id" = "/private/workspace"
        ));
        drop(tracing::info_span!(
            target: jackin_telemetry::TELEMETRY_TARGET,
            "telemetry.validate",
            "session.id" = oversized.as_str()
        ));
        drop(tracing::info_span!(
            target: jackin_telemetry::TELEMETRY_TARGET,
            "telemetry.validate",
            "session.id" = true
        ));
    });
    export.logger_provider.force_flush().unwrap();
    export.tracer_provider.force_flush().unwrap();

    assert!(export.logs.get_emitted_logs().unwrap().is_empty());
    assert!(export.spans.get_finished_spans().unwrap().is_empty());
    let after = jackin_telemetry::facade_health();
    for (signal, reason) in [
        (
            jackin_telemetry::Signal::Log,
            jackin_telemetry::Rejection::Privacy,
        ),
        (
            jackin_telemetry::Signal::Log,
            jackin_telemetry::Rejection::SizeLimit,
        ),
        (
            jackin_telemetry::Signal::Trace,
            jackin_telemetry::Rejection::Privacy,
        ),
        (
            jackin_telemetry::Signal::Trace,
            jackin_telemetry::Rejection::SizeLimit,
        ),
        (
            jackin_telemetry::Signal::Log,
            jackin_telemetry::Rejection::InvalidValue,
        ),
        (
            jackin_telemetry::Signal::Trace,
            jackin_telemetry::Rejection::InvalidValue,
        ),
    ] {
        assert_eq!(
            after.by_signal_reason[signal as usize][reason as usize],
            before.by_signal_reason[signal as usize][reason as usize] + 1,
            "missing second-line rejection for {signal:?}/{reason:?}; before={before:?} after={after:?}"
        );
    }
}
