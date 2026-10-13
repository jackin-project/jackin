// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
        sanitized_authority("https://collector.example:4317/private/tenant"),
        Some("https://collector.example:4317".to_owned())
    );
}

#[test]
fn ordinary_https_enables_tls_without_custom_certificates() {
    assert!(otlp_channel::uses_tls("https://collector:4317"));
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
    let completed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let worker_completed = std::sync::Arc::clone(&completed);
    let task = FlushTask::spawn(
        move || {
            release_rx.recv().expect("release flush worker");
            worker_completed.store(true, std::sync::atomic::Ordering::Release);
            Ok(())
        },
        None,
    );
    let result = task.finish_before(started + std::time::Duration::from_millis(20));
    assert_eq!(result, Err("telemetry flush budget exhausted".to_owned()));
    assert!(!completed.load(std::sync::atomic::Ordering::Acquire));
    release_tx.send(()).expect("release flush worker");
    reap_flush_workers();
}

#[test]
fn validation_distinguishes_timeout_from_signal_failure() {
    let success = Ok(());
    let timeout = Err("telemetry flush budget exhausted".to_owned());
    let failure = Err("telemetry flush failed".to_owned());
    assert_eq!(
        validate_flush_results(&timeout, &success, &success),
        Err(ValidationFailure::Timeout)
    );
    assert_eq!(
        validate_flush_results(&success, &failure, &success),
        Err(ValidationFailure::Export("logs"))
    );
}

#[test]
fn telemetry_shutdown_fences_provider_in_an_isolated_process() {
    const CHILD: &str = "JACKIN_TELEMETRY_SHUTDOWN_CHILD";
    if std::env::var_os(CHILD).is_some() {
        run_telemetry_shutdown_scenario();
        println!("isolated telemetry shutdown scenario complete");
        return;
    }

    let output = std::process::Command::new(
        std::env::current_exe().expect("diagnostics test executable"),
    )
    .args([
        "--exact",
        "observability::otlp::tests::case_01::telemetry_shutdown_fences_provider_in_an_isolated_process",
        "--nocapture",
    ])
    .env(CHILD, "1")
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::piped())
    .spawn()
    .and_then(std::process::Child::wait_with_output)
    .expect("launch isolated telemetry shutdown test");
    assert!(
        output.status.success(),
        "isolated telemetry shutdown test failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("isolated telemetry shutdown scenario complete"),
        "isolated test process did not run the shutdown scenario:\n{}",
        String::from_utf8_lossy(&output.stdout)
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
        verified_container_id("abcdef123456"),
        Some("abcdef123456".to_owned())
    );
    assert_eq!(
        verified_container_id("ABCDEF123456"),
        Some("abcdef123456".to_owned())
    );
    assert_eq!(verified_container_id("named-capsule"), None);
    assert_eq!(verified_container_id("abc123"), None);
    assert_eq!(
        container_id_from_cgroup("0::/kubepods.slice/docker-ABCDEF1234567890.scope\n"),
        Some("abcdef1234567890".to_owned())
    );
    assert_eq!(
        container_id_from_cgroup("0::/user.slice/named-capsule.scope\n"),
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
    let _lock = crate::DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let before = runtime_creation_count();
    let env = |_key: &str| None;
    assert_eq!(config::resolve_otlp_config(&env), Ok(None));
    shutdown();
    shutdown();
    assert_eq!(runtime_creation_count(), before);
}

#[test]
fn tls_file_errors_do_not_expose_configured_paths() {
    let config = config::TlsConfig {
        certificate: Some("/secret/tenant-ca.pem".to_owned()),
        client_key: None,
        client_certificate: None,
    };
    let error = otlp_channel::validate_transport("https://collector:4317", &config)
        .expect_err("missing certificate must fail");
    let error = error.to_string();
    assert_eq!(error, "OTLP CA certificate is unavailable");
    assert!(!error.contains("/secret/tenant-ca.pem"));
    assert!(!error.contains("No such file"));
}
