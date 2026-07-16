// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[test]
fn second_activation_cannot_disrupt_live_exporters() -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let testbed = runtime.block_on(async { jackin_otlp_testbed::Testbed::start() })?;
    let _runtime_guard = runtime.enter();
    jackin_diagnostics::init_wire_test_export(
        &testbed.endpoint(),
        jackin_diagnostics::ServiceIdentity::HOST_ONE_SHOT,
    )?;
    let creations = jackin_diagnostics::otlp_runtime_creation_count_for_test();

    let error = jackin_diagnostics::init_wire_test_export(
        &testbed.endpoint(),
        jackin_diagnostics::ServiceIdentity::HOST_ONE_SHOT,
    )
    .expect_err("a second owner must be rejected");
    assert!(error.to_string().contains("already active"));
    assert_eq!(
        jackin_diagnostics::otlp_runtime_creation_count_for_test(),
        creations
    );
    assert!(jackin_diagnostics::otlp_runtime_active_for_test());
    assert_eq!(
        jackin_diagnostics::telemetry_health_snapshot().active_signals,
        3
    );

    let operation =
        jackin_telemetry::root_operation(&jackin_telemetry::operation::TELEMETRY_VALIDATE, &[])
            .map_err(|error| anyhow::anyhow!("validation operation rejected: {error:?}"))?;
    jackin_telemetry::emit_event(
        &jackin_telemetry::event::TELEMETRY_VALIDATE,
        jackin_telemetry::FieldSet::default(),
    )
    .map_err(|error| anyhow::anyhow!("validation event rejected: {error:?}"))?;
    jackin_telemetry::counter(&jackin_telemetry::metric::TELEMETRY_VALIDATE)
        .add(1, &[])
        .map_err(|error| anyhow::anyhow!("validation metric rejected: {error:?}"))?;
    operation.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);
    jackin_diagnostics::flush_wire_test_export()?;
    assert!(!testbed.traces().is_empty());
    assert!(!testbed.logs().is_empty());
    assert!(!testbed.metrics().is_empty());

    jackin_diagnostics::shutdown_capsule_tracing();
    assert!(!jackin_diagnostics::otlp_runtime_active_for_test());
    Ok(())
}
