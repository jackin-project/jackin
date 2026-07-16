// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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
    jackin_diagnostics::init_wire_test_export(&testbed.endpoint(), identity)?;

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
    jackin_diagnostics::flush_wire_test_export()?;
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
    assert!(
        testbed
            .spans()
            .iter()
            .any(|span| span.name == "telemetry.validate"),
        "governed validation span missing"
    );
    assert!(
        testbed.find_event("telemetry.validate").is_some(),
        "native governed validation event missing"
    );
    assert!(
        testbed
            .metric_names()
            .iter()
            .any(|name| name == "telemetry.validate"),
        "governed validation metric missing"
    );
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
    for resource in traces
        .iter()
        .flat_map(|request| &request.resource_spans)
        .filter_map(|batch| batch.resource.as_ref())
        .chain(
            logs.iter()
                .flat_map(|request| &request.resource_logs)
                .filter_map(|batch| batch.resource.as_ref()),
        )
        .chain(
            metrics
                .iter()
                .flat_map(|request| &request.resource_metrics)
                .filter_map(|batch| batch.resource.as_ref()),
        )
    {
        assert_resource_contract(resource, identity.service_name);
    }
    jackin_diagnostics::shutdown_capsule_tracing();
    std::env::set_current_dir(original_dir)?;
    assert!(
        std::fs::read_dir(home.path())?.next().is_none(),
        "governed telemetry created a local artifact"
    );
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
    assert!(resource.attributes.iter().all(|attribute| {
        !attribute.key.starts_with("jackin.") && !attribute.key.starts_with("parallax.")
    }));
}
