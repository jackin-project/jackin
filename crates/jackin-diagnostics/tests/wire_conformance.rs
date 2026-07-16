// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_diagnostics::ServiceIdentity;

#[test]
fn conformance_wire_host_delivers_all_three_signals() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("test runtime");
    let testbed = runtime
        .block_on(async { jackin_otlp_testbed::Testbed::start() })
        .expect("start OTLP testbed");
    let runtime_guard = runtime.enter();
    jackin_diagnostics::init_wire_test_export(&testbed.endpoint(), ServiceIdentity::HOST_ONE_SHOT)
        .expect("install wire exporter");

    let operation =
        jackin_telemetry::root_operation(&jackin_telemetry::operation::TELEMETRY_VALIDATE, &[])
            .expect("validation operation");
    assert!(
        !operation.span().is_disabled(),
        "facade span unexpectedly disabled"
    );
    let metadata = operation.span().metadata().expect("enabled span metadata");
    assert_eq!(metadata.target(), jackin_telemetry::TELEMETRY_TARGET);
    assert_eq!(metadata.name(), "telemetry.validate");
    let span_guard = operation.span().enter();
    jackin_telemetry::emit_event(
        &jackin_telemetry::event::TELEMETRY_VALIDATE,
        jackin_telemetry::FieldSet::default(),
    )
    .expect("validation event");
    jackin_telemetry::counter(&jackin_telemetry::metric::TELEMETRY_VALIDATE)
        .add(1, &[])
        .expect("validation metric");
    drop(span_guard);
    operation.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);
    jackin_diagnostics::flush_wire_test_export().expect("flush all signals");
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

    assert!(!testbed.traces().is_empty(), "trace request missing");
    assert!(!testbed.logs().is_empty(), "logs request missing");
    assert!(!testbed.metrics().is_empty(), "metrics request missing");
    jackin_diagnostics::shutdown_capsule_tracing();
}
