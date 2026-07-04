// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use opentelemetry::Key;

use super::keys;
use super::{
    build_resource, grpc_endpoint, resolve_endpoint, resolve_endpoints, unsupported_protocol,
};

fn attr(resource: &opentelemetry_sdk::Resource, key: &'static str) -> Option<String> {
    resource
        .get(&Key::from_static_str(key))
        .map(|value| value.to_string())
}

#[test]
fn grpc_endpoint_strips_trailing_slashes_and_keeps_path_free() {
    // gRPC routes by service name: the endpoint is the channel target,
    // verbatim apart from trailing-slash normalization. No `/v1/*`.
    assert_eq!(
        grpc_endpoint("http://127.0.0.1:4317"),
        "http://127.0.0.1:4317"
    );
    assert_eq!(
        grpc_endpoint("http://127.0.0.1:4317/"),
        "http://127.0.0.1:4317"
    );
    assert_eq!(
        grpc_endpoint("http://127.0.0.1:4317//"),
        "http://127.0.0.1:4317"
    );
}

#[test]
fn only_grpc_protocol_is_accepted() {
    assert!(!unsupported_protocol(""));
    assert!(!unsupported_protocol("grpc"));
    assert!(!unsupported_protocol("  grpc  "));
    assert!(unsupported_protocol("http/protobuf"));
    assert!(unsupported_protocol("http/json"));
}

#[test]
fn endpoint_empty_filtering() {
    // A configured endpoint resolves.
    assert_eq!(
        resolve_endpoint(Some("http://otel:4317".into())),
        Some("http://otel:4317".into())
    );
    // An exported-but-empty var → None (no malformed exporter against "").
    assert_eq!(resolve_endpoint(Some(String::new())), None);
    // Unset → None (no OTLP layer installed).
    assert_eq!(resolve_endpoint(None), None);
}

#[test]
fn generic_endpoint_resolves_all_signals() {
    // One base drives every signal verbatim — gRPC appends no path.
    let endpoints = resolve_endpoints(Some("http://otel:4317/".into()), None, None, None).unwrap();

    assert_eq!(endpoints.traces, "http://otel:4317");
    assert_eq!(endpoints.logs, "http://otel:4317");
    assert_eq!(endpoints.metrics.as_deref(), Some("http://otel:4317"));
}

#[test]
fn per_signal_endpoints_enable_host_export() {
    let endpoints = resolve_endpoints(
        None,
        Some("http://traces:4317".into()),
        Some("http://logs:4317".into()),
        Some("http://metrics:4317".into()),
    )
    .unwrap();

    assert_eq!(endpoints.traces, "http://traces:4317");
    assert_eq!(endpoints.logs, "http://logs:4317");
    assert_eq!(endpoints.metrics.as_deref(), Some("http://metrics:4317"));
}

#[test]
fn per_signal_endpoints_do_not_require_metrics() {
    let endpoints = resolve_endpoints(
        None,
        Some("http://traces:4317".into()),
        Some("http://logs:4317".into()),
        None,
    )
    .unwrap();

    assert_eq!(endpoints.traces, "http://traces:4317");
    assert_eq!(endpoints.logs, "http://logs:4317");
    assert_eq!(endpoints.metrics, None);
}

#[test]
fn resource_carries_service_name_run_id_and_component() {
    let resource = build_resource("0a1b2c");
    assert_eq!(attr(&resource, keys::SERVICE_NAME), Some("jackin".into()));
    assert_eq!(attr(&resource, keys::COMPONENT), Some("host".into()));
    // The single dotted run-id key is parallax.run.id (no jackin.run.id).
    assert_eq!(keys::RUN_ID, "parallax.run.id");
    assert_eq!(attr(&resource, keys::RUN_ID), Some("0a1b2c".into()));
}

#[test]
fn adopted_wrapper_run_id_is_stamped_on_resource() {
    let resource = build_resource("18b946258b86fe20");
    assert_eq!(
        attr(&resource, keys::RUN_ID),
        Some("18b946258b86fe20".into())
    );
    assert_eq!(attr(&resource, keys::COMPONENT), Some("host".into()));
}

#[test]
fn metrics_only_endpoint_is_incomplete() {
    // Only a metrics endpoint, no base/traces/logs: traces+logs are
    // mandatory, so the whole config resolves to None. The caller surfaces
    // this rather than silently treating export as never requested.
    assert_eq!(
        resolve_endpoints(None, None, None, Some("http://metrics:4317".into())),
        None
    );
}

#[test]
fn otel_internal_visitor_flattens_name_message_and_fields() {
    use super::super::OtelInternalVisitor;
    let mut visitor = OtelInternalVisitor::default();
    visitor.record_field("name", "ExportFailed".to_owned());
    visitor.record_field("error", "connection refused".to_owned());
    visitor.record_field("message", "export failed".to_owned());
    // `name` first, then `message` (hoisted to the front of the ad-hoc
    // fields), then remaining fields as `key=value`.
    assert_eq!(
        visitor.into_message(),
        "ExportFailed export failed error=connection refused"
    );
}

#[test]
fn otel_internal_visitor_empty_uses_fallback() {
    use super::super::OtelInternalVisitor;
    assert_eq!(
        OtelInternalVisitor::default().into_message(),
        "opentelemetry internal event"
    );
}
