// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

fn string_attribute<'a>(
    record: &'a opentelemetry_proto::tonic::logs::v1::LogRecord,
    key: &str,
) -> Option<&'a str> {
    record
        .attributes
        .iter()
        .find(|attribute| attribute.key == key)
        .and_then(|attribute| attribute.value.as_ref())
        .and_then(|value| value.value.as_ref())
        .and_then(|value| match value {
            opentelemetry_proto::tonic::common::v1::any_value::Value::StringValue(value) => {
                Some(value.as_str())
            }
            _ => None,
        })
}

#[test]
fn conformance_sequential_reattach_preserves_previous_id() -> anyhow::Result<()> {
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
    let second_start = logs
        .iter()
        .find(|record| {
            record.event_name == "session.start"
                && string_attribute(record, "session.id") == Some(second_id.as_str())
        })
        .ok_or_else(|| anyhow::anyhow!("second session start missing"))?;
    assert_eq!(
        string_attribute(second_start, "session.previous_id"),
        Some(first_id.as_str())
    );
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

#[test]
fn session_concurrency_child() -> anyhow::Result<()> {
    let Ok(endpoint) = std::env::var("JACKIN_TEST_OTLP_ENDPOINT") else {
        return Ok(());
    };
    let child_id = std::env::var("JACKIN_TEST_CHILD_ID")?;
    let barrier = std::path::PathBuf::from(std::env::var("JACKIN_TEST_BARRIER")?);
    let invocation = jackin_telemetry::identity::InvocationId::mint();
    jackin_telemetry::identity::set_current_invocation(invocation)
        .map_err(|current| anyhow::anyhow!("invocation already owned by {current}"))?;
    jackin_diagnostics::init_wire_test_export(
        &endpoint,
        jackin_diagnostics::ServiceIdentity::HOST_ONE_SHOT,
    )?;
    let session = jackin_telemetry::identity::SessionGuard::begin(
        jackin_telemetry::identity::SessionKind::Attachment,
    )?;
    std::fs::write(barrier.join(format!("ready-{child_id}")), b"ready")?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while std::fs::read_dir(&barrier)?.count() < 2 {
        anyhow::ensure!(
            std::time::Instant::now() < deadline,
            "concurrent-session barrier timed out"
        );
        std::thread::park_timeout(std::time::Duration::from_millis(5));
    }

    let outcome = [jackin_telemetry::Attr {
        key: jackin_telemetry::schema::attrs::OUTCOME,
        value: jackin_telemetry::Value::Str("success"),
    }];
    jackin_telemetry::emit_event(
        &jackin_telemetry::event::OPERATION_LOG,
        jackin_telemetry::FieldSet::new(&outcome, None),
    )
    .map_err(|error| anyhow::anyhow!("operation log rejected: {error:?}"))?;
    let operation = jackin_telemetry::operation(&jackin_telemetry::operation::PROCESS_COMMAND, &[])
        .map_err(|error| anyhow::anyhow!("process operation rejected: {error:?}"))?;
    operation.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);
    drop(session);
    jackin_diagnostics::flush_wire_test_export()?;
    jackin_diagnostics::shutdown_capsule_tracing();
    Ok(())
}

#[test]
fn conformance_interleaved_sessions_never_cross_contaminate_ids() -> anyhow::Result<()> {
    use std::collections::{HashMap, HashSet};
    use std::process::Command;

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let testbed = runtime.block_on(async { jackin_otlp_testbed::Testbed::start() })?;
    let barrier = tempfile::tempdir()?;
    let mut children = (0..2)
        .map(|child_id| {
            Command::new(std::env::current_exe()?)
                .args(["--exact", "session_concurrency_child", "--nocapture"])
                .env("JACKIN_TEST_OTLP_ENDPOINT", testbed.endpoint())
                .env("JACKIN_TEST_CHILD_ID", child_id.to_string())
                .env("JACKIN_TEST_BARRIER", barrier.path())
                .spawn()
                .map_err(anyhow::Error::from)
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    for child in &mut children {
        anyhow::ensure!(child.wait()?.success(), "concurrent session child failed");
    }
    anyhow::ensure!(
        runtime.block_on(testbed.wait_for_all_signals(std::time::Duration::from_secs(2))),
        "concurrent children did not export every signal"
    );

    let breadcrumbs = testbed
        .log_records()
        .into_iter()
        .filter(|record| record.event_name == "operation.log")
        .collect::<Vec<_>>();
    assert_eq!(breadcrumbs.len(), 2);
    let pairs = breadcrumbs
        .iter()
        .map(|record| {
            (
                string_attribute(record, "cli.invocation.id")
                    .expect("breadcrumb invocation")
                    .to_owned(),
                string_attribute(record, "session.id")
                    .expect("breadcrumb session")
                    .to_owned(),
            )
        })
        .collect::<HashMap<_, _>>();
    assert_eq!(pairs.len(), 2, "each process must retain its invocation");
    assert_eq!(
        pairs.values().collect::<HashSet<_>>().len(),
        2,
        "sessions crossed process boundaries"
    );

    let commands = testbed
        .spans()
        .into_iter()
        .filter(|span| span.name == "process.command")
        .collect::<Vec<_>>();
    assert_eq!(commands.len(), 2);
    for command in commands {
        let invocation = command
            .attributes
            .iter()
            .find(|attribute| attribute.key == "cli.invocation.id")
            .and_then(|attribute| attribute.value.as_ref())
            .and_then(|value| value.value.as_ref())
            .and_then(|value| match value {
                opentelemetry_proto::tonic::common::v1::any_value::Value::StringValue(value) => {
                    Some(value.as_str())
                }
                _ => None,
            })
            .expect("command invocation");
        let session = command
            .attributes
            .iter()
            .find(|attribute| attribute.key == "session.id")
            .and_then(|attribute| attribute.value.as_ref())
            .and_then(|value| value.value.as_ref())
            .and_then(|value| match value {
                opentelemetry_proto::tonic::common::v1::any_value::Value::StringValue(value) => {
                    Some(value.as_str())
                }
                _ => None,
            })
            .expect("command session");
        assert_eq!(pairs.get(invocation).map(String::as_str), Some(session));
    }
    Ok(())
}
