// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub(crate) fn assert_scripted_response(
    behavior: jackin_otlp_testbed::Behavior,
    flush_succeeds: bool,
    expected_requests: usize,
) -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let testbed = runtime.block_on(async { jackin_otlp_testbed::Testbed::start() })?;
    testbed.set_behavior(behavior);
    let runtime_guard = runtime.enter();
    jackin_diagnostics::init_wire_test_export(
        &testbed.endpoint(),
        jackin_diagnostics::ServiceIdentity::HOST_ONE_SHOT,
    )?;
    let before = jackin_diagnostics::telemetry_health_snapshot();

    let operation =
        jackin_telemetry::root_operation(&jackin_telemetry::operation::TELEMETRY_VALIDATE, &[])
            .map_err(|error| anyhow::anyhow!("validation operation rejected: {error:?}"))?;
    let span_guard = operation.span().enter();
    jackin_telemetry::emit_event(
        &jackin_telemetry::event::TELEMETRY_VALIDATE,
        jackin_telemetry::FieldSet::default(),
    )
    .map_err(|error| anyhow::anyhow!("validation event rejected: {error:?}"))?;
    drop(span_guard);
    operation.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);

    let result = jackin_diagnostics::flush_wire_test_export();
    assert_eq!(result.is_ok(), flush_succeeds);
    let flushed = jackin_diagnostics::telemetry_health_snapshot();
    assert_signal_delta(before.traces, flushed.traces, flush_succeeds);
    assert_signal_delta(before.logs, flushed.logs, flush_succeeds);
    assert_signal_delta(before.metrics, flushed.metrics, flush_succeeds);
    assert_eq!(flushed.export_attempts, before.export_attempts + 3);
    assert_eq!(
        flushed.export_successes,
        before.export_successes + if flush_succeeds { 3 } else { 0 }
    );
    assert_eq!(
        flushed.export_failures,
        before.export_failures + if flush_succeeds { 0 } else { 3 }
    );
    drop(runtime_guard);
    assert_eq!(testbed.traces().len(), expected_requests);
    assert_eq!(testbed.logs().len(), expected_requests);
    assert_eq!(testbed.metrics().len(), expected_requests);
    jackin_diagnostics::shutdown_capsule_tracing();
    let shutdown = jackin_diagnostics::telemetry_health_snapshot();
    assert_eq!(shutdown.active_signals, 0);
    assert!(shutdown.shutdown_completed);
    assert_eq!(shutdown.shutdown_succeeded, flush_succeeds);
    assert!(!shutdown.shutdown_timed_out);
    Ok(())
}

fn assert_signal_delta(
    before: jackin_diagnostics::TelemetrySignalHealth,
    after: jackin_diagnostics::TelemetrySignalHealth,
    succeeded: bool,
) {
    assert_eq!(after.attempts, before.attempts + 1);
    assert_eq!(after.successes, before.successes + u64::from(succeeded));
    assert_eq!(after.failures, before.failures + u64::from(!succeeded));
}
