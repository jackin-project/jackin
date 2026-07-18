// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[test]
fn tls_build_failure_rolls_back_export_runtime() -> anyhow::Result<()> {
    let status = std::process::Command::new(std::env::current_exe()?)
        .args(["--exact", "tls_failure_child", "--nocapture"])
        .env("JACKIN_TLS_FAILURE_CHILD", "1")
        .env(
            "OTEL_EXPORTER_OTLP_ENDPOINT",
            "https://collector.invalid:4317",
        )
        .env(
            "OTEL_EXPORTER_OTLP_CERTIFICATE",
            "/missing/private/tenant-ca.pem",
        )
        .status()?;
    assert!(status.success(), "TLS rollback child failed");
    Ok(())
}

#[test]
fn tls_failure_child() {
    if std::env::var_os("JACKIN_TLS_FAILURE_CHILD").is_none() {
        return;
    }
    let error = jackin_diagnostics::init_tracing(false, "tls-rollback")
        .expect_err("missing CA must reject activation");
    assert!(error.to_string().contains("CA certificate"));
    assert!(!error.to_string().contains("/missing/private"));
    assert!(!jackin_diagnostics::otlp_runtime_active_for_test());
}

#[test]
fn metric_tls_failure_rolls_back_built_trace_log_providers() -> anyhow::Result<()> {
    let status = std::process::Command::new(std::env::current_exe()?)
        .args(["--exact", "metric_tls_failure_child", "--nocapture"])
        .env("JACKIN_METRIC_TLS_FAILURE_CHILD", "1")
        .env("OTEL_EXPORTER_OTLP_ENDPOINT", "http://127.0.0.1:9")
        .env(
            "OTEL_EXPORTER_OTLP_METRICS_CERTIFICATE",
            "/missing/private/metric-ca.pem",
        )
        .status()?;
    assert!(status.success(), "metric rollback child failed");
    Ok(())
}

#[test]
fn metric_tls_failure_child() {
    if std::env::var_os("JACKIN_METRIC_TLS_FAILURE_CHILD").is_none() {
        return;
    }
    use opentelemetry::metrics::MeterProvider as _;

    let error = jackin_diagnostics::init_tracing(false, "metric-rollback")
        .expect_err("missing metric CA must reject activation");
    assert!(error.to_string().contains("metrics CA certificate"));
    assert!(!error.to_string().contains("/missing/private"));
    assert!(!jackin_diagnostics::otlp_runtime_active_for_test());
    assert_eq!(
        jackin_diagnostics::telemetry_health_snapshot().active_signals,
        0
    );
    tracing::subscriber::set_global_default(tracing_subscriber::registry())
        .expect("failed activation must not install a subscriber");
    let provider = opentelemetry_sdk::metrics::SdkMeterProvider::builder().build();
    jackin_telemetry::install(&provider.meter("post-failure-facade"))
        .expect("failed activation must release the facade reservation");
}
