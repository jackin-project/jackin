// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn exported_span(
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

pub(super) fn exported_status(
    outcome: Option<schema::enums::OutcomeValue>,
    error_type: Option<schema::enums::ErrorType>,
) -> Status {
    exported_span(outcome, error_type).status
}
