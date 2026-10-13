// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use opentelemetry::trace::TracerProvider as _;
use tracing_subscriber::prelude::*;

use super::*;

#[test]
fn drop_records_cancellation_without_error_status() {
    let exporter = opentelemetry_sdk::trace::InMemorySpanExporter::default();
    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    let subscriber = tracing_subscriber::registry()
        .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("stream-drop-test")));

    tracing::subscriber::with_default(subscriber, || drop(close_on_drop()));
    provider.force_flush().unwrap();

    let spans = exporter.get_finished_spans().unwrap();
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].status, opentelemetry::trace::Status::Unset);
    assert!(spans[0].attributes.iter().any(|attribute| {
        attribute.key.as_str() == schema::attrs::OUTCOME
            && attribute.value.as_str() == schema::enums::OutcomeValue::Cancellation.as_str()
    }));
    assert!(
        spans[0]
            .attributes
            .iter()
            .all(|attribute| attribute.key.as_str() != schema::attrs::std_attrs::ERROR_TYPE)
    );
}

#[test]
fn close_marker_records_success_and_cancellation_without_lifetime_span() {
    let exporter = opentelemetry_sdk::trace::InMemorySpanExporter::default();
    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    let subscriber = tracing_subscriber::registry()
        .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("stream-close-test")));
    tracing::subscriber::with_default(subscriber, || {
        close_on_drop().complete_success();
        drop(close_on_drop());
    });
    provider.force_flush().unwrap();

    let spans = exporter.get_finished_spans().unwrap();
    assert_eq!(spans.len(), 2);
    let outcomes = spans
        .iter()
        .map(|span| {
            assert_eq!(span.name, schema::spans::STREAM_OPERATION);
            assert_eq!(span.status, opentelemetry::trace::Status::Unset);
            assert!(span.attributes.iter().any(|attribute| {
                attribute.key.as_str() == schema::attrs::STREAM_OPERATION
                    && attribute.value.as_str() == schema::enums::StreamOperation::Close.as_str()
            }));
            span.attributes
                .iter()
                .find(|attribute| attribute.key.as_str() == schema::attrs::OUTCOME)
                .map(|attribute| attribute.value.as_str().into_owned())
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert!(outcomes.contains(&schema::enums::OutcomeValue::Success.as_str().to_owned()));
    assert!(
        outcomes.contains(
            &schema::enums::OutcomeValue::Cancellation
                .as_str()
                .to_owned()
        )
    );
}
