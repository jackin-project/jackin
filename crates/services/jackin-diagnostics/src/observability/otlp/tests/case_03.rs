// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn widget_lifecycle_exports_exact_stable_identity_pair() {
    use opentelemetry::logs::AnyValue;

    let (export, subscriber) = test_layers(true, "unused");
    tracing::subscriber::with_default(subscriber, || {
        let mut tracker = jackin_telemetry::ui::WidgetFocusTracker::default();
        tracker.focus("capsule.pane").unwrap();
        tracker.unfocus().unwrap();
    });
    export.logger_provider.force_flush().unwrap();

    let logs = export.logs.get_emitted_logs().unwrap();
    assert_eq!(logs.len(), 2);
    for (log, event_name) in logs
        .iter()
        .zip(["ui.widget.focused", "ui.widget.unfocused"])
    {
        assert_eq!(log.record.event_name(), Some(event_name));
        assert_eq!(
            log_attribute(&log.record, "app.widget.id"),
            Some(&AnyValue::String("capsule.pane".into()))
        );
        assert_eq!(
            log_attribute(&log.record, "app.widget.name"),
            Some(&AnyValue::String("capsule.pane".into()))
        );
        assert_eq!(log.record.attributes_iter().count(), 2);
    }
}

#[test]
fn isolation_events_export_exact_private_shape() {
    use jackin_telemetry::schema::enums::{DindMode, NetworkMode, WorkspaceIsolationMode};

    let (export, subscriber) = test_layers(false, "unused");
    tracing::subscriber::with_default(subscriber, || {
        crate::operation::isolation_decision(
            WorkspaceIsolationMode::Worktree,
            NetworkMode::Allowlist,
            DindMode::Rootless,
        );
        crate::operation::isolation_firewall_failed(NetworkMode::Allowlist);
    });
    export.logger_provider.force_flush().unwrap();
    let logs = export.logs.get_emitted_logs().unwrap();
    assert_eq!(logs.len(), 2);

    let decision = logs
        .iter()
        .find(|log| log.record.event_name() == Some("isolation.decision"))
        .expect("decision event");
    let mut decision_keys = decision
        .record
        .attributes_iter()
        .map(|(key, _)| key.as_str())
        .collect::<Vec<_>>();
    decision_keys.sort_unstable();
    assert_eq!(
        decision_keys,
        [
            "dind.mode",
            "network.mode",
            "outcome",
            "workspace.isolation.mode"
        ]
    );

    let firewall = logs
        .iter()
        .find(|log| log.record.event_name() == Some("isolation.firewall.failed"))
        .expect("firewall event");
    let mut firewall_keys = firewall
        .record
        .attributes_iter()
        .map(|(key, _)| key.as_str())
        .collect::<Vec<_>>();
    firewall_keys.sort_unstable();
    assert_eq!(firewall_keys, ["error.type", "network.mode", "outcome"]);

    for log in &logs {
        assert!(log.record.body().is_none());
        assert!(!log.record.attributes_iter().any(|(key, _)| {
            ["path", "workspace", "role", "container", "host"]
                .iter()
                .any(|forbidden| key.as_str().contains(forbidden))
                && key.as_str() != "workspace.isolation.mode"
        }));
    }
}

#[test]
fn conformance_single_delivery_preserves_native_shape() {
    use opentelemetry::logs::{AnyValue, Severity};
    use opentelemetry::trace::Status;

    let (export, subscriber) = test_layers(false, "unused");
    tracing::subscriber::with_default(subscriber, || {
        let operation = jackin_telemetry::operation(
            &jackin_telemetry::operation::CLI_COMMAND,
            &cli_command_test_attrs(),
        )
        .unwrap();
        let entered = operation.span().enter();
        let attrs = [
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::CONFIG_MIGRATION_STEP_COUNT,
                value: jackin_telemetry::Value::U64(3),
            },
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::CONFIG_OPERATION,
                value: jackin_telemetry::Value::Str("migrate"),
            },
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::CONFIG_SCHEMA_VERSION_FROM,
                value: jackin_telemetry::Value::Str("legacy"),
            },
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::CONFIG_SCHEMA_VERSION_TO,
                value: jackin_telemetry::Value::Str("v1alpha9"),
            },
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::CONFIG_SCOPE,
                value: jackin_telemetry::Value::Str("global"),
            },
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::OUTCOME,
                value: jackin_telemetry::Value::Str("success"),
            },
        ];
        jackin_telemetry::emit_event(
            &jackin_telemetry::event::CONFIG_OPERATION,
            jackin_telemetry::FieldSet::new(&attrs, Some("configuration migrated")),
        )
        .unwrap();
        drop(entered);
        operation.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);
    });
    export.logger_provider.force_flush().unwrap();
    export.tracer_provider.force_flush().unwrap();

    let logs = export.logs.get_emitted_logs().unwrap();
    let spans = export.spans.get_finished_spans().unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(spans.len(), 1);
    let log = &logs[0];
    let span = &spans[0];
    assert_eq!(log.record.event_name(), Some("config.operation"));
    assert_eq!(log.record.severity_number(), Some(Severity::Info));
    assert_eq!(
        log.record.body(),
        Some(&AnyValue::String("configuration migrated".into()))
    );
    assert_eq!(
        log_attribute(&log.record, "config.migration.step_count"),
        Some(&AnyValue::Int(3))
    );
    assert_eq!(
        log_attribute(&log.record, "config.operation"),
        Some(&AnyValue::String("migrate".into()))
    );
    assert_eq!(
        log_attribute(&log.record, "config.scope"),
        Some(&AnyValue::String("global".into()))
    );
    let trace = log.record.trace_context().expect("active log context");
    assert_eq!(trace.trace_id, span.span_context.trace_id());
    assert_eq!(trace.span_id, span.span_context.span_id());
    assert!(
        span.events.is_empty(),
        "log event must not become a span event"
    );
    assert_eq!(span.status, Status::Unset);

    let resource = log
        .resource
        .iter()
        .map(|(key, value)| (key.as_str(), value.to_string()))
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(
        resource.get("service.name").map(String::as_str),
        Some("jackin")
    );
    assert_eq!(
        resource.get("app.mode").map(String::as_str),
        Some("one_shot")
    );
    assert!(!resource.contains_key("session.id"));
    assert!(!resource.contains_key("cli.invocation.id"));
}

#[test]
fn registered_scalar_types_round_trip() {
    use opentelemetry::logs::AnyValue;

    let (export, subscriber) = test_layers(false, "unused");
    tracing::subscriber::with_default(subscriber, || {
        let jank = [
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::std_attrs::APP_JANK_FRAME_COUNT,
                value: jackin_telemetry::Value::U64(7),
            },
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::std_attrs::APP_JANK_PERIOD,
                value: jackin_telemetry::Value::F64(0.25),
            },
        ];
        jackin_telemetry::emit_event(
            &jackin_telemetry::event::APP_JANK,
            jackin_telemetry::FieldSet::new(&jank, None),
        )
        .unwrap();

        let agent = [
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::std_attrs::GEN_AI_AGENT_NAME,
                value: jackin_telemetry::Value::Str("codex"),
            },
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::AGENT_STATE,
                value: jackin_telemetry::Value::Str("working"),
            },
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::AGENT_STATUS_SOURCE,
                value: jackin_telemetry::Value::Str("reported"),
            },
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::AGENT_STATUS_CONFIDENCE,
                value: jackin_telemetry::Value::Str("authoritative"),
            },
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::AGENT_STATUS_STUCK,
                value: jackin_telemetry::Value::Bool(true),
            },
        ];
        jackin_telemetry::emit_event(
            &jackin_telemetry::event::AGENT_STATE_CHANGED,
            jackin_telemetry::FieldSet::new(&agent, None),
        )
        .unwrap();

        jackin_telemetry::emit_event(
            &jackin_telemetry::event::TELEMETRY_VALIDATE,
            jackin_telemetry::FieldSet::default(),
        )
        .unwrap();
    });
    export.logger_provider.force_flush().unwrap();
    let logs = export.logs.get_emitted_logs().unwrap();
    assert_eq!(logs.len(), 3);
    assert_eq!(
        log_attribute(&logs[0].record, "app.jank.frame_count"),
        Some(&AnyValue::Int(7))
    );
    assert_eq!(
        log_attribute(&logs[0].record, "app.jank.period"),
        Some(&AnyValue::Double(0.25))
    );
    assert_eq!(
        log_attribute(&logs[1].record, "agent.status.stuck"),
        Some(&AnyValue::Boolean(true))
    );
    assert_eq!(logs[2].record.event_name(), Some("telemetry.validate"));
}

#[test]
fn every_registered_event_round_trips_once_with_canonical_severity() {
    use jackin_telemetry::schema::{RequirementLevel, ValueType};
    use opentelemetry::logs::Severity;

    static ARRAY_VALUE: &[&str] = &["proof"];
    let (export, subscriber) = test_layers_at("trace", "unused");
    tracing::subscriber::with_default(subscriber, || {
        for name in jackin_telemetry::schema::events::ALL {
            let definition = jackin_telemetry::event::definition(name)
                .expect("every generated event must have a facade definition");
            let metadata = jackin_telemetry::schema::events::definition(name)
                .expect("every generated event must have metadata");
            let attrs = metadata
                .attributes
                .iter()
                .filter(|attribute| attribute.requirement == RequirementLevel::Required)
                .map(|attribute| jackin_telemetry::Attr {
                    key: attribute.name,
                    value: match attribute.value_type {
                        ValueType::String => jackin_telemetry::Value::Str(
                            attribute.allowed_values.first().copied().unwrap_or("proof"),
                        ),
                        ValueType::Boolean => jackin_telemetry::Value::Bool(true),
                        ValueType::Integer => jackin_telemetry::Value::I64(1),
                        ValueType::Double => jackin_telemetry::Value::F64(1.0),
                        ValueType::StringArray => jackin_telemetry::Value::StrArray(ARRAY_VALUE),
                    },
                })
                .collect::<Vec<_>>();
            jackin_telemetry::emit_event(definition, jackin_telemetry::FieldSet::new(&attrs, None))
                .unwrap_or_else(|reason| panic!("{name} fixture rejected: {reason:?}"));
        }
    });
    export.logger_provider.force_flush().unwrap();
    let logs = export.logs.get_emitted_logs().unwrap();
    assert_eq!(logs.len(), jackin_telemetry::schema::events::ALL.len());
    for name in jackin_telemetry::schema::events::ALL {
        let matching = logs
            .iter()
            .filter(|log| log.record.event_name() == Some(*name))
            .collect::<Vec<_>>();
        assert_eq!(matching.len(), 1, "{name} delivery count");
        let expected = match jackin_telemetry::event::canonical_severity(name).unwrap() {
            jackin_telemetry::event::Severity::Trace => Severity::Trace,
            jackin_telemetry::event::Severity::Debug => Severity::Debug,
            jackin_telemetry::event::Severity::Info => Severity::Info,
            jackin_telemetry::event::Severity::Warn => Severity::Warn,
            jackin_telemetry::event::Severity::Error => Severity::Error,
        };
        assert_eq!(
            matching[0].record.severity_number(),
            Some(expected),
            "{name}"
        );
    }
}

#[test]
fn governed_operation_line_does_not_duplicate_active_run_log() {
    let (export, subscriber) = test_layers(false, "unused");
    tracing::subscriber::with_default(subscriber, || {
        let directory = tempfile::tempdir().expect("temporary diagnostics directory");
        let paths = jackin_core::JackinPaths::for_tests(directory.path());
        let run = crate::RunDiagnostics::start(
            &paths,
            false,
            "status",
            crate::ServiceIdentity::HOST_ONE_SHOT,
        )
        .expect("diagnostics run");
        let _active = run.activate();
        export.logs.reset();
        let attrs = [jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::OUTCOME,
            value: jackin_telemetry::Value::Str("success"),
        }];
        jackin_telemetry::emit_event(
            &jackin_telemetry::event::OPERATION_LOG,
            jackin_telemetry::FieldSet::new(&attrs, Some("one delivery")),
        )
        .expect("registered operation log");
    });
    export.logger_provider.force_flush().unwrap();
    let logs = export.logs.get_emitted_logs().unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].record.event_name(), Some("operation.log"));
}
