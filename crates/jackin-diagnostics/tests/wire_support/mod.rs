// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

const CORRELATION_KEYS: [&str; 7] = [
    "cli.invocation.id",
    "session.id",
    "session.previous_id",
    "job.id",
    "ui.screen.visit.id",
    "gen_ai.conversation.id",
    "app.crash.id",
];

pub(crate) fn assert_three_signal_delivery(
    identity: jackin_diagnostics::ServiceIdentity,
) -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let original_dir = std::env::current_dir()?;
    std::env::set_current_dir(home.path())?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let testbed = runtime.block_on(async { jackin_otlp_testbed::Testbed::start() })?;
    let runtime_guard = runtime.enter();
    let invocation = jackin_telemetry::identity::InvocationId::mint();
    jackin_telemetry::identity::set_current_invocation(invocation)
        .map_err(|current| anyhow::anyhow!("invocation already owned by {current}"))?;
    jackin_diagnostics::init_wire_test_export(&testbed.endpoint(), identity)?;
    let session_kind = if identity == jackin_diagnostics::ServiceIdentity::CAPSULE {
        jackin_telemetry::identity::SessionKind::Capsule
    } else {
        jackin_telemetry::identity::SessionKind::Console
    };
    let session = jackin_telemetry::identity::SessionGuard::begin(session_kind)?;
    let session_id = session.context().current.to_string();
    let before = jackin_diagnostics::telemetry_health_snapshot();

    let operation =
        jackin_telemetry::root_operation(&jackin_telemetry::operation::TELEMETRY_VALIDATE, &[])
            .map_err(|error| anyhow::anyhow!("validation operation rejected: {error:?}"))?;
    assert!(
        !operation.span().is_disabled(),
        "facade span unexpectedly disabled"
    );
    let metadata = operation
        .span()
        .metadata()
        .ok_or_else(|| anyhow::anyhow!("enabled span has no metadata"))?;
    assert_eq!(metadata.target(), jackin_telemetry::TELEMETRY_TARGET);
    assert_eq!(metadata.name(), "telemetry.validate");
    let span_guard = operation.span().enter();
    jackin_telemetry::emit_event(
        &jackin_telemetry::event::TELEMETRY_VALIDATE,
        jackin_telemetry::FieldSet::default(),
    )
    .map_err(|error| anyhow::anyhow!("validation event rejected: {error:?}"))?;
    jackin_telemetry::counter(&jackin_telemetry::metric::TELEMETRY_VALIDATE)
        .add(1, &[])
        .map_err(|error| anyhow::anyhow!("validation metric rejected: {error:?}"))?;
    drop(span_guard);
    operation.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);
    drop(session);
    jackin_diagnostics::flush_wire_test_export()?;
    let flushed = jackin_diagnostics::telemetry_health_snapshot();
    for (before, after) in [
        (before.traces, flushed.traces),
        (before.logs, flushed.logs),
        (before.metrics, flushed.metrics),
    ] {
        assert_eq!(after.attempts, before.attempts + 1);
        assert_eq!(after.successes, before.successes + 1);
        assert_eq!(after.failures, before.failures);
    }
    assert_eq!(flushed.export_attempts, before.export_attempts + 3);
    assert_eq!(flushed.export_successes, before.export_successes + 3);
    assert_eq!(flushed.export_failures, before.export_failures);
    drop(runtime_guard);
    assert!(
        runtime.block_on(testbed.wait_for_all_signals(std::time::Duration::from_secs(2))),
        "timed out waiting for all OTLP signals: traces={}, logs={}, metrics={}, facade={:?}, export={:?}",
        testbed.traces().len(),
        testbed.logs().len(),
        testbed.metrics().len(),
        jackin_telemetry::facade_health(),
        jackin_diagnostics::telemetry_health_snapshot(),
    );

    let traces = testbed.traces();
    let logs = testbed.logs();
    let metrics = testbed.metrics();
    assert!(!traces.is_empty(), "trace request missing");
    assert!(!logs.is_empty(), "logs request missing");
    assert!(!metrics.is_empty(), "metrics request missing");
    assert_correlation_contract(&testbed, invocation, &session_id)?;
    assert!(
        testbed
            .metric_names()
            .iter()
            .any(|name| name == "telemetry.validate"),
        "governed validation metric missing"
    );
    let metric_names = testbed.metric_names();
    for expected in [
        "process.cpu.utilization",
        "process.memory.usage",
        "tokio.runtime.workers",
        "tokio.runtime.alive_tasks",
        "tokio.runtime.global_queue.depth",
    ] {
        assert!(
            metric_names.iter().any(|name| name == expected),
            "runtime/process metric {expected} missing from wire export: {metric_names:?}"
        );
    }
    assert_eq!(
        testbed.legacy_namespace_violations(),
        Vec::<String>::new(),
        "legacy namespace escaped onto the OTLP wire"
    );
    assert_eq!(
        testbed.prohibited_value_violations(&[
            "/home/operator/private-workspace",
            "https://example.invalid/api?token=fixture-secret",
            "authorization=Bearer fixture-secret",
            "--password=fixture-secret",
            "fixture-role-name",
            "fixture-container-name",
            "fixture-tab-label",
            "fixture-pty-bytes",
            "mouse_x=413",
        ]),
        Vec::<String>::new(),
        "prohibited fixture material escaped onto the OTLP wire"
    );
    assert_cross_provider_resources(&traces, &logs, &metrics, identity)?;
    jackin_diagnostics::shutdown_capsule_tracing();
    let shutdown = jackin_diagnostics::telemetry_health_snapshot();
    assert_eq!(shutdown.active_signals, 0);
    assert!(shutdown.shutdown_completed);
    assert!(shutdown.shutdown_succeeded);
    assert!(!shutdown.shutdown_timed_out);
    std::env::set_current_dir(original_dir)?;
    assert!(
        std::fs::read_dir(home.path())?.next().is_none(),
        "governed telemetry created a local artifact"
    );
    Ok(())
}

fn assert_correlation_contract(
    testbed: &jackin_otlp_testbed::Testbed,
    invocation: jackin_telemetry::identity::InvocationId,
    session_id: &str,
) -> anyhow::Result<()> {
    let validation_span = testbed
        .spans()
        .into_iter()
        .find(|span| span.name == "telemetry.validate")
        .ok_or_else(|| anyhow::anyhow!("governed validation span missing"))?;
    let validation_event = testbed
        .find_event("telemetry.validate")
        .ok_or_else(|| anyhow::anyhow!("native governed validation event missing"))?;
    let invocation = invocation.to_string();
    for (key, expected) in [
        ("cli.invocation.id", invocation.as_str()),
        ("session.id", session_id),
    ] {
        assert_eq!(
            string_attribute(&validation_span.attributes, key),
            Some(expected)
        );
        assert_eq!(
            string_attribute(&validation_event.attributes, key),
            Some(expected)
        );
    }
    let metric_dimension_keys = testbed.metric_dimension_keys();
    for forbidden in CORRELATION_KEYS {
        assert!(
            !metric_dimension_keys.iter().any(|key| key == forbidden),
            "correlation identifier {forbidden} escaped into metric dimensions"
        );
    }
    Ok(())
}

fn string_attribute<'a>(
    attributes: &'a [opentelemetry_proto::tonic::common::v1::KeyValue],
    key: &str,
) -> Option<&'a str> {
    attributes
        .iter()
        .find(|attribute| attribute.key == key)
        .and_then(|attribute| attribute.value.as_ref())
        .and_then(|value| value.value.as_ref())
        .and_then(|value| match value {
            opentelemetry_proto::tonic::common::v1::any_value::Value::StringValue(value) => {
                Some(value.as_str())
            }
            _ => None,
        })
}

fn assert_cross_provider_resources(
    traces: &[opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest],
    logs: &[opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest],
    metrics: &[opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest],
    identity: jackin_diagnostics::ServiceIdentity,
) -> anyhow::Result<()> {
    let trace_resources = traces
        .iter()
        .flat_map(|request| &request.resource_spans)
        .map(|batch| {
            batch
                .resource
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("trace batch missing Resource"))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let log_resources = logs
        .iter()
        .flat_map(|request| &request.resource_logs)
        .map(|batch| {
            batch
                .resource
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("log batch missing Resource"))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let metric_resources = metrics
        .iter()
        .flat_map(|request| &request.resource_metrics)
        .map(|batch| {
            batch
                .resource
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("metric batch missing Resource"))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    anyhow::ensure!(!log_resources.is_empty(), "log Resource missing");
    anyhow::ensure!(!metric_resources.is_empty(), "metric Resource missing");
    let canonical_resource = trace_resources
        .first()
        .ok_or_else(|| anyhow::anyhow!("trace Resource missing"))?;
    anyhow::ensure!(
        trace_resources
            .iter()
            .chain(&log_resources)
            .chain(&metric_resources)
            .all(|resource| *resource == *canonical_resource),
        "trace, log, and metric providers must export the same immutable Resource"
    );
    for resource in trace_resources
        .into_iter()
        .chain(log_resources)
        .chain(metric_resources)
    {
        assert_resource_contract(resource, identity.service_name());
    }
    Ok(())
}

fn assert_resource_contract(
    resource: &opentelemetry_proto::tonic::resource::v1::Resource,
    service_name: &str,
) {
    let attribute = |key: &str| {
        resource
            .attributes
            .iter()
            .find(|attribute| attribute.key == key)
            .and_then(|attribute| attribute.value.as_ref())
            .and_then(|value| value.value.as_ref())
    };
    let expected = opentelemetry_proto::tonic::common::v1::any_value::Value::StringValue(
        service_name.to_owned(),
    );
    assert_eq!(attribute("service.name"), Some(&expected));
    assert!(attribute("service.version").is_some());
    assert!(attribute("service.instance.id").is_some());
    assert!(attribute("app.mode").is_some());
    let allowed = [
        "service.namespace",
        "service.name",
        "service.version",
        "service.instance.id",
        "process.pid",
        "process.executable.name",
        "app.mode",
        "process.runtime.name",
        "process.runtime.version",
        "os.type",
        "os.version",
        "container.id",
    ];
    assert!(
        resource
            .attributes
            .iter()
            .all(|attribute| allowed.contains(&attribute.key.as_str())),
        "Resource contains an attribute outside the fixed allowlist"
    );
    for forbidden in CORRELATION_KEYS {
        assert!(
            attribute(forbidden).is_none(),
            "correlation identifier {forbidden} must not be stored on Resource"
        );
    }
}
