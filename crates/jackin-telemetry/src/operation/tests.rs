// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use opentelemetry::trace::{Status, TracerProvider as _};
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
