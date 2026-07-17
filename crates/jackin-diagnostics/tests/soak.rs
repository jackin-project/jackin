// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

const ITERATIONS: usize = 10_000;
const CYCLE_INTERVAL: usize = 10;
const JOB_INTERVAL: usize = 100;
const FLUSH_INTERVAL: usize = 500;

fn governed<T>(result: Result<T, jackin_telemetry::Rejection>) -> anyhow::Result<T> {
    result.map_err(|reason| anyhow::anyhow!("soak telemetry rejected: {reason:?}"))
}

fn string_attribute<'a>(
    attributes: &'a [opentelemetry_proto::tonic::common::v1::KeyValue],
    key: &str,
) -> Option<&'a str> {
    attributes.iter().find_map(|attribute| {
        if attribute.key != key {
            return None;
        }
        match attribute.value.as_ref()?.value.as_ref()? {
            opentelemetry_proto::tonic::common::v1::any_value::Value::StringValue(value) => {
                Some(value.as_str())
            }
            _ => None,
        }
    })
}

#[test]
#[ignore = "accelerated lifecycle soak runs in the scheduled soak profile"]
fn soak_week_long_console_has_only_bounded_operations() -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let testbed = runtime.block_on(async { jackin_otlp_testbed::Testbed::start() })?;
    let runtime_guard = runtime.enter();
    jackin_diagnostics::init_wire_test_export(
        &testbed.endpoint(),
        jackin_diagnostics::ServiceIdentity::HOST_INTERACTIVE,
    )?;
    let invocation = jackin_telemetry::identity::InvocationId::mint();
    jackin_telemetry::identity::set_current_invocation(invocation)
        .map_err(|current| anyhow::anyhow!("invocation already owned by {current}"))?;
    let invocation = invocation.to_string();
    let started = Instant::now();
    let mut screens = jackin_telemetry::ui::ScreenVisitTracker::new();
    let mut sessions = BTreeSet::new();
    let mut previous_session = None;

    for index in 0..ITERATIONS {
        governed(screens.enter(jackin_telemetry::schema::enums::ScreenId::WorkspaceList))?;
        let action = governed(jackin_telemetry::root_operation(
            &jackin_telemetry::operation::UI_ACTION,
            &[jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::UI_ACTION_NAME,
                value: jackin_telemetry::Value::Str("workspace.open"),
            }],
        ))?;
        action.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);
        governed(screens.exit(jackin_telemetry::schema::enums::TransitionReason::Action))?;

        if index % CYCLE_INTERVAL == 0 {
            let cycle = governed(jackin_telemetry::autonomous_root_operation(
                &jackin_telemetry::operation::BACKGROUND_CYCLE,
                &[jackin_telemetry::Attr {
                    key: jackin_telemetry::schema::attrs::BACKGROUND_CYCLE_NAME,
                    value: jackin_telemetry::Value::Str("agent_status"),
                }],
            ))?;
            cycle.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);
            let connection = governed(jackin_telemetry::root_operation(
                &jackin_telemetry::operation::CONNECTION_ATTEMPT,
                &[jackin_telemetry::Attr {
                    key: jackin_telemetry::schema::attrs::CONNECTION_PEER_TYPE,
                    value: jackin_telemetry::Value::Str("provider"),
                }],
            ))?;
            connection.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);
        }
        if index % JOB_INTERVAL == 0 {
            runtime.block_on(async {
                jackin_telemetry::spawn::spawn_prewarm_job(
                    jackin_telemetry::schema::enums::JobType::ImagePrewarm,
                    async {},
                    |()| jackin_telemetry::spawn::DetachedCompletion::success(),
                )
                .await
            })?;
            let session = jackin_telemetry::identity::SessionGuard::begin(
                jackin_telemetry::identity::SessionKind::Attachment,
            )?;
            assert_eq!(session.context().previous, previous_session);
            previous_session = Some(session.context().current);
            sessions.insert(session.context().current.to_string());
            drop(session);
        }
        if (index + 1) % FLUSH_INTERVAL == 0 {
            jackin_diagnostics::flush_wire_test_export()?;
        }
    }
    jackin_diagnostics::flush_wire_test_export()?;
    assert!(started.elapsed() < Duration::from_secs(60));

    let spans = testbed.spans();
    let mut counts = BTreeMap::new();
    for span in &spans {
        *counts.entry(span.name.as_str()).or_insert(0_usize) += 1;
        assert!(span.end_time_unix_nano >= span.start_time_unix_nano);
        assert!(
            span.end_time_unix_nano - span.start_time_unix_nano
                < Duration::from_secs(1).as_nanos() as u64,
            "unbounded {} span: {span:?}",
            span.name
        );
    }
    assert_eq!(counts.get("ui.action"), Some(&ITERATIONS));
    assert_eq!(
        counts.get("background.cycle"),
        Some(&(ITERATIONS / CYCLE_INTERVAL))
    );
    assert_eq!(
        counts.get("connection.attempt"),
        Some(&(ITERATIONS / CYCLE_INTERVAL))
    );
    assert_eq!(
        counts.get("prewarm.schedule"),
        Some(&(ITERATIONS / JOB_INTERVAL))
    );
    assert_eq!(
        counts.get("prewarm.attempt"),
        Some(&(ITERATIONS / JOB_INTERVAL))
    );

    let producers = spans
        .iter()
        .filter(|span| span.name == "prewarm.schedule")
        .map(|span| (span.span_id.clone(), span.trace_id.clone()))
        .collect::<BTreeSet<_>>();
    for consumer in spans.iter().filter(|span| span.name == "prewarm.attempt") {
        assert_eq!(consumer.links.len(), 1);
        let link = &consumer.links[0];
        assert!(producers.contains(&(link.span_id.clone(), link.trace_id.clone())));
    }
    for action in spans.iter().filter(|span| span.name == "ui.action") {
        assert_eq!(
            string_attribute(&action.attributes, "cli.invocation.id"),
            Some(invocation.as_str())
        );
    }
    assert!(
        spans
            .iter()
            .filter(|span| span.name == "background.cycle")
            .all(|span| string_attribute(&span.attributes, "cli.invocation.id").is_none())
    );
    assert_eq!(screens.sequence(), (ITERATIONS * 2) as u64);
    assert_eq!(screens.current_screen(), None);
    assert_eq!(sessions.len(), ITERATIONS / JOB_INTERVAL);
    assert_eq!(jackin_telemetry::identity::current_session(), None);

    let metric_names = testbed.metric_names();
    for expected in [
        "background.cycles",
        "background.cycle.duration",
        "connection.active",
        "connection.attempts",
        "connection.duration",
        "prewarm.active",
        "prewarm.duration",
        "prewarm.jobs",
        "process.memory.usage",
        "tokio.runtime.global_queue.depth",
    ] {
        assert!(
            metric_names.iter().any(|name| name == expected),
            "missing {expected}"
        );
    }
    assert!(
        !testbed
            .metric_dimension_keys()
            .iter()
            .any(|key| key == "job.id")
    );
    assert_eq!(testbed.legacy_namespace_violations(), Vec::<String>::new());
    jackin_diagnostics::shutdown_capsule_tracing();
    drop(runtime_guard);
    Ok(())
}
