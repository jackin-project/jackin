// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[test]
fn conformance_endpoint_loss_never_blocks_product_emission() -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let testbed = runtime.block_on(async { jackin_otlp_testbed::Testbed::start() })?;
    let runtime_guard = runtime.enter();
    jackin_diagnostics::init_wire_test_export(
        &testbed.endpoint(),
        jackin_diagnostics::ServiceIdentity::HOST_ONE_SHOT,
    )?;
    let endpoint = testbed.endpoint();
    drop(testbed);
    drop(runtime_guard);
    wait_for_testbed_peer_close(&runtime, &endpoint)?;

    let started = std::time::Instant::now();
    for _ in 0..10_000 {
        let _event_result = jackin_telemetry::emit_event(
            &jackin_telemetry::event::TELEMETRY_VALIDATE,
            jackin_telemetry::FieldSet::default(),
        );
    }
    assert!(
        started.elapsed() < std::time::Duration::from_secs(1),
        "saturated emission blocked product work"
    );

    let flush = jackin_diagnostics::flush_wire_test_export();
    assert!(flush.is_err());
    let health = jackin_diagnostics::telemetry_health_snapshot();
    assert!(
        health.traces.failures + health.logs.failures + health.metrics.failures > 0,
        "flush={flush:?} health={health:?}"
    );
    jackin_diagnostics::shutdown_capsule_tracing();
    Ok(())
}

fn wait_for_testbed_peer_close(
    runtime: &tokio::runtime::Runtime,
    endpoint: &str,
) -> anyhow::Result<()> {
    let authority = endpoint
        .strip_prefix("http://")
        .ok_or_else(|| anyhow::anyhow!("unexpected testbed endpoint: {endpoint}"))?;
    runtime.block_on(async {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            match tokio::time::timeout(
                std::time::Duration::from_millis(100),
                tokio::net::TcpStream::connect(authority),
            )
            .await
            {
                Ok(Err(_)) => return Ok(()),
                Ok(Ok(stream)) => drop(stream),
                Err(_) => {}
            }
            anyhow::ensure!(
                std::time::Instant::now() < deadline,
                "OTLP testbed still accepted peers after stop"
            );
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
}
