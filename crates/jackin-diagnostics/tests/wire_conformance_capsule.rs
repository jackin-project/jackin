// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod wire_support;

#[test]
fn conformance_wire_capsule_safe_delivers_all_three_signals() -> anyhow::Result<()> {
    wire_support::assert_three_signal_delivery(jackin_diagnostics::ServiceIdentity::CAPSULE)
}

#[test]
fn authenticated_capsule_export_child() -> anyhow::Result<()> {
    if std::env::var_os("JACKIN_TEST_AUTHENTICATED_CAPSULE").is_none() {
        return Ok(());
    }
    let invocation = jackin_telemetry::identity::InvocationId::mint();
    jackin_telemetry::identity::set_current_invocation(invocation)
        .map_err(|current| anyhow::anyhow!("invocation already owned by {current}"))?;
    anyhow::ensure!(
        jackin_diagnostics::init_capsule_tracing(None)?,
        "Capsule OTLP export was not activated"
    );
    let session = jackin_telemetry::identity::SessionGuard::begin(
        jackin_telemetry::identity::SessionKind::Capsule,
    )?;
    let operation =
        jackin_telemetry::root_operation(&jackin_telemetry::operation::TELEMETRY_VALIDATE, &[])
            .map_err(|error| anyhow::anyhow!("validation operation rejected: {error:?}"))?;
    let guard = operation.span().enter();
    jackin_telemetry::emit_event(
        &jackin_telemetry::event::TELEMETRY_VALIDATE,
        jackin_telemetry::FieldSet::default(),
    )
    .map_err(|error| anyhow::anyhow!("validation event rejected: {error:?}"))?;
    jackin_telemetry::counter(&jackin_telemetry::metric::TELEMETRY_VALIDATE)
        .add(1, &[])
        .map_err(|error| anyhow::anyhow!("validation metric rejected: {error:?}"))?;
    drop(guard);
    operation.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);
    drop(session);
    jackin_diagnostics::flush_wire_test_export()?;
    jackin_diagnostics::shutdown_capsule_tracing();
    Ok(())
}

#[test]
fn conformance_wire_authenticated_capsule_safe_delivers_all_three_signals() -> anyhow::Result<()> {
    use std::process::Command;

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let testbed = runtime.block_on(async { jackin_otlp_testbed::Testbed::start() })?;
    testbed.set_behavior(jackin_otlp_testbed::Behavior::RequireHeader {
        name: "authorization",
        value: "Bearer capsule-safe-wire-secret",
    });
    let status = Command::new(std::env::current_exe()?)
        .args([
            "--exact",
            "authenticated_capsule_export_child",
            "--nocapture",
        ])
        .env("JACKIN_TEST_AUTHENTICATED_CAPSULE", "1")
        .env("OTEL_EXPORTER_OTLP_ENDPOINT", testbed.endpoint())
        .env("OTEL_EXPORTER_OTLP_PROTOCOL", "grpc")
        .env(
            "OTEL_EXPORTER_OTLP_HEADERS",
            "authorization=Bearer capsule-safe-wire-secret",
        )
        .env_remove("OTEL_SDK_DISABLED")
        .status()?;
    anyhow::ensure!(status.success(), "authenticated Capsule child failed");
    anyhow::ensure!(
        runtime.block_on(testbed.wait_for_all_signals(std::time::Duration::from_secs(2))),
        "authenticated Capsule did not deliver every signal"
    );

    anyhow::ensure!(
        testbed
            .spans()
            .iter()
            .any(|span| span.name == "telemetry.validate"),
        "authenticated Capsule trace marker missing"
    );
    anyhow::ensure!(
        testbed.find_event("telemetry.validate").is_some(),
        "authenticated Capsule log marker missing"
    );
    anyhow::ensure!(
        testbed
            .metric_names()
            .iter()
            .any(|name| name == "telemetry.validate"),
        "authenticated Capsule metric marker missing"
    );
    let wire_text = format!(
        "{:?}{:?}{:?}",
        testbed.traces(),
        testbed.logs(),
        testbed.metrics()
    );
    anyhow::ensure!(wire_text.contains("jackin-capsule"));
    assert_eq!(
        testbed.prohibited_value_violations(&["capsule-safe-wire-secret"]),
        Vec::<String>::new()
    );
    assert_eq!(testbed.legacy_namespace_violations(), Vec::<String>::new());
    Ok(())
}
