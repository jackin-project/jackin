// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[test]
#[ignore = "scheduled slow-export deadline gate"]
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
    assert!(elapsed >= std::time::Duration::from_secs(5), "{elapsed:?}");
    // Three signals flush sequentially. Each attempt is capped at five seconds
    // and the bounded policy permits at most three attempts per signal.
    assert!(elapsed < std::time::Duration::from_secs(46), "{elapsed:?}");
    testbed.set_behavior(jackin_otlp_testbed::Behavior::Ok);
    drop(runtime_guard);
    jackin_diagnostics::shutdown_capsule_tracing();
    Ok(())
}
