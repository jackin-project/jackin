// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Bounded stream and watcher lifecycle phases.

use crate::{Attr, OperationGuard, Value, autonomous_root_operation, operation, schema};

#[must_use]
pub fn phase(phase: schema::enums::StreamOperation) -> Option<OperationGuard> {
    let attrs = [Attr {
        key: schema::attrs::STREAM_OPERATION,
        value: Value::Str(phase.as_str()),
    }];
    autonomous_root_operation(&operation::STREAM_OPERATION, &attrs).ok()
}

pub fn complete_success(operation: Option<OperationGuard>) {
    if let Some(operation) = operation {
        operation.complete(schema::enums::OutcomeValue::Success, None);
    }
}

pub fn complete_error(operation: Option<OperationGuard>, error_type: schema::enums::ErrorType) {
    if let Some(operation) = operation {
        operation.complete(schema::enums::OutcomeValue::Error, Some(error_type));
    }
}

#[derive(Debug)]
pub struct CloseOnDrop {
    completed: bool,
}

#[must_use]
pub const fn close_on_drop() -> CloseOnDrop {
    CloseOnDrop { completed: false }
}

impl CloseOnDrop {
    pub fn complete_success(mut self) {
        self.completed = true;
        complete_success(phase(schema::enums::StreamOperation::Close));
    }

    pub fn complete_error(mut self, error_type: schema::enums::ErrorType) {
        self.completed = true;
        complete_error(phase(schema::enums::StreamOperation::Close), error_type);
    }
}

impl Drop for CloseOnDrop {
    fn drop(&mut self) {
        if !self.completed
            && let Some(operation) = phase(schema::enums::StreamOperation::Close)
        {
            operation.complete(schema::enums::OutcomeValue::Cancellation, None);
        }
    }
}

#[cfg(test)]
mod tests {
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
                        && attribute.value.as_str()
                            == schema::enums::StreamOperation::Close.as_str()
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
}
