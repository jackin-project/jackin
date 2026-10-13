// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn tls_client_key_errors_expose_only_the_bounded_signal_and_asset() {
    let certificate = tempfile::NamedTempFile::new().expect("temporary certificate");
    let config = config::TlsConfig {
        certificate: None,
        client_key: Some("/secret/tenant-client.key".to_owned()),
        client_certificate: Some(certificate.path().to_string_lossy().into_owned()),
    };
    let error = otlp_channel::validate_transport("https://collector:4317", &config)
        .expect_err("missing client key must fail")
        .to_string();
    assert_eq!(error, "OTLP client key is unavailable");
    assert!(!error.contains("/secret/tenant-client.key"));
    assert!(!error.contains("No such file"));
}

#[cfg(feature = "test-support")]
#[test]
fn conformance_wire_tls_paths_are_consumed_without_export() -> anyhow::Result<()> {
    let _lock = crate::DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let private_dir = tempfile::tempdir()?;
    let ca_path = private_dir.path().join("wire-private-tenant-ca.pem");
    let certificate_path = private_dir
        .path()
        .join("wire-private-tenant-client-certificate.pem");
    let key_path = private_dir
        .path()
        .join("wire-private-tenant-client-key.pem");
    let ca_pem = "wire-private-ca-material";
    let certificate_pem = "wire-private-client-certificate-material";
    let key_pem = "wire-private-client-key-material";
    std::fs::write(&ca_path, ca_pem)?;
    std::fs::write(&certificate_path, certificate_pem)?;
    std::fs::write(&key_path, key_pem)?;
    let config = config::TlsConfig {
        certificate: Some(ca_path.to_string_lossy().into_owned()),
        client_key: Some(key_path.to_string_lossy().into_owned()),
        client_certificate: Some(certificate_path.to_string_lossy().into_owned()),
    };

    let tls_error = otlp_channel::validate_transport("https://collector.invalid:4317", &config)
        .expect_err("invalid private TLS material must be consumed and rejected")
        .to_string();
    assert!(!tls_error.contains("wire-private"));

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let testbed = runtime.block_on(async { jackin_otlp_testbed::Testbed::start() })?;
    super::super::super::init_wire_test_export(
        &testbed.endpoint(),
        super::super::ServiceIdentity::HOST_ONE_SHOT,
    )?;
    let operation =
        jackin_telemetry::root_operation(&jackin_telemetry::operation::TELEMETRY_VALIDATE, &[])
            .map_err(|reason| anyhow::anyhow!("validation operation rejected: {reason:?}"))?;
    jackin_telemetry::emit_event(
        &jackin_telemetry::event::TELEMETRY_VALIDATE,
        jackin_telemetry::FieldSet::default(),
    )
    .map_err(|reason| anyhow::anyhow!("validation event rejected: {reason:?}"))?;
    jackin_telemetry::counter(&jackin_telemetry::metric::TELEMETRY_VALIDATE)
        .add(1, &[])
        .map_err(|reason| anyhow::anyhow!("validation metric rejected: {reason:?}"))?;
    operation.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);
    super::super::super::flush_wire_test_export()?;
    assert!(
        runtime.block_on(testbed.wait_for_all_signals(std::time::Duration::from_secs(2))),
        "TLS privacy fixture did not deliver all three signals"
    );

    let ca_path = ca_path.to_string_lossy();
    let certificate_path = certificate_path.to_string_lossy();
    let key_path = key_path.to_string_lossy();
    assert_eq!(
        testbed.prohibited_value_violations(&[
            ca_path.as_ref(),
            certificate_path.as_ref(),
            key_path.as_ref(),
            ca_pem,
            certificate_pem,
            key_pem,
        ]),
        Vec::<String>::new(),
        "private TLS path or credential material escaped onto the OTLP wire"
    );
    super::super::super::shutdown_capsule_tracing();
    Ok(())
}

#[test]
fn facade_event_exports_native_event_name_once() {
    let (export, subscriber) = test_layers(false, "unused");
    tracing::subscriber::with_default(subscriber, || {
        let attrs = [jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::SESSION_ID,
            value: jackin_telemetry::Value::Str("session-test"),
        }];
        jackin_telemetry::emit_event(
            &jackin_telemetry::event::SESSION_START,
            jackin_telemetry::FieldSet::new(&attrs, None),
        )
        .unwrap();
    });
    export.logger_provider.force_flush().unwrap();
    let logs = export.logs.get_emitted_logs().unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].record.event_name(), Some("session.start"));
}

#[test]
fn crash_event_exports_complete_bounded_private_shape() {
    use opentelemetry::logs::AnyValue;

    let (export, subscriber) = test_layers(false, "unused");
    let session = jackin_telemetry::identity::SessionGuard::claim(
        jackin_telemetry::identity::SessionKind::Console,
    )
    .expect("crash test session");
    let expected_session = session.context().current.to_string();
    tracing::subscriber::with_default(subscriber, || {
        let payload = format!("{} token=supersecret", "x".repeat(5_000));
        crate::run::emit_crash_message("host panic", &payload);
    });
    export.logger_provider.force_flush().unwrap();
    drop(session);

    let logs = export.logs.get_emitted_logs().unwrap();
    assert_eq!(logs.len(), 1);
    let record = &logs[0].record;
    assert_eq!(record.event_name(), Some("app.crash"));
    let crash_id = log_attribute(record, "app.crash.id")
        .and_then(|value| match value {
            AnyValue::String(value) => Some(value.as_str()),
            _ => None,
        })
        .expect("crash UUID");
    uuid::Uuid::parse_str(crash_id).expect("valid crash UUID");
    assert_eq!(
        log_attribute(record, "session.id"),
        Some(&AnyValue::String(expected_session.into()))
    );
    assert_eq!(
        log_attribute(record, "exception.type"),
        Some(&AnyValue::String("panic".into()))
    );
    let message = log_attribute(record, "exception.message")
        .and_then(|value| match value {
            AnyValue::String(value) => Some(value.as_str()),
            _ => None,
        })
        .expect("exception message");
    assert!(message.len() <= 4 * 1024);
    assert!(!message.contains("supersecret"));
    assert!(
        !record
            .attributes_iter()
            .any(|(key, _)| matches!(key.as_str(), "outcome" | "error.type"))
    );
}

#[test]
fn facade_redacts_then_utf8_truncates_body_and_exception_fields() {
    use opentelemetry::logs::AnyValue;

    let (export, subscriber) = test_layers(false, "unused");
    let sensitive = format!("token=supersecret {}", "🦀".repeat(2_000));
    let attrs = [
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::EXCEPTION_TYPE,
            value: jackin_telemetry::Value::Str("panic"),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::EXCEPTION_MESSAGE,
            value: jackin_telemetry::Value::Str(&sensitive),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::EXCEPTION_STACKTRACE,
            value: jackin_telemetry::Value::Str(&sensitive),
        },
    ];
    tracing::subscriber::with_default(subscriber, || {
        jackin_telemetry::emit_event(
            &jackin_telemetry::event::APP_CRASH,
            jackin_telemetry::FieldSet::new(&attrs, Some(&sensitive)),
        )
        .expect("oversized private crash fields are sanitized before validation");
    });
    export.logger_provider.force_flush().unwrap();

    let logs = export.logs.get_emitted_logs().unwrap();
    assert_eq!(
        logs.len(),
        1,
        "facade health after dropped sanitized event: {:?}",
        jackin_telemetry::facade_health()
    );
    let record = &logs[0].record;
    let body = match record.body() {
        Some(AnyValue::String(value)) => value.as_str(),
        other => panic!("expected string body, got {other:?}"),
    };
    for value in [
        body,
        log_attribute(record, "exception.message")
            .and_then(|value| match value {
                AnyValue::String(value) => Some(value.as_str()),
                _ => None,
            })
            .expect("exception message"),
        log_attribute(record, "exception.stacktrace")
            .and_then(|value| match value {
                AnyValue::String(value) => Some(value.as_str()),
                _ => None,
            })
            .expect("exception stacktrace"),
    ] {
        assert!(value.len() <= jackin_telemetry::limits::MAX_BODY_BYTES);
        assert!(value.is_char_boundary(value.len()));
        assert!(!value.contains("supersecret"));
    }
}

#[test]
fn jank_event_exports_once_per_active_crossing() {
    use opentelemetry::logs::AnyValue;

    let (export, subscriber) = test_layers(false, "unused");
    tracing::subscriber::with_default(subscriber, || {
        let mut monitor = jackin_telemetry::ui::JankMonitor::default();
        monitor.record_frame(
            jackin_telemetry::schema::enums::ScreenId::WorkspaceList,
            0.101,
        );
        monitor.record_frame(
            jackin_telemetry::schema::enums::ScreenId::WorkspaceList,
            0.150,
        );
    });
    export.logger_provider.force_flush().unwrap();

    let logs = export.logs.get_emitted_logs().unwrap();
    assert_eq!(logs.len(), 1);
    let record = &logs[0].record;
    assert_eq!(record.event_name(), Some("app.jank"));
    assert_eq!(
        log_attribute(record, "app.jank.frame_count"),
        Some(&AnyValue::Int(1))
    );
    assert_eq!(
        log_attribute(record, "app.jank.period"),
        Some(&AnyValue::Double(1.0))
    );
    assert_eq!(
        log_attribute(record, "app.jank.threshold"),
        Some(&AnyValue::Double(0.1))
    );
    assert_eq!(record.attributes_iter().count(), 3);
}

#[test]
fn screen_transition_correlates_old_and_new_lifecycle_logs() {
    use opentelemetry::logs::AnyValue;

    let (export, subscriber) = test_layers(false, "unused");
    tracing::subscriber::with_default(subscriber, || {
        let action_attrs = [jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::UI_ACTION_NAME,
            value: jackin_telemetry::Value::Str("workspace.open"),
        }];
        jackin_telemetry::ui::remember_action_parent(
            jackin_telemetry::root_operation(
                &jackin_telemetry::operation::UI_ACTION,
                &action_attrs,
            )
            .unwrap(),
        );
        let parent = jackin_telemetry::ui::take_action_parent().expect("action parent");
        let mut tracker = jackin_telemetry::ui::ScreenVisitTracker::new();
        tracker
            .enter(jackin_telemetry::schema::enums::ScreenId::WorkspaceList)
            .unwrap();
        tracker
            .transition(
                jackin_telemetry::schema::enums::ScreenId::WorkspaceEditor,
                jackin_telemetry::schema::enums::TransitionReason::Action,
                Some(&parent),
            )
            .unwrap();
        drop(parent);
    });
    export.logger_provider.force_flush().unwrap();
    export.tracer_provider.force_flush().unwrap();

    let logs = export.logs.get_emitted_logs().unwrap();
    assert_eq!(logs.len(), 3);
    let spans = export.spans.get_finished_spans().unwrap();
    let transition = spans
        .iter()
        .find(|span| span.name == "ui.screen.transition")
        .expect("transition span");
    let entered = logs
        .iter()
        .filter(|log| log.record.event_name() == Some("ui.screen.entered"))
        .collect::<Vec<_>>();
    let exited = logs
        .iter()
        .find(|log| log.record.event_name() == Some("ui.screen.exited"))
        .expect("exited lifecycle log");
    assert_eq!(entered.len(), 2);
    assert_eq!(
        log_attribute(&entered[0].record, "app.screen.id"),
        Some(&AnyValue::String("workspace.list".into()))
    );
    assert_eq!(
        log_attribute(&entered[0].record, "app.screen.name"),
        Some(&AnyValue::String("workspace.list".into()))
    );
    assert_eq!(
        log_attribute(&exited.record, "app.screen.id"),
        Some(&AnyValue::String("workspace.list".into()))
    );
    assert_eq!(
        log_attribute(&exited.record, "app.screen.name"),
        Some(&AnyValue::String("workspace.list".into()))
    );
    assert_eq!(
        log_attribute(&entered[1].record, "app.screen.id"),
        Some(&AnyValue::String("workspace.editor".into()))
    );
    assert_eq!(
        log_attribute(&entered[1].record, "app.screen.name"),
        Some(&AnyValue::String("workspace.editor".into()))
    );
    assert_eq!(
        span_attribute(transition, "ui.transition.from_screen.id").as_deref(),
        Some("workspace.list")
    );
    assert_eq!(
        span_attribute(transition, "app.screen.name").as_deref(),
        Some("workspace.editor")
    );
    let mut transition_keys = transition
        .attributes
        .iter()
        .map(|attribute| attribute.key.as_str())
        .collect::<Vec<_>>();
    transition_keys.sort_unstable();
    assert_eq!(
        transition_keys,
        [
            "app.screen.id",
            "app.screen.name",
            "outcome",
            "ui.transition.from_screen.id",
            "ui.transition.reason",
        ]
    );
    for (log, sequence) in [
        (&entered[0].record, 1),
        (&exited.record, 2),
        (&entered[1].record, 3),
    ] {
        assert_eq!(
            log_attribute(log, "ui.navigation.sequence"),
            Some(&AnyValue::Int(sequence))
        );
    }
    let first_visit = log_attribute(&entered[0].record, "ui.screen.visit.id");
    assert_eq!(
        log_attribute(&exited.record, "ui.screen.visit.id"),
        first_visit
    );
    assert_ne!(
        log_attribute(&entered[1].record, "ui.screen.visit.id"),
        first_visit
    );
    assert_eq!(entered[0].record.attributes_iter().count(), 4);
    assert_eq!(exited.record.attributes_iter().count(), 5);
    assert_eq!(entered[1].record.attributes_iter().count(), 4);
    for log in [exited, entered[1]] {
        let context = log.record.trace_context().expect("transition log context");
        assert_eq!(context.span_id, transition.span_context.span_id());
        assert_eq!(context.trace_id, transition.span_context.trace_id());
    }
}
