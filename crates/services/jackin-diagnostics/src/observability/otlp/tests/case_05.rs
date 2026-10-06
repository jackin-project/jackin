// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn conformance_no_lifetime_spans() {
    use std::time::Duration;

    let (export, subscriber) = test_layers(false, "unused");
    let idle = Duration::from_millis(80);
    tracing::subscriber::with_default(subscriber, || {
        for (definition, emit_session_start) in [
            (&jackin_telemetry::operation::APP_STARTUP, false),
            (&jackin_telemetry::operation::CLI_COMMAND, true),
        ] {
            let operation = jackin_telemetry::root_operation(definition, &cli_command_test_attrs())
                .expect("bounded operation");
            let entered = operation.span().enter();
            if emit_session_start {
                let attrs = [
                    jackin_telemetry::Attr {
                        key: jackin_telemetry::schema::attrs::std_attrs::SESSION_ID,
                        value: jackin_telemetry::Value::Str("session-proof"),
                    },
                    jackin_telemetry::Attr {
                        key: jackin_telemetry::schema::attrs::CLI_INVOCATION_ID,
                        value: jackin_telemetry::Value::Str("invocation-test"),
                    },
                ];
                jackin_telemetry::emit_event(
                    &jackin_telemetry::event::SESSION_START,
                    jackin_telemetry::FieldSet::new(&attrs, None),
                )
                .unwrap();
            }
            drop(entered);
            operation.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);
        }

        std::thread::park_timeout(idle);

        let shutdown = jackin_telemetry::root_operation(
            &jackin_telemetry::operation::APP_SHUTDOWN,
            &cli_command_test_attrs(),
        )
        .expect("bounded shutdown");
        shutdown.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);
    });
    export.logger_provider.force_flush().unwrap();
    export.tracer_provider.force_flush().unwrap();

    let spans = export.spans.get_finished_spans().unwrap();
    assert_eq!(spans.len(), 3);
    assert!(spans.iter().all(|span| {
        !matches!(
            span.name.as_ref(),
            "process" | "invocation" | "session" | "console.session" | "capsule.session"
        )
    }));
    assert!(
        spans
            .iter()
            .all(|span| { span.end_time.duration_since(span.start_time).unwrap() < idle / 2 }),
        "no bounded operation may cover the idle session interval"
    );
    assert!(
        spans
            .iter()
            .all(|span| span.attributes.iter().any(|attribute| {
                attribute.key.as_str() == jackin_telemetry::schema::attrs::CLI_INVOCATION_ID
                    && attribute.value.as_str() == "invocation-test"
            }))
    );
    let logs = export.logs.get_emitted_logs().unwrap();
    let session_start = logs
        .iter()
        .find(|log| log.record.event_name() == Some("session.start"))
        .expect("in-session log");
    assert_eq!(
        log_attribute(
            &session_start.record,
            jackin_telemetry::schema::attrs::CLI_INVOCATION_ID,
        ),
        Some(&opentelemetry::logs::AnyValue::String(
            "invocation-test".into()
        ))
    );
    assert!(
        session_start
            .resource
            .get(&opentelemetry::Key::from_static_str(
                jackin_telemetry::schema::attrs::CLI_INVOCATION_ID,
            ))
            .is_none(),
        "provider Resource must not contain invocation identity"
    );
    assert_eq!(
        jackin_telemetry::counter(&jackin_telemetry::metric::TELEMETRY_VALIDATE).add(
            1,
            &[jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::CLI_INVOCATION_ID,
                value: jackin_telemetry::Value::Str("invocation-test"),
            }],
        ),
        Err(jackin_telemetry::Rejection::Cardinality)
    );
}

#[test]
fn metric_export_contract_rejects_names_shapes_and_dimensions() {
    use jackin_telemetry::Rejection;

    assert_eq!(
        metric_contract_fields("unknown.metric", "unknown", "1"),
        Err(Rejection::UnknownName)
    );
    assert_eq!(
        metric_contract_fields(
            jackin_telemetry::schema::metrics::UI_JANK,
            "forged description",
            "{crossing}",
        ),
        Err(Rejection::InvalidValue)
    );

    let requirements = jackin_telemetry::schema::metrics::UI_JANK_DEF.attributes;
    let valid = [opentelemetry::KeyValue::new(
        "app.screen.id",
        "workspace.list",
    )];
    assert_eq!(
        validate_metric_attributes(requirements, valid.iter()),
        Ok(())
    );
    let wrong_type = [opentelemetry::KeyValue::new("app.screen.id", true)];
    assert_eq!(
        validate_metric_attributes(requirements, wrong_type.iter()),
        Err(Rejection::InvalidValue)
    );
    let unknown = [opentelemetry::KeyValue::new("bogus.secret", "secret")];
    assert_eq!(
        validate_metric_attributes(requirements, unknown.iter()),
        Err(Rejection::UnknownAttribute)
    );
    let sensitive = [opentelemetry::KeyValue::new(
        "app.screen.id",
        "/private/workspace",
    )];
    assert_eq!(
        validate_metric_attributes(requirements, sensitive.iter()),
        Err(Rejection::Privacy)
    );
    let duplicate = [
        opentelemetry::KeyValue::new("app.screen.id", "workspace.list"),
        opentelemetry::KeyValue::new("app.screen.id", "workspace.list"),
    ];
    assert_eq!(
        validate_metric_attributes(requirements, duplicate.iter()),
        Err(Rejection::InvalidValue)
    );
    assert_eq!(
        validate_metric_attributes(requirements, std::iter::empty()),
        Err(Rejection::InvalidValue)
    );
    let excessive = (0..=jackin_telemetry::limits::MAX_METRIC_ATTRIBUTES)
        .map(|_| opentelemetry::KeyValue::new("app.screen.id", "workspace.list"))
        .collect::<Vec<_>>();
    assert_eq!(
        validate_metric_attributes(requirements, excessive.iter()),
        Err(Rejection::SizeLimit)
    );
    let oversized = [opentelemetry::KeyValue::new(
        "app.screen.id",
        "x".repeat(jackin_telemetry::limits::MAX_STRING_ATTRIBUTE_BYTES + 1),
    )];
    assert_eq!(
        validate_metric_attributes(requirements, oversized.iter()),
        Err(Rejection::SizeLimit)
    );
    assert_eq!(
        validate_metric_points(
            0..=jackin_telemetry::limits::MAX_CARDINALITY,
            |_| Vec::new(),
            &[],
        ),
        Err(Rejection::Cardinality)
    );
}

#[test]
fn governed_raw_meter_rejects_every_metric_contract_class() {
    use opentelemetry::{Array, KeyValue, Value};

    assert_raw_metric_batch_rejected(jackin_telemetry::Rejection::UnknownName, |meter| {
        meter.u64_counter("unknown.metric").build().add(1, &[]);
    });
    assert_raw_metric_batch_rejected(jackin_telemetry::Rejection::InvalidValue, |meter| {
        meter
            .f64_histogram(jackin_telemetry::schema::metrics::UI_JANK)
            .with_description(jackin_telemetry::schema::metrics::UI_JANK_DEF.description)
            .with_unit(jackin_telemetry::schema::metrics::UI_JANK_DEF.unit)
            .build()
            .record(1.0, &[KeyValue::new("app.screen.id", "workspace.list")]);
    });
    assert_raw_metric_batch_rejected(jackin_telemetry::Rejection::UnknownAttribute, |meter| {
        meter
            .u64_counter(jackin_telemetry::schema::metrics::UI_JANK)
            .with_description(jackin_telemetry::schema::metrics::UI_JANK_DEF.description)
            .with_unit(jackin_telemetry::schema::metrics::UI_JANK_DEF.unit)
            .build()
            .add(1, &[KeyValue::new("bogus.secret", "bounded")]);
    });
    assert_raw_metric_batch_rejected(jackin_telemetry::Rejection::Privacy, |meter| {
        meter
            .u64_counter(jackin_telemetry::schema::metrics::UI_JANK)
            .with_description(jackin_telemetry::schema::metrics::UI_JANK_DEF.description)
            .with_unit(jackin_telemetry::schema::metrics::UI_JANK_DEF.unit)
            .build()
            .add(1, &[KeyValue::new("app.screen.id", "/private/workspace")]);
    });
    assert_raw_metric_batch_rejected(jackin_telemetry::Rejection::SizeLimit, |meter| {
        meter
            .u64_counter(jackin_telemetry::schema::metrics::UI_JANK)
            .with_description(jackin_telemetry::schema::metrics::UI_JANK_DEF.description)
            .with_unit(jackin_telemetry::schema::metrics::UI_JANK_DEF.unit)
            .build()
            .add(
                1,
                &[KeyValue::new(
                    "app.screen.id",
                    "x".repeat(jackin_telemetry::limits::MAX_STRING_ATTRIBUTE_BYTES + 1),
                )],
            );
    });
    assert_raw_metric_batch_rejected(jackin_telemetry::Rejection::InvalidValue, |meter| {
        meter
            .u64_counter(jackin_telemetry::schema::metrics::UI_JANK)
            .with_description(jackin_telemetry::schema::metrics::UI_JANK_DEF.description)
            .with_unit(jackin_telemetry::schema::metrics::UI_JANK_DEF.unit)
            .build()
            .add(
                1,
                &[KeyValue::new(
                    "app.screen.id",
                    Value::Array(Array::String(vec!["workspace.list".into()])),
                )],
            );
    });
    assert_raw_metric_batch_rejected(jackin_telemetry::Rejection::Cardinality, |meter| {
        let histogram = meter
            .f64_histogram(jackin_telemetry::schema::metrics::UI_FOCUS_DURATION)
            .with_description(jackin_telemetry::schema::metrics::UI_FOCUS_DURATION_DEF.description)
            .with_unit(jackin_telemetry::schema::metrics::UI_FOCUS_DURATION_DEF.unit)
            .build();
        for index in 0..=jackin_telemetry::limits::MAX_CARDINALITY {
            histogram.record(
                0.001,
                &[
                    KeyValue::new("app.screen.id", "workspace.list"),
                    KeyValue::new("app.widget.id", format!("widget-{index}")),
                ],
            );
        }
    });
}

#[test]
fn rejected_metric_collection_is_not_reported_as_exported() {
    let _lock = crate::DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let facade_before = jackin_telemetry::facade_health();
    let export_before = crate::telemetry_health_snapshot();
    let result = governed_metric_export_result(Err(jackin_telemetry::Rejection::UnknownName));

    assert!(matches!(
        result,
        Err(opentelemetry_sdk::error::OTelSdkError::InternalFailure(message))
            if message == "metric export rejected by telemetry governance"
    ));
    let facade_after = jackin_telemetry::facade_health();
    let export_after = crate::telemetry_health_snapshot();
    assert_eq!(
        facade_after.by_signal_reason[jackin_telemetry::Signal::Metric as usize]
            [jackin_telemetry::Rejection::UnknownName as usize],
        facade_before.by_signal_reason[jackin_telemetry::Signal::Metric as usize]
            [jackin_telemetry::Rejection::UnknownName as usize]
            + 1
    );
    assert_eq!(
        export_after.metrics.attempts,
        export_before.metrics.attempts + 1
    );
    assert_eq!(
        export_after.metrics.successes,
        export_before.metrics.successes
    );
    assert_eq!(
        export_after.metrics.failures,
        export_before.metrics.failures + 1
    );
}
