use super::*;

fn authenticated<T>(message: T) -> Request<T> {
    let mut request = Request::new(message);
    request.metadata_mut().insert(
        "authorization",
        "Bearer capsule-safe".parse().expect("valid metadata"),
    );
    request
}

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

    testbed.set_behavior(Behavior::RequireHeader {
        name: "authorization",
        value: "Bearer capsule-safe",
    });
    let error = traces
        .export(ExportTraceServiceRequest::default())
        .await
        .expect_err("missing authentication metadata");
    assert_eq!(error.code(), tonic::Code::Unauthenticated);
    traces
        .export(authenticated(ExportTraceServiceRequest::default()))
        .await
        .expect("authenticated trace export");
    logs.export(authenticated(ExportLogsServiceRequest::default()))
        .await
        .expect("authenticated log export");
    metrics
        .export(authenticated(ExportMetricsServiceRequest::default()))
        .await
        .expect("authenticated metric export");
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

#[test]
fn namespace_detector_scans_scope_and_metric_exemplar_metadata() {
    use opentelemetry_proto::tonic::common::v1::{InstrumentationScope, KeyValue};
    use opentelemetry_proto::tonic::metrics::v1::{
        Exemplar, Gauge, Metric, NumberDataPoint, metric::Data,
    };

    let scope = InstrumentationScope {
        attributes: vec![KeyValue {
            key: "jackin.scope".to_owned(),
            ..Default::default()
        }],
        ..Default::default()
    };
    let metric = Metric {
        data: Some(Data::Gauge(Gauge {
            data_points: vec![NumberDataPoint {
                exemplars: vec![Exemplar {
                    filtered_attributes: vec![KeyValue {
                        key: "parallax.exemplar".to_owned(),
                        ..Default::default()
                    }],
                    ..Default::default()
                }],
                ..Default::default()
            }],
        })),
        ..Default::default()
    };
    let mut violations = Vec::new();

    scan_scope(Some(&scope), "", &mut violations);
    scan_metric_points(metric.data.as_ref(), &mut violations);

    assert_eq!(violations, ["jackin.scope", "parallax.exemplar"]);
}

#[test]
fn privacy_detector_scans_links_scopes_and_metric_exemplars() {
    use opentelemetry_proto::tonic::common::v1::{AnyValue, InstrumentationScope, KeyValue};
    use opentelemetry_proto::tonic::metrics::v1::{Exemplar, Gauge, NumberDataPoint, metric::Data};
    use opentelemetry_proto::tonic::trace::v1::{Span, span::Link};

    let secret = |text: &str| KeyValue {
        key: "fixture.key".to_owned(),
        value: Some(AnyValue {
            value: Some(
                opentelemetry_proto::tonic::common::v1::any_value::Value::StringValue(
                    text.to_owned(),
                ),
            ),
        }),
        ..Default::default()
    };
    let scope = InstrumentationScope {
        attributes: vec![secret("scope-secret")],
        ..Default::default()
    };
    let span = Span {
        links: vec![Link {
            attributes: vec![secret("link-secret")],
            ..Default::default()
        }],
        ..Default::default()
    };
    let metric = Data::Gauge(Gauge {
        data_points: vec![NumberDataPoint {
            exemplars: vec![Exemplar {
                filtered_attributes: vec![secret("exemplar-secret")],
                ..Default::default()
            }],
            ..Default::default()
        }],
    });
    let prohibited = ["scope-secret", "link-secret", "exemplar-secret"];
    let mut violations = Vec::new();

    scan_scope_values(Some(&scope), "", &prohibited, &mut violations);
    scan_span_values(&span, &prohibited, &mut violations);
    scan_metric_point_values(Some(&metric), &prohibited, &mut violations);

    assert_eq!(violations, prohibited);
}
