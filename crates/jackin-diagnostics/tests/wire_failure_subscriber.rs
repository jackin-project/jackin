// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[test]
fn subscriber_failure_leaves_no_active_providers() -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let testbed = runtime.block_on(async { jackin_otlp_testbed::Testbed::start() })?;
    let _guard = runtime.enter();
    tracing::subscriber::set_global_default(tracing_subscriber::registry())?;

    let error = jackin_diagnostics::init_wire_test_export(
        &testbed.endpoint(),
        jackin_diagnostics::ServiceIdentity::HOST_ONE_SHOT,
    )
    .expect_err("subscriber collision must reject activation");
    assert!(error.to_string().contains("already installed"));
    assert_eq!(
        jackin_diagnostics::telemetry_health_snapshot().active_signals,
        0
    );
    assert!(!jackin_diagnostics::otlp_runtime_active_for_test());
    Ok(())
}

#[test]
fn facade_reservation_failure_rolls_back_without_global_residue() -> anyhow::Result<()> {
    let status = std::process::Command::new(std::env::current_exe()?)
        .args(["--exact", "facade_reservation_failure_child", "--nocapture"])
        .env("JACKIN_FACADE_FAILURE_CHILD", "1")
        .env("OTEL_EXPORTER_OTLP_ENDPOINT", "http://127.0.0.1:9")
        .status()?;
    assert!(status.success(), "facade rollback child failed");
    Ok(())
}

#[test]
fn facade_reservation_failure_child() {
    if std::env::var_os("JACKIN_FACADE_FAILURE_CHILD").is_none() {
        return;
    }
    use opentelemetry::metrics::MeterProvider as _;

    let existing_provider = opentelemetry_sdk::metrics::SdkMeterProvider::builder().build();
    jackin_telemetry::install(&existing_provider.meter("preexisting-facade"))
        .expect("preexisting facade meter must install");
    let error = jackin_diagnostics::init_tracing(false, "facade-rollback")
        .expect_err("facade collision must reject activation");
    assert!(error.to_string().contains("facade meter"));
    assert!(!jackin_diagnostics::otlp_runtime_active_for_test());
    assert_eq!(
        jackin_diagnostics::telemetry_health_snapshot().active_signals,
        0
    );
    tracing::subscriber::set_global_default(tracing_subscriber::registry())
        .expect("failed activation must not install a subscriber");
    jackin_telemetry::counter(&jackin_telemetry::metric::TELEMETRY_VALIDATE)
        .add(1, &[])
        .expect("preexisting facade meter must remain usable");
}
