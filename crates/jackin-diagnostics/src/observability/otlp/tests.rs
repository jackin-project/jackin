use super::{
    build_resource_for, build_resource_for_sources, exporter_tls, flush_before, grpc_endpoint,
    resolve_endpoint, runtime_creation_count, semantic_os_type, shutdown, unsupported_protocol,
};

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
    assert!(error.to_string().contains("OTLP traces CA certificate"));
    assert!(!error.to_string().contains("/secret/tenant-ca.pem"));
}

#[test]
fn facade_event_exports_native_event_name_once() {
    let (export, subscriber) = super::test_layers(false, "unused");
    tracing::subscriber::with_default(subscriber, || {
        jackin_telemetry::emit_event(
            &jackin_telemetry::event::SESSION_START,
            jackin_telemetry::FieldSet::default(),
        )
        .unwrap();
    });
    export.logger_provider.force_flush().unwrap();
    let logs = export.logs.get_emitted_logs().unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].record.event_name(), Some("session.start"));
}

fn log_attribute<'a>(
    record: &'a opentelemetry_sdk::logs::SdkLogRecord,
    name: &str,
) -> Option<&'a opentelemetry::logs::AnyValue> {
    record
        .attributes_iter()
        .find_map(|(key, value)| (key.as_str() == name).then_some(value))
}

#[test]
fn conformance_single_delivery_preserves_native_shape() {
    use opentelemetry::logs::{AnyValue, Severity};
    use opentelemetry::trace::Status;

    let (export, subscriber) = super::test_layers(false, "unused");
    tracing::subscriber::with_default(subscriber, || {
        let operation =
            jackin_telemetry::operation(&jackin_telemetry::operation::CLI_COMMAND, &[]).unwrap();
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
fn registered_scalar_and_array_types_round_trip() {
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

        let values = ["bridge", "typed"];
        let validation = [jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::TELEMETRY_VALIDATION_VALUES,
            value: jackin_telemetry::Value::StrArray(&values),
        }];
        jackin_telemetry::emit_event(
            &jackin_telemetry::event::TELEMETRY_VALIDATE,
            jackin_telemetry::FieldSet::new(&validation, None),
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
    assert_eq!(
        log_attribute(&logs[2].record, "telemetry.validation.values"),
        Some(&AnyValue::ListAny(Box::new(vec![
            AnyValue::String("bridge".into()),
            AnyValue::String("typed".into()),
        ])))
    );
}

#[test]
fn active_run_compatibility_path_does_not_duplicate_operation_log() {
    let _lock = crate::DIAGNOSTICS_TEST_LOCK.lock().expect("test lock");
    let (export, subscriber) = super::test_layers(false, "unused");
    tracing::subscriber::with_default(subscriber, || {
        let directory = tempfile::tempdir().expect("temporary diagnostics directory");
        let paths = jackin_core::JackinPaths::for_tests(directory.path());
        let run = crate::RunDiagnostics::start(&paths, false, "status").expect("diagnostics run");
        let _active = run.activate();
        export.logs.reset();
        crate::operation_log(
            crate::OperationLevel::Info,
            "ignored.compatibility.name",
            "test",
            "one delivery",
            &[],
        );
    });
    export.logger_provider.force_flush().unwrap();
    let logs = export.logs.get_emitted_logs().unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].record.event_name(), Some("operation.log"));
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
    for def in [
        &jackin_telemetry::event::UI_WIDGET_FOCUSED,
        &jackin_telemetry::event::SESSION_START,
        &jackin_telemetry::event::APP_JANK,
        &jackin_telemetry::event::APP_CRASH,
    ] {
        jackin_telemetry::emit_event(def, jackin_telemetry::FieldSet::default()).unwrap();
    }
}

#[test]
fn governed_event_level_gates_are_exact_and_do_not_infer_span_state() {
    use opentelemetry::trace::Status;

    for (level, expected) in [("info", 3usize), ("debug", 4usize), ("trace", 5usize)] {
        let (export, subscriber) = super::test_layers_at(level, "unused");
        tracing::subscriber::with_default(subscriber, || {
            let operation =
                jackin_telemetry::operation(&jackin_telemetry::operation::CLI_COMMAND, &[])
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
        assert_eq!(logs.len(), expected, "{level} log gate");
        assert_eq!(spans.len(), 1, "{level} span gate");
        assert!(spans[0].events.is_empty(), "{level} duplicate span events");
        assert_eq!(spans[0].status, Status::Unset, "{level} inferred status");
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
    });
    export.logger_provider.force_flush().unwrap();
    export.tracer_provider.force_flush().unwrap();
    assert!(export.logs.get_emitted_logs().unwrap().is_empty());
    assert!(export.spans.get_finished_spans().unwrap().is_empty());
    let after = jackin_telemetry::facade_health();
    assert!(after.unknown_name >= before.unknown_name + 2);
    assert!(after.invalid_value > before.invalid_value);
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
