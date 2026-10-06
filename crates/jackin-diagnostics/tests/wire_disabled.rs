// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[test]
fn disabled_init_is_inactive_and_creates_no_runtime() -> anyhow::Result<()> {
    if [
        "OTEL_EXPORTER_OTLP_ENDPOINT",
        "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT",
        "OTEL_EXPORTER_OTLP_LOGS_ENDPOINT",
        "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT",
    ]
    .iter()
    .any(|name| std::env::var_os(name).is_some())
    {
        return Ok(());
    }
    let before = jackin_diagnostics::otlp_runtime_creation_count_for_test();
    assert!(!jackin_diagnostics::init_tracing(false, "disabled-wire")?);
    assert_eq!(
        jackin_diagnostics::otlp_runtime_creation_count_for_test(),
        before
    );
    assert_eq!(
        jackin_diagnostics::telemetry_health_snapshot().active_signals,
        0
    );
    Ok(())
}
