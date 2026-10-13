// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[test]
fn conformance_slow_export_honors_each_signal_deadline() -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let testbed = runtime.block_on(async { jackin_otlp_testbed::Testbed::start() })?;
    testbed.set_behavior(jackin_otlp_testbed::Behavior::Delay(
        std::time::Duration::from_secs(30),
    ));
    let runtime_guard = runtime.enter();
    jackin_diagnostics::init_wire_test_export(
        &testbed.endpoint(),
        jackin_diagnostics::ServiceIdentity::HOST_ONE_SHOT,
    )?;

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

    let started = std::time::Instant::now();
    assert!(jackin_diagnostics::flush_wire_test_export().is_err());
    let elapsed = started.elapsed();
    assert!(elapsed >= std::time::Duration::from_secs(3), "{elapsed:?}");
    assert!(elapsed < std::time::Duration::from_secs(5), "{elapsed:?}");
    assert!(!testbed.traces().is_empty());
    assert!(!testbed.logs().is_empty());
    assert!(!testbed.metrics().is_empty());
    drop(runtime_guard);
    let shutdown_started = std::time::Instant::now();
    jackin_diagnostics::shutdown_capsule_tracing();
    let shutdown_elapsed = shutdown_started.elapsed();
    assert!(
        shutdown_elapsed < std::time::Duration::from_millis(5_250),
        "shutdown exceeded its coordinated budget: {shutdown_elapsed:?}"
    );
    let health = jackin_diagnostics::telemetry_health_snapshot();
    assert_eq!(health.active_signals, 0);
    assert!(health.shutdown_completed);
    assert!(!health.shutdown_succeeded);
    assert!(health.shutdown_timed_out);
    assert!(!jackin_diagnostics::otlp_runtime_active_for_test());
    Ok(())
}
