// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[test]
fn conformance_interleaved_sessions_never_cross_contaminate_ids() -> anyhow::Result<()> {
    let invocation = jackin_telemetry::identity::InvocationId::mint();
    jackin_telemetry::identity::set_current_invocation(invocation).unwrap();
    jackin_telemetry::identity::set_current_invocation(invocation).unwrap();
    assert_eq!(
        jackin_telemetry::identity::set_current_invocation(
            jackin_telemetry::identity::InvocationId::mint()
        ),
        Err(invocation)
    );
    let invocation_id = invocation.to_string();
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

    let first = jackin_telemetry::identity::SessionGuard::begin(
        jackin_telemetry::identity::SessionKind::Console,
    )
    .unwrap();
    let first_id = first.context().current.to_string();
    let outcome = [jackin_telemetry::Attr {
        key: jackin_telemetry::schema::attrs::OUTCOME,
        value: jackin_telemetry::Value::Str("success"),
    }];
    jackin_telemetry::emit_event(
        &jackin_telemetry::event::OPERATION_LOG,
        jackin_telemetry::FieldSet::new(&outcome, None),
    )
    .unwrap();
    drop(first);
    let second = jackin_telemetry::identity::SessionGuard::begin(
        jackin_telemetry::identity::SessionKind::Attachment,
    )
    .unwrap();
    let second_id = second.context().current.to_string();
    assert_ne!(first_id, second_id);
    assert_eq!(
        second.context().previous.map(|id| id.to_string()),
        Some(first_id.clone())
    );
    let operation =
        jackin_telemetry::operation(&jackin_telemetry::operation::PROCESS_COMMAND, &[]).unwrap();
    operation.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);
    drop(second);
    jackin_telemetry::emit_event(
        &jackin_telemetry::event::OPERATION_LOG,
        jackin_telemetry::FieldSet::new(&outcome, None),
    )
    .unwrap();
    jackin_diagnostics::flush_wire_test_export()?;
    drop(runtime_guard);

    let logs = testbed.log_records();
    let lifecycle: Vec<_> = logs
        .iter()
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
        assert!(record.attributes.iter().any(|attribute| {
            attribute.key == "cli.invocation.id"
                && attribute.value.as_ref().is_some_and(|value| {
                    matches!(
                        value.value.as_ref(),
                        Some(opentelemetry_proto::tonic::common::v1::any_value::Value::StringValue(value))
                            if value == &invocation_id
                    )
                })
        }));
    }
    let breadcrumbs: Vec<_> = logs
        .iter()
        .filter(|record| record.event_name == "operation.log")
        .collect();
    assert_eq!(breadcrumbs.len(), 2);
    assert!(breadcrumbs[0].attributes.iter().any(|attribute| {
        attribute.key == "session.id" && attribute.value.as_ref().is_some_and(|value| {
            matches!(
                value.value.as_ref(),
                Some(opentelemetry_proto::tonic::common::v1::any_value::Value::StringValue(value))
                    if value == &first_id
            )
        })
    }));
    assert!(
        !breadcrumbs[1]
            .attributes
            .iter()
            .any(|attribute| attribute.key == "session.id")
    );

    let spans = testbed.spans();
    assert!(!spans.iter().any(|span| span.name.contains("session")));
    let command = spans
        .iter()
        .find(|span| span.name == "process.command")
        .expect("bounded command span exported");
    assert!(command.attributes.iter().any(|attribute| {
        attribute.key == "session.id" && attribute.value.as_ref().is_some_and(|value| {
            matches!(
                value.value.as_ref(),
                Some(opentelemetry_proto::tonic::common::v1::any_value::Value::StringValue(value))
                    if value == &second_id
            )
        })
    }));
    jackin_diagnostics::shutdown_capsule_tracing();
    Ok(())
}
