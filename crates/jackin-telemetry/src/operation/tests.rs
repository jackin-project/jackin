// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use opentelemetry::trace::{
    SpanContext, SpanId, SpanKind, Status, TraceFlags, TraceId, TraceState, TracerProvider as _,
};
use tracing_subscriber::prelude::*;

use super::*;

fn exported_status(
    outcome: Option<schema::enums::OutcomeValue>,
    error_type: Option<&'static str>,
) -> Status {
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
        .status
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
            exported_status(Some(outcome), Some("timeout")),
            Status::Error { .. }
        ));
    }
}

#[test]
fn drop_records_cancellation_without_error_status() {
    assert_eq!(exported_status(None, None), Status::Unset);
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
        operation_with_remote_parent(&RPC_SERVER, &[], &parent)
            .unwrap()
            .complete(schema::enums::OutcomeValue::Success, None);
    });
    provider.force_flush().unwrap();
    let span = exporter.get_finished_spans().unwrap().pop().unwrap();
    assert_eq!(span.span_context.trace_id(), trace_id);
    assert_eq!(span.parent_span_id, span_id);
    assert_eq!(span.span_kind, SpanKind::Server);
}
