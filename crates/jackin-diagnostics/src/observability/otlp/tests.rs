use super::{
    build_resource_for, build_resource_for_sources, exporter_tls, flush_before, grpc_endpoint,
    install_observable_metrics, resolve_endpoint, runtime_creation_count, semantic_os_type,
    shutdown, unsupported_protocol,
};

#[test]
fn production_observable_callbacks_collect_promptly() {
    use opentelemetry::metrics::MeterProvider as _;
    use opentelemetry_sdk::metrics::{InMemoryMetricExporter, PeriodicReader, SdkMeterProvider};

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let exporter = InMemoryMetricExporter::default();
    let provider = SdkMeterProvider::builder()
        .with_reader(PeriodicReader::builder(exporter.clone()).build())
        .build();
    install_observable_metrics(
        &provider.meter("observable-callback-test"),
        Some(runtime.handle().clone()),
    );

    let expected = [
        "process.cpu.utilization",
        "process.memory.usage",
        "tokio.runtime.workers",
        "tokio.runtime.alive_tasks",
        "tokio.runtime.global_queue.depth",
    ];
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        exporter.reset();
        let started = std::time::Instant::now();
        provider.force_flush().unwrap();
        assert!(
            started.elapsed() < std::time::Duration::from_millis(500),
            "production observable callbacks exceeded their collection bound"
        );
        let names = exporter
            .get_finished_metrics()
            .unwrap()
            .iter()
            .flat_map(opentelemetry_sdk::metrics::data::ResourceMetrics::scope_metrics)
            .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics)
            .map(|metric| metric.name().to_owned())
            .collect::<Vec<_>>();
        if expected
            .iter()
            .all(|expected| names.iter().any(|name| name == expected))
        {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "production observable callbacks did not emit all metrics: {names:?}"
        );
        std::thread::park_timeout(std::time::Duration::from_millis(10));
    }
}

#[test]
fn grpc_endpoint_is_normalized_without_http_signal_paths() {
    assert_eq!(
        grpc_endpoint("http://127.0.0.1:4317///"),
        "http://127.0.0.1:4317"
    );
}

#[test]
fn endpoint_diagnostics_show_only_sanitized_authority() {
    assert_eq!(
        super::sanitized_authority("https://collector.example:4317/private/tenant"),
        Some("https://collector.example:4317".to_owned())
    );
}

#[test]
fn ordinary_https_enables_tls_without_custom_certificates() {
    assert!(
        exporter_tls(
            &super::super::config::TlsConfig::default(),
            "traces",
            "https://collector:4317",
            std::time::Duration::from_secs(1),
        )
        .expect("TLS config")
        .is_some()
    );
}

#[test]
fn expired_budget_skips_flush_work() {
    let called = std::sync::atomic::AtomicBool::new(false);
    let result = flush_before(std::time::Instant::now(), || {
        called.store(true, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    });
    assert!(result.is_err());
    assert!(!called.load(std::sync::atomic::Ordering::Relaxed));
}

#[test]
fn flush_timeout_returns_without_joining_hung_worker() {
    let started = std::time::Instant::now();
    let (release_tx, release_rx) = std::sync::mpsc::sync_channel(0);
    let task = super::FlushTask::spawn(move || {
        release_rx.recv().expect("release flush worker");
        Ok(())
    });
    let result = task.finish_before(started + std::time::Duration::from_millis(20));
    assert_eq!(result, Err("telemetry flush budget exhausted".to_owned()));
    assert!(started.elapsed() < std::time::Duration::from_millis(80));
    release_tx.send(()).expect("release flush worker");
    super::reap_flush_workers();
}

#[test]
fn validation_distinguishes_timeout_from_signal_failure() {
    let success = Ok(());
    let timeout = Err("telemetry flush budget exhausted".to_owned());
    let failure = Err("telemetry flush failed".to_owned());
    assert_eq!(
        super::validate_flush_results(&timeout, &success, &success),
        Err(super::super::ValidationFailure::Timeout)
    );
    assert_eq!(
        super::validate_flush_results(&success, &failure, &success),
        Err(super::super::ValidationFailure::Export("logs"))
    );
}

#[test]
fn provider_shutdown_order_is_tracer_logger_meter() {
    let _test_lock = super::super::health::TEST_STATE_LOCK
        .lock()
        .expect("health test lock");
    let (export, _subscriber) = super::test_layers(false, "unused");
    let meter = opentelemetry_sdk::metrics::SdkMeterProvider::builder().build();
    let generation = super::super::health::set_active_signals();
    let providers = super::OtlpProviders {
        tracer: export.tracer_provider,
        logger: export.logger_provider,
        meter,
        generation,
    };
    super::SHUTDOWN_ORDER.lock().expect("order lock").clear();
    assert!(
        providers.flush_and_shutdown(std::time::Instant::now() + std::time::Duration::from_secs(1))
    );
    assert_eq!(
        *super::SHUTDOWN_ORDER.lock().expect("order lock"),
        [
            "flush.tracer",
            "flush.logger",
            "flush.meter",
            "tracer",
            "logger",
            "meter"
        ]
    );
}

#[test]
fn resource_matrix_has_exact_allowlist_and_ignores_secret_env_injection() {
    let values = |resource: &opentelemetry_sdk::Resource| {
        resource
            .iter()
            .map(|(key, value)| (key.as_str().to_owned(), value.to_string()))
            .collect::<std::collections::HashMap<_, _>>()
    };
    let identities = [
        (
            super::super::ServiceIdentity::HOST_ONE_SHOT,
            "jackin",
            "one_shot",
        ),
        (
            super::super::ServiceIdentity::HOST_INTERACTIVE,
            "jackin",
            "interactive",
        ),
        (
            super::super::ServiceIdentity::DAEMON,
            "jackin-daemon",
            "daemon",
        ),
        (
            super::super::ServiceIdentity::CAPSULE,
            "jackin-capsule",
            "capsule",
        ),
        (
            super::super::ServiceIdentity::ROLE,
            "jackin-role",
            "one_shot",
        ),
    ];
    for (identity, service_name, app_mode) in identities {
        // Resource construction has no environment input. In particular, an
        // injected HOSTNAME/OTEL_RESOURCE_ATTRIBUTES cannot affect it.
        let cgroup_requests = std::sync::atomic::AtomicUsize::new(0);
        let cgroup = || {
            cgroup_requests.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Some("0::/docker/abcdef123456\n".to_owned())
        };
        let resource = values(&build_resource_for_sources(identity, &cgroup));
        assert_eq!(
            resource.get("service.namespace").map(String::as_str),
            Some("jackin")
        );
        assert_eq!(
            resource.get("service.name").map(String::as_str),
            Some(service_name)
        );
        assert_eq!(resource.get("app.mode").map(String::as_str), Some(app_mode));
        for required in [
            "service.version",
            "service.instance.id",
            "process.pid",
            "process.executable.name",
            "os.type",
            "process.runtime.name",
            "process.runtime.version",
        ] {
            assert!(
                resource
                    .get(required)
                    .is_some_and(|value| !value.is_empty()),
                "{identity:?}: {required}"
            );
        }
        assert_eq!(
            resource.get("process.runtime.name").map(String::as_str),
            Some("rust")
        );
        assert_eq!(
            resource.get("os.type").map(String::as_str),
            semantic_os_type(std::env::consts::OS)
        );
        for forbidden in [
            "cli.invocation.id",
            "session.id",
            "job.id",
            "parallax.run.id",
            "workspace.name",
            "container.name",
        ] {
            assert!(
                !resource.contains_key(forbidden),
                "{identity:?}: {forbidden}"
            );
        }
        let mut expected = std::collections::BTreeSet::from([
            "app.mode",
            "os.type",
            "process.executable.name",
            "process.pid",
            "process.runtime.name",
            "process.runtime.version",
            "service.instance.id",
            "service.name",
            "service.namespace",
            "service.version",
        ]);
        if sysinfo::System::long_os_version().is_some() {
            expected.insert("os.version");
        }
        if identity == super::super::ServiceIdentity::CAPSULE {
            expected.insert("container.id");
            assert_eq!(
                resource.get("container.id").map(String::as_str),
                Some("abcdef123456")
            );
            assert_eq!(
                cgroup_requests.load(std::sync::atomic::Ordering::Relaxed),
                1
            );
        } else {
            assert!(!resource.contains_key("container.id"));
            assert_eq!(
                cgroup_requests.load(std::sync::atomic::Ordering::Relaxed),
                0
            );
        }
        assert_eq!(
            resource
                .keys()
                .map(String::as_str)
                .collect::<std::collections::BTreeSet<_>>(),
            expected
        );
        assert!(resource.values().all(|value| {
            !value.contains("super-secret")
                && !value.contains("secret-service-name")
                && !value.contains("secret-id")
        }));
    }

    let first = values(&build_resource_for(
        super::super::ServiceIdentity::HOST_ONE_SHOT,
    ));
    let second = values(&build_resource_for(
        super::super::ServiceIdentity::HOST_ONE_SHOT,
    ));
    assert_eq!(
        first.get("service.instance.id"),
        second.get("service.instance.id")
    );
}

#[test]
fn target_os_names_map_to_exact_semantic_convention_values() {
    assert_eq!(semantic_os_type("macos"), Some("darwin"));
    assert_eq!(semantic_os_type("ios"), Some("darwin"));
    assert_eq!(semantic_os_type("android"), Some("linux"));
    assert_eq!(semantic_os_type("dragonfly"), Some("dragonflybsd"));
    assert_eq!(semantic_os_type("illumos"), Some("solaris"));
    assert_eq!(semantic_os_type("linux"), Some("linux"));
    assert_eq!(semantic_os_type("windows"), Some("windows"));
    assert_eq!(semantic_os_type("unsupported"), None);
}

#[test]
fn container_id_accepts_only_hex_runtime_ids() {
    assert_eq!(
        super::verified_container_id("abcdef123456"),
        Some("abcdef123456".to_owned())
    );
    assert_eq!(
        super::verified_container_id("ABCDEF123456"),
        Some("abcdef123456".to_owned())
    );
    assert_eq!(super::verified_container_id("named-capsule"), None);
    assert_eq!(super::verified_container_id("abc123"), None);
    assert_eq!(
        super::container_id_from_cgroup("0::/kubepods.slice/docker-ABCDEF1234567890.scope\n"),
        Some("abcdef1234567890".to_owned())
    );
    assert_eq!(
        super::container_id_from_cgroup("0::/user.slice/named-capsule.scope\n"),
        None
    );
}

#[test]
fn only_grpc_protocol_is_accepted() {
    assert!(!unsupported_protocol("grpc"));
    assert!(unsupported_protocol("http/protobuf"));
    assert!(unsupported_protocol("http/json"));
}

#[test]
fn empty_endpoint_disables_export() {
    assert_eq!(resolve_endpoint(None), None);
    assert_eq!(resolve_endpoint(Some(String::new())), None);
    assert_eq!(
        resolve_endpoint(Some("http://otel:4317".to_owned())),
        Some("http://otel:4317".to_owned())
    );
}

#[test]
fn disabled_configuration_creates_no_runtime_and_shutdown_is_idempotent() {
    let before = runtime_creation_count();
    let env = |_key: &str| None;
    assert_eq!(super::super::config::resolve_otlp_config(&env), Ok(None));
    shutdown();
    shutdown();
    assert_eq!(runtime_creation_count(), before);
}

#[test]
fn tls_file_errors_do_not_expose_configured_paths() {
    let config = super::super::config::TlsConfig {
        certificate: Some("/secret/tenant-ca.pem".to_owned()),
        client_key: None,
        client_certificate: None,
    };
    let error = exporter_tls(
        &config,
        "traces",
        "https://collector:4317",
        std::time::Duration::from_secs(1),
    )
    .expect_err("missing certificate must fail");
    let error = error.to_string();
    assert_eq!(error, "OTLP traces CA certificate is unavailable");
    assert!(!error.contains("/secret/tenant-ca.pem"));
    assert!(!error.contains("No such file"));
}

#[test]
fn tls_client_key_errors_expose_only_the_bounded_signal_and_asset() {
    let certificate = tempfile::NamedTempFile::new().expect("temporary certificate");
    let config = super::super::config::TlsConfig {
        certificate: None,
        client_key: Some("/secret/tenant-client.key".to_owned()),
        client_certificate: Some(certificate.path().to_string_lossy().into_owned()),
    };
    let error = exporter_tls(
        &config,
        "logs",
        "https://collector:4317",
        std::time::Duration::from_secs(1),
    )
    .expect_err("missing client key must fail")
    .to_string();
    assert_eq!(error, "OTLP logs client key is unavailable");
    assert!(!error.contains("/secret/tenant-client.key"));
    assert!(!error.contains("No such file"));
}

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
    let config = super::super::config::TlsConfig {
        certificate: Some(ca_path.to_string_lossy().into_owned()),
        client_key: Some(key_path.to_string_lossy().into_owned()),
        client_certificate: Some(certificate_path.to_string_lossy().into_owned()),
    };

    let tls = exporter_tls(
        &config,
        "traces",
        "https://collector.invalid:4317",
        std::time::Duration::from_secs(1),
    )?;
    assert!(
        tls.is_some(),
        "production TLS resolver ignored private files"
    );

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let testbed = runtime.block_on(async { jackin_otlp_testbed::Testbed::start() })?;
    super::super::init_wire_test_export(
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
    super::super::flush_wire_test_export()?;
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
    super::super::shutdown_capsule_tracing();
    Ok(())
}

#[test]
fn facade_event_exports_native_event_name_once() {
    let (export, subscriber) = super::test_layers(false, "unused");
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

    let _lock = crate::DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (export, subscriber) = super::test_layers(false, "unused");
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

    let _lock = crate::DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (export, subscriber) = super::test_layers(false, "unused");
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

    let _lock = crate::DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (export, subscriber) = super::test_layers(false, "unused");
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

    let (export, subscriber) = super::test_layers(false, "unused");
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
        log_attribute(&exited.record, "app.screen.id"),
        Some(&AnyValue::String("workspace.list".into()))
    );
    assert_eq!(
        log_attribute(&entered[1].record, "app.screen.id"),
        Some(&AnyValue::String("workspace.editor".into()))
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
    for log in [exited, entered[1]] {
        let context = log.record.trace_context().expect("transition log context");
        assert_eq!(context.span_id, transition.span_context.span_id());
        assert_eq!(context.trace_id, transition.span_context.trace_id());
    }
}

#[test]
fn isolation_events_export_exact_private_shape() {
    use jackin_telemetry::schema::enums::{DindMode, NetworkMode, WorkspaceIsolationMode};

    let _lock = crate::DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (export, subscriber) = super::test_layers(false, "unused");
    tracing::subscriber::with_default(subscriber, || {
        crate::operation::isolation_decision(
            WorkspaceIsolationMode::Worktree,
            NetworkMode::Allowlist,
            DindMode::Rootless,
        );
        crate::operation::isolation_firewall_failed(NetworkMode::Allowlist);
    });
    export.logger_provider.force_flush().unwrap();
    let logs = export.logs.get_emitted_logs().unwrap();
    assert_eq!(logs.len(), 2);

    let decision = logs
        .iter()
        .find(|log| log.record.event_name() == Some("isolation.decision"))
        .expect("decision event");
    let mut decision_keys = decision
        .record
        .attributes_iter()
        .map(|(key, _)| key.as_str())
        .collect::<Vec<_>>();
    decision_keys.sort_unstable();
    assert_eq!(
        decision_keys,
        [
            "dind.mode",
            "network.mode",
            "outcome",
            "workspace.isolation.mode"
        ]
    );

    let firewall = logs
        .iter()
        .find(|log| log.record.event_name() == Some("isolation.firewall.failed"))
        .expect("firewall event");
    let mut firewall_keys = firewall
        .record
        .attributes_iter()
        .map(|(key, _)| key.as_str())
        .collect::<Vec<_>>();
    firewall_keys.sort_unstable();
    assert_eq!(firewall_keys, ["error.type", "network.mode", "outcome"]);

    for log in &logs {
        assert!(log.record.body().is_none());
        assert!(!log.record.attributes_iter().any(|(key, _)| {
            ["path", "workspace", "role", "container", "host"]
                .iter()
                .any(|forbidden| key.as_str().contains(forbidden))
                && key.as_str() != "workspace.isolation.mode"
        }));
    }
}

fn log_attribute<'a>(
    record: &'a opentelemetry_sdk::logs::SdkLogRecord,
    name: &str,
) -> Option<&'a opentelemetry::logs::AnyValue> {
    record
        .attributes_iter()
        .find_map(|(key, value)| (key.as_str() == name).then_some(value))
}

fn cli_command_test_attrs() -> [jackin_telemetry::Attr<'static>; 2] {
    [
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::CLI_COMMAND_NAME,
            value: jackin_telemetry::Value::Str("diagnostics"),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::CLI_INVOCATION_ID,
            value: jackin_telemetry::Value::Str("invocation-test"),
        },
    ]
}

#[test]
fn conformance_single_delivery_preserves_native_shape() {
    use opentelemetry::logs::{AnyValue, Severity};
    use opentelemetry::trace::Status;

    let (export, subscriber) = super::test_layers(false, "unused");
    tracing::subscriber::with_default(subscriber, || {
        let operation = jackin_telemetry::operation(
            &jackin_telemetry::operation::CLI_COMMAND,
            &cli_command_test_attrs(),
        )
        .unwrap();
        let entered = operation.span().enter();
        let attrs = [
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::CONFIG_MIGRATION_STEP_COUNT,
                value: jackin_telemetry::Value::U64(3),
            },
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::CONFIG_OPERATION,
                value: jackin_telemetry::Value::Str("migrate"),
            },
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::CONFIG_SCHEMA_VERSION_FROM,
                value: jackin_telemetry::Value::Str("legacy"),
            },
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::CONFIG_SCHEMA_VERSION_TO,
                value: jackin_telemetry::Value::Str("v1alpha9"),
            },
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::CONFIG_SCOPE,
                value: jackin_telemetry::Value::Str("global"),
            },
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::OUTCOME,
                value: jackin_telemetry::Value::Str("success"),
            },
        ];
        jackin_telemetry::emit_event(
            &jackin_telemetry::event::CONFIG_OPERATION,
            jackin_telemetry::FieldSet::new(&attrs, Some("configuration migrated")),
        )
        .unwrap();
        drop(entered);
        operation.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);
    });
    export.logger_provider.force_flush().unwrap();
    export.tracer_provider.force_flush().unwrap();

    let logs = export.logs.get_emitted_logs().unwrap();
    let spans = export.spans.get_finished_spans().unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(spans.len(), 1);
    let log = &logs[0];
    let span = &spans[0];
    assert_eq!(log.record.event_name(), Some("config.operation"));
    assert_eq!(log.record.severity_number(), Some(Severity::Info));
    assert_eq!(
        log.record.body(),
        Some(&AnyValue::String("configuration migrated".into()))
    );
    assert_eq!(
        log_attribute(&log.record, "config.migration.step_count"),
        Some(&AnyValue::Int(3))
    );
    assert_eq!(
        log_attribute(&log.record, "config.operation"),
        Some(&AnyValue::String("migrate".into()))
    );
    assert_eq!(
        log_attribute(&log.record, "config.scope"),
        Some(&AnyValue::String("global".into()))
    );
    let trace = log.record.trace_context().expect("active log context");
    assert_eq!(trace.trace_id, span.span_context.trace_id());
    assert_eq!(trace.span_id, span.span_context.span_id());
    assert!(
        span.events.is_empty(),
        "log event must not become a span event"
    );
    assert_eq!(span.status, Status::Unset);

    let resource = log
        .resource
        .iter()
        .map(|(key, value)| (key.as_str(), value.to_string()))
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(
        resource.get("service.name").map(String::as_str),
        Some("jackin")
    );
    assert_eq!(
        resource.get("app.mode").map(String::as_str),
        Some("one_shot")
    );
    assert!(!resource.contains_key("session.id"));
    assert!(!resource.contains_key("cli.invocation.id"));
}

#[test]
fn registered_scalar_types_round_trip() {
    use opentelemetry::logs::AnyValue;

    let (export, subscriber) = super::test_layers(false, "unused");
    tracing::subscriber::with_default(subscriber, || {
        let jank = [
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::std_attrs::APP_JANK_FRAME_COUNT,
                value: jackin_telemetry::Value::U64(7),
            },
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::std_attrs::APP_JANK_PERIOD,
                value: jackin_telemetry::Value::F64(0.25),
            },
        ];
        jackin_telemetry::emit_event(
            &jackin_telemetry::event::APP_JANK,
            jackin_telemetry::FieldSet::new(&jank, None),
        )
        .unwrap();

        let agent = [
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::std_attrs::GEN_AI_AGENT_NAME,
                value: jackin_telemetry::Value::Str("codex"),
            },
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::AGENT_STATE,
                value: jackin_telemetry::Value::Str("working"),
            },
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::AGENT_STATUS_SOURCE,
                value: jackin_telemetry::Value::Str("reported"),
            },
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::AGENT_STATUS_CONFIDENCE,
                value: jackin_telemetry::Value::Str("authoritative"),
            },
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::AGENT_STATUS_STUCK,
                value: jackin_telemetry::Value::Bool(true),
            },
        ];
        jackin_telemetry::emit_event(
            &jackin_telemetry::event::AGENT_STATE_CHANGED,
            jackin_telemetry::FieldSet::new(&agent, None),
        )
        .unwrap();

        jackin_telemetry::emit_event(
            &jackin_telemetry::event::TELEMETRY_VALIDATE,
            jackin_telemetry::FieldSet::default(),
        )
        .unwrap();
    });
    export.logger_provider.force_flush().unwrap();
    let logs = export.logs.get_emitted_logs().unwrap();
    assert_eq!(logs.len(), 3);
    assert_eq!(
        log_attribute(&logs[0].record, "app.jank.frame_count"),
        Some(&AnyValue::Int(7))
    );
    assert_eq!(
        log_attribute(&logs[0].record, "app.jank.period"),
        Some(&AnyValue::Double(0.25))
    );
    assert_eq!(
        log_attribute(&logs[1].record, "agent.status.stuck"),
        Some(&AnyValue::Boolean(true))
    );
    assert_eq!(logs[2].record.event_name(), Some("telemetry.validate"));
}

#[test]
fn every_registered_event_round_trips_once_with_canonical_severity() {
    use jackin_telemetry::schema::{RequirementLevel, ValueType};
    use opentelemetry::logs::Severity;

    static ARRAY_VALUE: &[&str] = &["proof"];
    let (export, subscriber) = super::test_layers_at("trace", "unused");
    tracing::subscriber::with_default(subscriber, || {
        for name in jackin_telemetry::schema::events::ALL {
            let definition = jackin_telemetry::event::definition(name)
                .expect("every generated event must have a facade definition");
            let metadata = jackin_telemetry::schema::events::definition(name)
                .expect("every generated event must have metadata");
            let attrs = metadata
                .attributes
                .iter()
                .filter(|attribute| attribute.requirement == RequirementLevel::Required)
                .map(|attribute| jackin_telemetry::Attr {
                    key: attribute.name,
                    value: match attribute.value_type {
                        ValueType::String => jackin_telemetry::Value::Str(
                            attribute.allowed_values.first().copied().unwrap_or("proof"),
                        ),
                        ValueType::Boolean => jackin_telemetry::Value::Bool(true),
                        ValueType::Integer => jackin_telemetry::Value::I64(1),
                        ValueType::Double => jackin_telemetry::Value::F64(1.0),
                        ValueType::StringArray => jackin_telemetry::Value::StrArray(ARRAY_VALUE),
                    },
                })
                .collect::<Vec<_>>();
            jackin_telemetry::emit_event(definition, jackin_telemetry::FieldSet::new(&attrs, None))
                .unwrap_or_else(|reason| panic!("{name} fixture rejected: {reason:?}"));
        }
    });
    export.logger_provider.force_flush().unwrap();
    let logs = export.logs.get_emitted_logs().unwrap();
    assert_eq!(logs.len(), jackin_telemetry::schema::events::ALL.len());
    for name in jackin_telemetry::schema::events::ALL {
        let matching = logs
            .iter()
            .filter(|log| log.record.event_name() == Some(*name))
            .collect::<Vec<_>>();
        assert_eq!(matching.len(), 1, "{name} delivery count");
        let expected = match jackin_telemetry::event::canonical_severity(name).unwrap() {
            jackin_telemetry::event::Severity::Trace => Severity::Trace,
            jackin_telemetry::event::Severity::Debug => Severity::Debug,
            jackin_telemetry::event::Severity::Info => Severity::Info,
            jackin_telemetry::event::Severity::Warn => Severity::Warn,
            jackin_telemetry::event::Severity::Error => Severity::Error,
        };
        assert_eq!(
            matching[0].record.severity_number(),
            Some(expected),
            "{name}"
        );
    }
}

#[test]
fn governed_operation_line_does_not_duplicate_active_run_log() {
    let _lock = crate::DIAGNOSTICS_TEST_LOCK.lock().expect("test lock");
    let (export, subscriber) = super::test_layers(false, "unused");
    tracing::subscriber::with_default(subscriber, || {
        let directory = tempfile::tempdir().expect("temporary diagnostics directory");
        let paths = jackin_core::JackinPaths::for_tests(directory.path());
        let run = crate::RunDiagnostics::start(
            &paths,
            false,
            "status",
            crate::ServiceIdentity::HOST_ONE_SHOT,
        )
        .expect("diagnostics run");
        let _active = run.activate();
        export.logs.reset();
        let attrs = [jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::OUTCOME,
            value: jackin_telemetry::Value::Str("success"),
        }];
        jackin_telemetry::emit_event(
            &jackin_telemetry::event::OPERATION_LOG,
            jackin_telemetry::FieldSet::new(&attrs, Some("one delivery")),
        )
        .expect("registered operation log");
    });
    export.logger_provider.force_flush().unwrap();
    let logs = export.logs.get_emitted_logs().unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].record.event_name(), Some("operation.log"));
}

#[test]
fn result_error_helper_exports_one_typed_error_without_raw_value() {
    use jackin_telemetry::ResultTelemetryExt as _;
    use opentelemetry::logs::{AnyValue, Severity};

    struct PrivateError;

    let _lock = crate::DIAGNOSTICS_TEST_LOCK.lock().expect("test lock");
    let (export, subscriber) = super::test_layers(false, "unused");
    tracing::subscriber::with_default(subscriber, || {
        let ok: Result<(), PrivateError> = Ok(());
        assert!(matches!(
            ok.record_telemetry_error(jackin_telemetry::schema::enums::ErrorType::DbError),
            Ok(())
        ));

        let error: Result<(), PrivateError> = Err(PrivateError);
        assert!(matches!(
            error.record_telemetry_error(jackin_telemetry::schema::enums::ErrorType::DbError),
            Err(PrivateError)
        ));
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

    let _lock = crate::DIAGNOSTICS_TEST_LOCK.lock().expect("test lock");
    let (export, subscriber) = super::test_layers(false, "unused");
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

    let _lock = crate::DIAGNOSTICS_TEST_LOCK.lock().expect("test lock");
    let (export, subscriber) = super::test_layers(false, "unused");
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

fn emit_severity_matrix() {
    let outcome = [jackin_telemetry::Attr {
        key: jackin_telemetry::schema::attrs::OUTCOME,
        value: jackin_telemetry::Value::Str("success"),
    }];
    jackin_telemetry::emit_event(
        &jackin_telemetry::event::TIMING_STARTED,
        jackin_telemetry::FieldSet::new(&outcome, None),
    )
    .unwrap();
    let widget = [jackin_telemetry::Attr {
        key: jackin_telemetry::schema::attrs::std_attrs::APP_WIDGET_ID,
        value: jackin_telemetry::Value::Str("matrix.widget"),
    }];
    jackin_telemetry::emit_event(
        &jackin_telemetry::event::UI_WIDGET_FOCUSED,
        jackin_telemetry::FieldSet::new(&widget, None),
    )
    .unwrap();
    for def in [
        &jackin_telemetry::event::PTY_SPAWN,
        &jackin_telemetry::event::APP_JANK,
        &jackin_telemetry::event::APP_CRASH,
    ] {
        jackin_telemetry::emit_event(def, jackin_telemetry::FieldSet::default()).unwrap();
    }
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
        let (export, subscriber) = super::test_layers_at(level, "unused");
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
fn governed_unknown_names_and_forged_severity_are_rejected() {
    let before = jackin_telemetry::facade_health();
    let (export, subscriber) = super::test_layers_at("trace", "unused");
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
    let before = jackin_telemetry::facade_health().unknown_attribute;
    let (export, subscriber) = super::test_layers(false, "unused");
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
    let before = jackin_telemetry::facade_health();
    let (export, subscriber) = super::test_layers_at("trace", "unused");
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

#[test]
fn conformance_no_lifetime_spans() {
    use std::time::Duration;

    let (export, subscriber) = super::test_layers(false, "unused");
    let idle = Duration::from_millis(80);
    tracing::subscriber::with_default(subscriber, || {
        for (definition, emit_session_start) in [
            (&jackin_telemetry::operation::APP_STARTUP, false),
            (&jackin_telemetry::operation::CLI_COMMAND, true),
        ] {
            let operation = jackin_telemetry::root_operation(definition, &cli_command_test_attrs())
                .expect("bounded operation");
            let entered = operation.span().enter();
            if emit_session_start {
                let attrs = [
                    jackin_telemetry::Attr {
                        key: jackin_telemetry::schema::attrs::std_attrs::SESSION_ID,
                        value: jackin_telemetry::Value::Str("session-proof"),
                    },
                    jackin_telemetry::Attr {
                        key: jackin_telemetry::schema::attrs::CLI_INVOCATION_ID,
                        value: jackin_telemetry::Value::Str("invocation-test"),
                    },
                ];
                jackin_telemetry::emit_event(
                    &jackin_telemetry::event::SESSION_START,
                    jackin_telemetry::FieldSet::new(&attrs, None),
                )
                .unwrap();
            }
            drop(entered);
            operation.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);
        }

        std::thread::park_timeout(idle);

        let shutdown = jackin_telemetry::root_operation(
            &jackin_telemetry::operation::APP_SHUTDOWN,
            &cli_command_test_attrs(),
        )
        .expect("bounded shutdown");
        shutdown.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);
    });
    export.logger_provider.force_flush().unwrap();
    export.tracer_provider.force_flush().unwrap();

    let spans = export.spans.get_finished_spans().unwrap();
    assert_eq!(spans.len(), 3);
    assert!(spans.iter().all(|span| {
        !matches!(
            span.name.as_ref(),
            "process" | "invocation" | "session" | "console.session" | "capsule.session"
        )
    }));
    assert!(
        spans
            .iter()
            .all(|span| { span.end_time.duration_since(span.start_time).unwrap() < idle / 2 }),
        "no bounded operation may cover the idle session interval"
    );
    assert!(
        spans
            .iter()
            .all(|span| span.attributes.iter().any(|attribute| {
                attribute.key.as_str() == jackin_telemetry::schema::attrs::CLI_INVOCATION_ID
                    && attribute.value.as_str() == "invocation-test"
            }))
    );
    let logs = export.logs.get_emitted_logs().unwrap();
    let session_start = logs
        .iter()
        .find(|log| log.record.event_name() == Some("session.start"))
        .expect("in-session log");
    assert_eq!(
        log_attribute(
            &session_start.record,
            jackin_telemetry::schema::attrs::CLI_INVOCATION_ID,
        ),
        Some(&opentelemetry::logs::AnyValue::String(
            "invocation-test".into()
        ))
    );
    assert!(
        session_start
            .resource
            .get(&opentelemetry::Key::from_static_str(
                jackin_telemetry::schema::attrs::CLI_INVOCATION_ID,
            ))
            .is_none(),
        "provider Resource must not contain invocation identity"
    );
    assert_eq!(
        jackin_telemetry::counter(&jackin_telemetry::metric::TELEMETRY_VALIDATE).add(
            1,
            &[jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::CLI_INVOCATION_ID,
                value: jackin_telemetry::Value::Str("invocation-test"),
            }],
        ),
        Err(jackin_telemetry::Rejection::Cardinality)
    );
}

#[test]
fn metric_export_contract_rejects_names_shapes_and_dimensions() {
    use jackin_telemetry::Rejection;

    assert_eq!(
        super::metric_contract_fields("unknown.metric", "unknown", "1"),
        Err(Rejection::UnknownName)
    );
    assert_eq!(
        super::metric_contract_fields(
            jackin_telemetry::schema::metrics::UI_JANK,
            "forged description",
            "{crossing}",
        ),
        Err(Rejection::InvalidValue)
    );

    let requirements = jackin_telemetry::schema::metrics::UI_JANK_DEF.attributes;
    let valid = [opentelemetry::KeyValue::new(
        "app.screen.id",
        "workspace.list",
    )];
    assert_eq!(
        super::validate_metric_attributes(requirements, valid.iter()),
        Ok(())
    );
    let wrong_type = [opentelemetry::KeyValue::new("app.screen.id", true)];
    assert_eq!(
        super::validate_metric_attributes(requirements, wrong_type.iter()),
        Err(Rejection::InvalidValue)
    );
    let unknown = [opentelemetry::KeyValue::new("bogus.secret", "secret")];
    assert_eq!(
        super::validate_metric_attributes(requirements, unknown.iter()),
        Err(Rejection::UnknownAttribute)
    );
    let sensitive = [opentelemetry::KeyValue::new(
        "app.screen.id",
        "/private/workspace",
    )];
    assert_eq!(
        super::validate_metric_attributes(requirements, sensitive.iter()),
        Err(Rejection::Privacy)
    );
    let duplicate = [
        opentelemetry::KeyValue::new("app.screen.id", "workspace.list"),
        opentelemetry::KeyValue::new("app.screen.id", "workspace.list"),
    ];
    assert_eq!(
        super::validate_metric_attributes(requirements, duplicate.iter()),
        Err(Rejection::InvalidValue)
    );
    assert_eq!(
        super::validate_metric_attributes(requirements, std::iter::empty()),
        Err(Rejection::InvalidValue)
    );
    let excessive = (0..=jackin_telemetry::limits::MAX_METRIC_ATTRIBUTES)
        .map(|_| opentelemetry::KeyValue::new("app.screen.id", "workspace.list"))
        .collect::<Vec<_>>();
    assert_eq!(
        super::validate_metric_attributes(requirements, excessive.iter()),
        Err(Rejection::SizeLimit)
    );
    let oversized = [opentelemetry::KeyValue::new(
        "app.screen.id",
        "x".repeat(jackin_telemetry::limits::MAX_STRING_ATTRIBUTE_BYTES + 1),
    )];
    assert_eq!(
        super::validate_metric_attributes(requirements, oversized.iter()),
        Err(Rejection::SizeLimit)
    );
    assert_eq!(
        super::validate_metric_points(
            0..=jackin_telemetry::limits::MAX_CARDINALITY,
            |_| Vec::new(),
            &[],
        ),
        Err(Rejection::Cardinality)
    );
}

fn assert_raw_metric_batch_rejected(
    reason: jackin_telemetry::Rejection,
    record: impl FnOnce(&opentelemetry::metrics::Meter),
) {
    use opentelemetry::metrics::MeterProvider as _;
    use opentelemetry_sdk::metrics::{InMemoryMetricExporter, PeriodicReader, SdkMeterProvider};

    let before = jackin_telemetry::facade_health().by_signal_reason
        [jackin_telemetry::Signal::Metric as usize][reason as usize];
    let exporter = InMemoryMetricExporter::default();
    let provider = SdkMeterProvider::builder()
        .with_reader(
            PeriodicReader::builder(super::GovernedMetricExporter(exporter.clone())).build(),
        )
        .build();
    record(&provider.meter("jackin"));

    assert!(
        provider.force_flush().is_err(),
        "raw metric batch unexpectedly passed governance for {reason:?}"
    );
    assert!(exporter.get_finished_metrics().unwrap().is_empty());
    assert_eq!(
        jackin_telemetry::facade_health().by_signal_reason
            [jackin_telemetry::Signal::Metric as usize][reason as usize],
        before + 1,
        "raw batch did not move the exact metric rejection cell"
    );
}

#[test]
fn governed_raw_meter_rejects_every_metric_contract_class() {
    use opentelemetry::{Array, KeyValue, Value};

    assert_raw_metric_batch_rejected(jackin_telemetry::Rejection::UnknownName, |meter| {
        meter.u64_counter("unknown.metric").build().add(1, &[]);
    });
    assert_raw_metric_batch_rejected(jackin_telemetry::Rejection::InvalidValue, |meter| {
        meter
            .f64_histogram(jackin_telemetry::schema::metrics::UI_JANK)
            .with_description(jackin_telemetry::schema::metrics::UI_JANK_DEF.description)
            .with_unit(jackin_telemetry::schema::metrics::UI_JANK_DEF.unit)
            .build()
            .record(1.0, &[KeyValue::new("app.screen.id", "workspace.list")]);
    });
    assert_raw_metric_batch_rejected(jackin_telemetry::Rejection::UnknownAttribute, |meter| {
        meter
            .u64_counter(jackin_telemetry::schema::metrics::UI_JANK)
            .with_description(jackin_telemetry::schema::metrics::UI_JANK_DEF.description)
            .with_unit(jackin_telemetry::schema::metrics::UI_JANK_DEF.unit)
            .build()
            .add(1, &[KeyValue::new("bogus.secret", "bounded")]);
    });
    assert_raw_metric_batch_rejected(jackin_telemetry::Rejection::Privacy, |meter| {
        meter
            .u64_counter(jackin_telemetry::schema::metrics::UI_JANK)
            .with_description(jackin_telemetry::schema::metrics::UI_JANK_DEF.description)
            .with_unit(jackin_telemetry::schema::metrics::UI_JANK_DEF.unit)
            .build()
            .add(1, &[KeyValue::new("app.screen.id", "/private/workspace")]);
    });
    assert_raw_metric_batch_rejected(jackin_telemetry::Rejection::SizeLimit, |meter| {
        meter
            .u64_counter(jackin_telemetry::schema::metrics::UI_JANK)
            .with_description(jackin_telemetry::schema::metrics::UI_JANK_DEF.description)
            .with_unit(jackin_telemetry::schema::metrics::UI_JANK_DEF.unit)
            .build()
            .add(
                1,
                &[KeyValue::new(
                    "app.screen.id",
                    "x".repeat(jackin_telemetry::limits::MAX_STRING_ATTRIBUTE_BYTES + 1),
                )],
            );
    });
    assert_raw_metric_batch_rejected(jackin_telemetry::Rejection::InvalidValue, |meter| {
        meter
            .u64_counter(jackin_telemetry::schema::metrics::UI_JANK)
            .with_description(jackin_telemetry::schema::metrics::UI_JANK_DEF.description)
            .with_unit(jackin_telemetry::schema::metrics::UI_JANK_DEF.unit)
            .build()
            .add(
                1,
                &[KeyValue::new(
                    "app.screen.id",
                    Value::Array(Array::String(vec!["workspace.list".into()])),
                )],
            );
    });
    assert_raw_metric_batch_rejected(jackin_telemetry::Rejection::Cardinality, |meter| {
        let histogram = meter
            .f64_histogram(jackin_telemetry::schema::metrics::UI_FOCUS_DURATION)
            .with_description(jackin_telemetry::schema::metrics::UI_FOCUS_DURATION_DEF.description)
            .with_unit(jackin_telemetry::schema::metrics::UI_FOCUS_DURATION_DEF.unit)
            .build();
        for index in 0..=jackin_telemetry::limits::MAX_CARDINALITY {
            histogram.record(
                0.001,
                &[
                    KeyValue::new("app.screen.id", "workspace.list"),
                    KeyValue::new("app.widget.id", format!("widget-{index}")),
                ],
            );
        }
    });
}

#[test]
fn rejected_metric_collection_is_not_reported_as_exported() {
    let facade_before = jackin_telemetry::facade_health();
    let export_before = crate::telemetry_health_snapshot();
    let result =
        super::governed_metric_export_result(Err(jackin_telemetry::Rejection::UnknownName));

    assert!(matches!(
        result,
        Err(opentelemetry_sdk::error::OTelSdkError::InternalFailure(message))
            if message == "metric export rejected by telemetry governance"
    ));
    let facade_after = jackin_telemetry::facade_health();
    let export_after = crate::telemetry_health_snapshot();
    assert_eq!(
        facade_after.by_signal_reason[jackin_telemetry::Signal::Metric as usize]
            [jackin_telemetry::Rejection::UnknownName as usize],
        facade_before.by_signal_reason[jackin_telemetry::Signal::Metric as usize]
            [jackin_telemetry::Rejection::UnknownName as usize]
            + 1
    );
    assert_eq!(
        export_after.metrics.attempts,
        export_before.metrics.attempts + 1
    );
    assert_eq!(
        export_after.metrics.successes,
        export_before.metrics.successes
    );
    assert_eq!(
        export_after.metrics.failures,
        export_before.metrics.failures + 1
    );
}
