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
    assert_wire_requests(&testbed, expected_requests);
    jackin_diagnostics::shutdown_capsule_tracing();
    let shutdown = jackin_diagnostics::telemetry_health_snapshot();
    assert_eq!(shutdown.active_signals, 0);
    assert!(shutdown.shutdown_completed);
    assert_eq!(shutdown.shutdown_succeeded, flush_succeeds);
    assert!(!shutdown.shutdown_timed_out);
    Ok(())
}

fn assert_wire_requests(testbed: &jackin_otlp_testbed::Testbed, expected_requests: usize) {
    let traces = testbed.traces();
    let logs = testbed.logs();
    let metrics = testbed.metrics();
    let log_records = testbed.log_records();
    let validate_event = jackin_telemetry::schema::events::TELEMETRY_VALIDATE;
    let counts_match = traces.len() == expected_requests
        && logs.len() == expected_requests
        && metrics.len() == expected_requests;
    // Per-request distribution, not just the flattened total: retries re-send
    // the identical batch, so every captured log request must carry exactly
    // one matching record. A 0/1/2 split across three requests with total 3
    // is retry-payload corruption and must fail (PR #1014 Codex P2).
    let records_match = log_records.len() == expected_requests
        && logs.len() == expected_requests
        && logs.iter().all(|request| {
            let records: Vec<_> = request
                .resource_logs
                .iter()
                .flat_map(|resource| resource.scope_logs.iter())
                .flat_map(|scope| scope.log_records.iter())
                .collect();
            records.len() == 1 && records[0].event_name == validate_event
        });
    if counts_match && records_match {
        return;
    }
    let spans_per_request: Vec<usize> = traces
        .iter()
        .map(|request| {
            request
                .resource_spans
                .iter()
                .flat_map(|resource| resource.scope_spans.iter())
                .map(|scope| scope.spans.len())
                .sum()
        })
        .collect();
    let records_per_request: Vec<usize> = logs
        .iter()
        .map(|request| {
            request
                .resource_logs
                .iter()
                .flat_map(|resource| resource.scope_logs.iter())
                .map(|scope| scope.log_records.len())
                .sum()
        })
        .collect();
    let metrics_per_request: Vec<usize> = metrics
        .iter()
        .map(|request| {
            request
                .resource_metrics
                .iter()
                .flat_map(|resource| resource.scope_metrics.iter())
                .map(|scope| scope.metrics.len())
                .sum()
        })
        .collect();
    let span_names: Vec<String> = testbed
        .spans()
        .iter()
        .map(|span| span.name.clone())
        .collect();
    let event_names: Vec<String> = log_records
        .iter()
        .map(|record| record.event_name.clone())
        .collect();
    let health = jackin_diagnostics::telemetry_health_snapshot();
    // NOTE: assert!, not panic! — clippy::panic is denied (-D warnings).
    assert!(
        counts_match && records_match,
        "wire request mismatch: expected {expected_requests} requests per signal \
         with one `{validate_event}` log record each; got traces={} (spans per request: {spans_per_request:?}, span names: {span_names:?}), \
         logs={} (records per request: {records_per_request:?}, event names: {event_names:?}), \
         metrics={} (metrics per request: {metrics_per_request:?}, metric names: {:?}); \
         telemetry health at failure: {health:#?}",
        traces.len(),
        logs.len(),
        metrics.len(),
        testbed.metric_names(),
    );
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
