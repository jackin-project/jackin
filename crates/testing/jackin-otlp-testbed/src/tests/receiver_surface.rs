// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use opentelemetry_proto::tonic::collector::trace::v1::{
    ExportTraceServiceRequest, trace_service_client::TraceServiceClient,
};

use crate::{Behavior, ShutdownError, Testbed};

type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

#[tokio::test]
async fn exports_trace_request_to_testbed() -> TestResult {
    let mut bed = Testbed::start()?;
    assert!(bed.endpoint().starts_with("http://127.0.0.1:"));
    assert!(bed.traces().is_empty());
    let mut client = TraceServiceClient::connect(bed.endpoint()).await?;
    let response = client.export(ExportTraceServiceRequest::default()).await?;
    assert!(response.into_inner().partial_success.is_none());
    assert_eq!(bed.traces().len(), 1);
    assert!(bed.spans().is_empty());
    bed.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn partial_success_reports_rejected_spans() -> TestResult {
    let mut bed = Testbed::start()?;
    bed.set_behavior(Behavior::PartialSuccess);
    let mut client = TraceServiceClient::connect(bed.endpoint()).await?;
    let response = client.export(ExportTraceServiceRequest::default()).await?;
    let rejected = match response.into_inner().partial_success {
        Some(partial) => partial.rejected_spans,
        None => return Err("partial success was expected".into()),
    };
    assert_eq!(rejected, 1);
    assert_eq!(bed.traces().len(), 1);
    bed.shutdown().await?;
    Ok(())
}

#[test]
fn timeout_shutdown_explains_the_deadline() {
    assert!(ShutdownError::Timeout.to_string().contains("timed out"));
}
