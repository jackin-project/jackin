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

#[tokio::test(flavor = "current_thread")]
async fn waits_for_export_ack_before_reporting_a_trace() {
    let mut testbed = Testbed::start().expect("start testbed");
    testbed.set_behavior(Behavior::Delay(std::time::Duration::from_millis(100)));
    let mut traces = opentelemetry_proto::tonic::collector::trace::v1::
        trace_service_client::TraceServiceClient::connect(testbed.endpoint())
        .await
        .expect("connect trace client");
    let export = tokio::spawn(async move {
        traces
            .export(ExportTraceServiceRequest {
                resource_spans: vec![opentelemetry_proto::tonic::trace::v1::ResourceSpans {
                    scope_spans: vec![opentelemetry_proto::tonic::trace::v1::ScopeSpans {
                        spans: vec![opentelemetry_proto::tonic::trace::v1::Span {
                            name: "delayed.trace".to_owned(),
                            ..Default::default()
                        }],
                        ..Default::default()
                    }],
                    ..Default::default()
                }],
            })
            .await
            .expect("delayed trace export");
    });

    assert!(
        testbed
            .wait_for_trace_request(std::time::Duration::from_secs(1))
            .await
    );
    assert!(
        !testbed
            .wait_for_span_count("delayed.trace", 1, std::time::Duration::from_millis(20))
            .await
    );
    assert!(
        testbed
            .wait_for_span_count("delayed.trace", 1, std::time::Duration::from_secs(1))
            .await
    );
    export.await.expect("export task");
    testbed.shutdown().await.expect("join testbed receiver");
}

#[tokio::test(flavor = "current_thread")]
async fn zero_span_wait_is_immediate() {
    let testbed = Testbed::start().expect("start testbed");
    assert!(
        testbed
            .wait_for_span_count("unused.trace", 0, std::time::Duration::ZERO)
            .await
    );
}

#[tokio::test(flavor = "current_thread")]
async fn shutdown_forces_an_open_delayed_client_after_grace_timeout() {
    let mut testbed = Testbed::start().expect("start testbed");
    testbed.set_behavior(Behavior::Delay(std::time::Duration::from_secs(30)));
    let mut traces = opentelemetry_proto::tonic::collector::trace::v1::
        trace_service_client::TraceServiceClient::connect(testbed.endpoint())
        .await
        .expect("connect trace client");
    let export =
        tokio::spawn(async move { traces.export(ExportTraceServiceRequest::default()).await });

    assert!(
        testbed
            .wait_for_trace_request(std::time::Duration::from_secs(1))
            .await
    );
    let shutdown = tokio::time::timeout(std::time::Duration::from_secs(2), testbed.shutdown())
        .await
        .expect("forced shutdown must be bounded");
    assert!(matches!(shutdown, Err(ShutdownError::Timeout)));

    let export = tokio::time::timeout(std::time::Duration::from_secs(1), export)
        .await
        .expect("forced shutdown must release the delayed client")
        .expect("delayed export task");
    assert!(
        export.is_err(),
        "forced shutdown must cancel the delayed export"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn shutdown_closes_a_half_open_connection_task() {
    let mut testbed = Testbed::start().expect("start testbed");
    let client = TcpStream::connect(testbed.addr)
        .await
        .expect("connect half-open client");

    assert!(
        testbed
            .connections
            .wait_for_connection(std::time::Duration::from_secs(1))
            .await,
        "testbed must register the accepted connection before shutdown"
    );
    assert!(
        testbed
            .connections
            .wait_for_io_poll(std::time::Duration::from_secs(1))
            .await,
        "Tonic must poll the detached connection before shutdown"
    );

    let shutdown = tokio::time::timeout(std::time::Duration::from_secs(2), testbed.shutdown())
        .await
        .expect("half-open connection shutdown must be bounded");
    assert!(matches!(shutdown, Err(ShutdownError::Timeout)));
    assert_eq!(
        testbed.connections.active_count(),
        0,
        "forced shutdown must release every detached Tonic connection"
    );
    assert!(
        testbed.traces().is_empty(),
        "an incomplete stream must not reach the export service"
    );
    drop(client);
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
