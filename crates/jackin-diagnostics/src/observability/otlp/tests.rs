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
