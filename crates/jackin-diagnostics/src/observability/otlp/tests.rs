use super::{
    grpc_endpoint, resolve_endpoint, runtime_creation_count, shutdown, unsupported_protocol,
};

#[test]
fn grpc_endpoint_is_normalized_without_http_signal_paths() {
    assert_eq!(
        grpc_endpoint("http://127.0.0.1:4317///"),
        "http://127.0.0.1:4317"
    );
}

#[test]
fn only_grpc_protocol_is_accepted() {
    assert!(!unsupported_protocol("grpc"));
    assert!(unsupported_protocol("http/protobuf"));
    assert!(unsupported_protocol("http/json"));
}

#[test]
fn empty_endpoint_disables_export() {
    assert_eq!(resolve_endpoint(None), None);
    assert_eq!(resolve_endpoint(Some(String::new())), None);
    assert_eq!(
        resolve_endpoint(Some("http://otel:4317".to_owned())),
        Some("http://otel:4317".to_owned())
    );
}

#[test]
fn disabled_configuration_creates_no_runtime_and_shutdown_is_idempotent() {
    let before = runtime_creation_count();
    let env = |_key: &str| None;
    assert_eq!(super::super::config::resolve_otlp_config(&env), Ok(None));
    shutdown();
    shutdown();
    assert_eq!(runtime_creation_count(), before);
}

#[test]
fn facade_event_exports_native_event_name_once() {
    let (export, subscriber) = super::test_layers(false, "unused");
    tracing::subscriber::with_default(subscriber, || {
        jackin_telemetry::emit_event(
            &jackin_telemetry::event::SESSION_START,
            jackin_telemetry::FieldSet::default(),
        )
        .unwrap();
    });
    export.logger_provider.force_flush().unwrap();
    let logs = export.logs.get_emitted_logs().unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].record.event_name(), Some("session.start"));
}

#[test]
fn governed_unknown_attribute_is_dropped() {
    let before = jackin_telemetry::facade_health().unknown_attribute;
    let (export, subscriber) = super::test_layers(false, "unused");
    tracing::subscriber::with_default(subscriber, || {
        tracing::event!(
            name: "session.start",
            target: jackin_telemetry::TELEMETRY_TARGET,
            tracing::Level::INFO,
            "bogus.secret" = "must-not-export"
        );
    });
    export.logger_provider.force_flush().unwrap();
    assert!(export.logs.get_emitted_logs().unwrap().is_empty());
    assert_eq!(
        jackin_telemetry::facade_health().unknown_attribute,
        before + 1
    );
}
