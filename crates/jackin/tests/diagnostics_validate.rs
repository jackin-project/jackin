// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn diagnostics_validate_confirms_live_otlp_and_rejects_stopped_endpoint() -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let mut testbed = runtime.block_on(async { jackin_otlp_testbed::Testbed::start() })?;
    let home = tempfile::tempdir()?;
    let endpoint = testbed.endpoint();

    let mut live = Command::cargo_bin("jackin")?;
    let output = live
        .timeout(std::time::Duration::from_secs(20))
        .args(["diagnostics", "validate"])
        .env("JACKIN_HOME_DIR", home.path())
        .env("OTEL_EXPORTER_OTLP_ENDPOINT", &endpoint)
        .env("OTEL_EXPORTER_OTLP_PROTOCOL", "grpc")
        .output()
        .map_err(|error| {
            anyhow::anyhow!(
                "live validate did not exit: {error}; traces={} logs={} metrics={}",
                testbed.traces().len(),
                testbed.logs().len(),
                testbed.metrics().len()
            )
        })?;
    assert!(
        output.status.success(),
        "{output:?}; traces={} logs={} metrics={}",
        testbed.traces().len(),
        testbed.logs().len(),
        testbed.metrics().len()
    );
    assert!(
        predicate::str::contains("signals:   traces ok  logs ok  metrics ok")
            .eval(&String::from_utf8_lossy(&output.stdout))
    );
    assert!(runtime.block_on(testbed.wait_for_all_signals(std::time::Duration::from_secs(2))));

    testbed.stop();
    Command::cargo_bin("jackin")?
        .args(["diagnostics", "validate"])
        .env("JACKIN_HOME_DIR", home.path())
        .env("OTEL_EXPORTER_OTLP_ENDPOINT", &endpoint)
        .env("OTEL_EXPORTER_OTLP_PROTOCOL", "grpc")
        .assert()
        .failure()
        .stderr(predicate::str::contains("telemetry export failed"));
    Ok(())
}
