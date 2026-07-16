use super::*;

#[tokio::test(flavor = "current_thread")]
async fn serves_all_three_otlp_services() {
    let testbed = Testbed::start().expect("start testbed");
    assert!(testbed.endpoint().starts_with("http://127.0.0.1:"));

    let mut traces = opentelemetry_proto::tonic::collector::trace::v1::
        trace_service_client::TraceServiceClient::connect(testbed.endpoint())
        .await
        .expect("connect trace client");
    traces
        .export(ExportTraceServiceRequest::default())
        .await
        .expect("export traces");
    let mut logs = opentelemetry_proto::tonic::collector::logs::v1::
        logs_service_client::LogsServiceClient::connect(testbed.endpoint())
        .await
        .expect("connect logs client");
    logs.export(ExportLogsServiceRequest::default())
        .await
        .expect("export logs");
    let mut metrics = opentelemetry_proto::tonic::collector::metrics::v1::
        metrics_service_client::MetricsServiceClient::connect(testbed.endpoint())
        .await
        .expect("connect metrics client");
    metrics
        .export(ExportMetricsServiceRequest::default())
        .await
        .expect("export metrics");
    assert_eq!(testbed.traces().len(), 1);
    assert_eq!(testbed.logs().len(), 1);
    assert_eq!(testbed.metrics().len(), 1);

    testbed.set_behavior(Behavior::Reject(tonic::Code::Unavailable));
    let error = traces
        .export(ExportTraceServiceRequest::default())
        .await
        .expect_err("scripted rejection");
    assert_eq!(error.code(), tonic::Code::Unavailable);

    testbed.set_behavior(Behavior::PartialSuccess);
    let response = traces
        .export(ExportTraceServiceRequest::default())
        .await
        .expect("partial success is a successful gRPC response")
        .into_inner();
    assert_eq!(
        response
            .partial_success
            .map(|partial| partial.rejected_spans),
        Some(1)
    );
}

#[test]
fn namespace_detector_rejects_synthetic_legacy_attribute() {
    let attributes = [opentelemetry_proto::tonic::common::v1::KeyValue {
        key: "jackin.synthetic".to_owned(),
        ..Default::default()
    }];
    let mut violations = Vec::new();
    scan_attributes(&attributes, &mut violations);
    assert_eq!(violations, ["jackin.synthetic"]);
}

#[test]
fn privacy_detector_rejects_nested_synthetic_value() {
    let value = opentelemetry_proto::tonic::common::v1::AnyValue {
        value: Some(
            opentelemetry_proto::tonic::common::v1::any_value::Value::StringValue(
                "authorization=Bearer fixture-secret".to_owned(),
            ),
        ),
    };
    let mut violations = Vec::new();
    scan_any_value(Some(&value), &["fixture-secret"], &mut violations);
    assert_eq!(violations, ["fixture-secret"]);
}
