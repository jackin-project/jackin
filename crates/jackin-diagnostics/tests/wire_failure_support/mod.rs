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
    drop(runtime_guard);
    assert_eq!(testbed.traces().len(), expected_requests);
    assert_eq!(testbed.logs().len(), expected_requests);
    assert_eq!(testbed.metrics().len(), expected_requests);
    jackin_diagnostics::shutdown_capsule_tracing();
    Ok(())
}
