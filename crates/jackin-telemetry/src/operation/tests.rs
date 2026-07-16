// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use opentelemetry::trace::{
    SpanContext, SpanId, SpanKind, Status, TraceFlags, TraceId, TraceState, TracerProvider as _,
};
use tracing_subscriber::prelude::*;

use super::*;

fn exported_span(
    outcome: Option<schema::enums::OutcomeValue>,
    error_type: Option<schema::enums::ErrorType>,
) -> opentelemetry_sdk::trace::SpanData {
    let exporter = opentelemetry_sdk::trace::InMemorySpanExporter::default();
    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    let subscriber = tracing_subscriber::registry()
        .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("test")));
    tracing::subscriber::with_default(subscriber, || {
        let guard = operation(&PROCESS_COMMAND, &[]).expect("registered operation");
        if let Some(outcome) = outcome {
            guard.complete(outcome, error_type);
        } else {
            drop(guard);
        }
    });
    provider.force_flush().expect("flush");
    exporter
        .get_finished_spans()
        .expect("export")
        .pop()
        .expect("span")
}

fn exported_status(
    outcome: Option<schema::enums::OutcomeValue>,
    error_type: Option<schema::enums::ErrorType>,
) -> Status {
    exported_span(outcome, error_type).status
}

#[test]
fn outcome_status_mapping_is_explicit() {
    for outcome in [
        schema::enums::OutcomeValue::Success,
        schema::enums::OutcomeValue::Skip,
        schema::enums::OutcomeValue::Cancellation,
    ] {
        assert_eq!(exported_status(Some(outcome), None), Status::Unset);
    }
    for outcome in [
        schema::enums::OutcomeValue::Failure,
        schema::enums::OutcomeValue::Error,
        schema::enums::OutcomeValue::Timeout,
    ] {
        assert!(matches!(
            exported_status(Some(outcome), Some(schema::enums::ErrorType::Timeout)),
            Status::Error { .. }
        ));
    }
}

#[test]
fn completion_matrix_rejects_impossible_pairs() {
    use schema::enums::{ErrorType, OutcomeValue};
    assert!(valid_completion(OutcomeValue::Success, None));
    assert!(valid_completion(
        OutcomeValue::Success,
        Some(ErrorType::RecoveredDegradation)
    ));
    assert!(valid_completion(
        OutcomeValue::Failure,
        Some(ErrorType::RpcError)
    ));
    assert!(!valid_completion(OutcomeValue::Failure, None));
    assert!(!valid_completion(
        OutcomeValue::Success,
        Some(ErrorType::RpcError)
    ));
    assert!(!valid_completion(
        OutcomeValue::Cancellation,
        Some(ErrorType::DependencyCancelled)
    ));
}

#[test]
fn invalid_completion_exports_as_instrumentation_fault() {
    use schema::enums::{ErrorType, OutcomeValue};

    let span = exported_span(Some(OutcomeValue::Success), Some(ErrorType::RpcError));
    assert!(matches!(span.status, Status::Error { .. }));
    assert!(span.attributes.iter().any(|attribute| {
        attribute.key.as_str() == schema::attrs::OUTCOME
            && attribute.value.as_str() == OutcomeValue::Error.as_str()
    }));
    assert!(span.attributes.iter().any(|attribute| {
        attribute.key.as_str() == schema::attrs::std_attrs::ERROR_TYPE
            && attribute.value.as_str() == ErrorType::TelemetryInstrumentationFault.as_str()
    }));
}

#[test]
fn abandoned_guard_records_instrumentation_fault() {
    let span = exported_span(None, None);
    assert!(matches!(span.status, Status::Error { .. }));
    assert!(span.attributes.iter().any(|attribute| {
        attribute.key.as_str() == schema::attrs::std_attrs::ERROR_TYPE
            && attribute.value.as_str() == "telemetry_instrumentation_fault"
    }));
}

#[test]
fn rejected_initial_shape_exports_no_span() {
    let exporter = opentelemetry_sdk::trace::InMemorySpanExporter::default();
    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    let subscriber = tracing_subscriber::registry()
        .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("test")));
    tracing::subscriber::with_default(subscriber, || {
        assert_eq!(
            operation(
                &LAUNCH,
                &[Attr {
                    key: schema::attrs::LAUNCH_TARGET_KIND,
                    value: Value::Str("not-registered"),
                }]
            )
            .map(drop),
            Err(Rejection::InvalidValue)
        );
    });
    provider.force_flush().unwrap();
    assert!(exporter.get_finished_spans().unwrap().is_empty());
}

#[test]
fn operation_guard_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<OperationGuard>();
}

#[test]
fn rpc_server_honors_remote_parent_and_kind() {
    let exporter = opentelemetry_sdk::trace::InMemorySpanExporter::default();
    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    let subscriber = tracing_subscriber::registry()
        .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("test")));
    let trace_id = TraceId::from_hex("4bf92f3577b34da6a3ce929d0e0e4736").unwrap();
    let span_id = SpanId::from_hex("00f067aa0ba902b7").unwrap();
    let parent = SpanContext::new(
        trace_id,
        span_id,
        TraceFlags::SAMPLED,
        true,
        TraceState::default(),
    );
    tracing::subscriber::with_default(subscriber, || {
        let attrs = [
            Attr {
                key: schema::attrs::std_attrs::RPC_SYSTEM_NAME,
                value: Value::Str("jackin"),
            },
            Attr {
                key: schema::attrs::std_attrs::RPC_METHOD,
                value: Value::Str("jackin.host.Daemon/Status"),
            },
        ];
        operation_with_remote_parent(&RPC_SERVER, &attrs, &parent)
            .unwrap()
            .complete(schema::enums::OutcomeValue::Success, None);
    });
    provider.force_flush().unwrap();
    let span = exporter.get_finished_spans().unwrap().pop().unwrap();
    assert_eq!(span.span_context.trace_id(), trace_id);
    assert_eq!(span.parent_span_id, span_id);
    assert_eq!(span.span_kind, SpanKind::Server);
}

#[test]
fn connection_attempt_exports_bounded_peer_shape() {
    use schema::enums::{ConnectionPeerType, ErrorType, OutcomeValue};

    let exporter = opentelemetry_sdk::trace::InMemorySpanExporter::default();
    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    let subscriber = tracing_subscriber::registry()
        .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("test")));
    tracing::subscriber::with_default(subscriber, || {
        let attrs = [Attr {
            key: schema::attrs::CONNECTION_PEER_TYPE,
            value: Value::Str(ConnectionPeerType::CapsuleControl.as_str()),
        }];
        operation(&CONNECTION_ATTEMPT, &attrs)
            .unwrap()
            .complete(OutcomeValue::Failure, Some(ErrorType::ConnectionRefused));
    });
    provider.force_flush().unwrap();
    let span = exporter.get_finished_spans().unwrap().pop().unwrap();
    assert_eq!(span.name, schema::spans::CONNECTION_ATTEMPT);
    assert_eq!(span.span_kind, SpanKind::Client);
    let keys = span
        .attributes
        .iter()
        .map(|attribute| attribute.key.as_str())
        .collect::<Vec<_>>();
    assert!(keys.contains(&schema::attrs::CONNECTION_PEER_TYPE));
    assert!(keys.contains(&schema::attrs::OUTCOME));
    assert!(keys.contains(&schema::attrs::std_attrs::ERROR_TYPE));
    for forbidden in [
        "file.path",
        "process.command_args",
        "process.command_line",
        "process.output",
        "server.address",
    ] {
        assert!(!keys.contains(&forbidden));
    }
}

#[test]
fn connection_metric_dimensions_are_bounded() {
    let attempts = crate::metric::CONNECTION_ATTEMPTS
        .dimensions()
        .iter()
        .map(|attribute| attribute.name)
        .collect::<Vec<_>>();
    assert_eq!(
        attempts,
        [
            schema::attrs::CONNECTION_PEER_TYPE,
            schema::attrs::std_attrs::ERROR_TYPE,
            schema::attrs::OUTCOME,
        ]
    );
    assert_eq!(
        crate::metric::CONNECTION_ACTIVE.dimensions()[0].name,
        schema::attrs::CONNECTION_PEER_TYPE
    );
    assert_eq!(
        crate::metric::CONNECTION_DURATION
            .dimensions()
            .iter()
            .map(|attribute| attribute.name)
            .collect::<Vec<_>>(),
        attempts
    );
}
