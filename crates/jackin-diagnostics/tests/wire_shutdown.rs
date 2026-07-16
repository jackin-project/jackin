// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[test]
fn conformance_shutdown_flushes_session_end_and_is_idempotent() -> anyhow::Result<()> {
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
    jackin_diagnostics::flush_wire_test_export()?;

    // Telemetry emitted after an explicit validation flush still belongs to
    // the final process flush immediately before provider shutdown.
    let session = jackin_telemetry::identity::SessionGuard::begin(
        jackin_telemetry::identity::SessionKind::Capsule,
    )
    .unwrap();
    let session_id = session.context().current.to_string();
    drop(session);
    jackin_diagnostics::shutdown_capsule_tracing();
    jackin_diagnostics::shutdown_capsule_tracing();
    drop(runtime_guard);

    let record = testbed
        .find_event("session.end")
        .ok_or_else(|| anyhow::anyhow!("shutdown did not flush session.end"))?;
    assert!(record.attributes.iter().any(|attribute| {
        attribute.key == "session.id"
            && attribute.value.as_ref().is_some_and(|value| {
                value.value.as_ref().is_some_and(|value| {
                    matches!(
                        value,
                        opentelemetry_proto::tonic::common::v1::any_value::Value::StringValue(value)
                            if value == &session_id
                    )
                })
            })
    }));
    Ok(())
}
