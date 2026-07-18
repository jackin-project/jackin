// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use opentelemetry::trace::TracerProvider as _;
use tracing_subscriber::prelude::*;

use super::*;

async fn exported_stream_outcomes(
    input: &[u8],
    output_open: bool,
) -> (Result<()>, Vec<String>, Vec<String>) {
    let exporter = opentelemetry_sdk::trace::InMemorySpanExporter::default();
    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    let subscriber = tracing_subscriber::registry()
        .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("mcp-stream-test")));
    let _guard = tracing::subscriber::set_default(subscriber);

    let (mut input_writer, input_reader) = tokio::io::duplex(1024);
    input_writer.write_all(input).await.unwrap();
    input_writer.shutdown().await.unwrap();
    let (output_writer, output_reader) = tokio::io::duplex(1024);
    if !output_open {
        drop(output_reader);
    }
    let result = run_with_io(input_reader, output_writer).await;
    provider.force_flush().unwrap();
    let spans = exporter
        .get_finished_spans()
        .unwrap()
        .into_iter()
        .filter(|span| span.name == jackin_telemetry::schema::spans::STREAM_OPERATION)
        .collect::<Vec<_>>();
    let outcomes = spans
        .iter()
        .filter_map(|span| {
            span.attributes
                .iter()
                .find(|attribute| {
                    attribute.key.as_str() == jackin_telemetry::schema::attrs::OUTCOME
                })
                .map(|attribute| attribute.value.as_str().into_owned())
        })
        .collect();
    let errors = spans
        .iter()
        .filter_map(|span| {
            span.attributes
                .iter()
                .find(|attribute| {
                    attribute.key.as_str() == jackin_telemetry::schema::attrs::std_attrs::ERROR_TYPE
                })
                .map(|attribute| attribute.value.as_str().into_owned())
        })
        .collect();
    (result, outcomes, errors)
}

#[tokio::test]
async fn stdio_stream_closes_successfully_on_eof() {
    let (result, outcomes, errors) = exported_stream_outcomes(&[], true).await;
    result.unwrap();
    assert_eq!(
        outcomes,
        [
            jackin_telemetry::schema::enums::OutcomeValue::Success
                .as_str()
                .to_owned(),
            jackin_telemetry::schema::enums::OutcomeValue::Success
                .as_str()
                .to_owned(),
        ]
    );
    assert!(errors.is_empty());
}

#[tokio::test]
async fn stdio_stream_closes_with_typed_error_on_write_failure() {
    let (result, outcomes, errors) = exported_stream_outcomes(
        br#"{"jsonrpc":"2.0","id":1,"method":"ping"}
"#,
        false,
    )
    .await;
    result.unwrap_err();
    assert!(
        outcomes.contains(
            &jackin_telemetry::schema::enums::OutcomeValue::Error
                .as_str()
                .to_owned()
        )
    );
    assert_eq!(
        errors,
        [jackin_telemetry::schema::enums::ErrorType::IoError
            .as_str()
            .to_owned()]
    );
}
