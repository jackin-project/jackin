// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[test]
fn conformance_interleaved_sessions_never_cross_contaminate_ids() -> anyhow::Result<()> {
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

    let first = jackin_telemetry::identity::SessionGuard::begin();
    let first_id = first.context().current.to_string();
    let second = jackin_telemetry::identity::SessionGuard::begin();
    let second_id = second.context().current.to_string();
    assert_ne!(first_id, second_id);
    assert_eq!(
        second.context().previous.map(|id| id.to_string()),
        Some(first_id.clone())
    );
    drop(first);
    drop(second);
    jackin_diagnostics::flush_wire_test_export()?;
    drop(runtime_guard);

    let lifecycle: Vec<_> = testbed
        .log_records()
        .into_iter()
        .filter(|record| record.event_name == "session.start" || record.event_name == "session.end")
        .collect();
    assert_eq!(lifecycle.len(), 4);
    for record in lifecycle {
        let ids: Vec<_> = record
            .attributes
            .iter()
            .filter(|attribute| attribute.key == "session.id")
            .filter_map(|attribute| attribute.value.as_ref())
            .filter_map(|value| value.value.as_ref())
            .filter_map(|value| match value {
                opentelemetry_proto::tonic::common::v1::any_value::Value::StringValue(value) => {
                    Some(value.as_str())
                }
                _ => None,
            })
            .collect();
        assert_eq!(ids.len(), 1);
        assert!(ids[0] == first_id || ids[0] == second_id);
    }
    jackin_diagnostics::shutdown_capsule_tracing();
    Ok(())
}
