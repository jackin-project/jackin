use jackin_core::JackinPaths;
use opentelemetry::Key;
use opentelemetry::logs::{AnyValue, Severity};
use opentelemetry::trace::Status;
use opentelemetry_sdk::logs::SdkLogRecord;
use opentelemetry_sdk::trace::SpanData;

use super::keys;
use super::{
    TestExport, build_resource, export_filter_directive_with_internal, grpc_endpoint,
    parse_traceparent, resolve_endpoint, resolve_endpoints, test_layers, unsupported_protocol,
};

fn attr(resource: &opentelemetry_sdk::Resource, key: &'static str) -> Option<String> {
    resource
        .get(&Key::from_static_str(key))
        .map(|value| value.to_string())
}

fn log_attr(record: &SdkLogRecord, key: &str) -> Option<String> {
    record
        .attributes_iter()
        .find(|(name, _)| name.as_str() == key)
        .map(|(_, value)| any_value_to_string(value))
}

fn span_attr(span: &SpanData, key: &str) -> Option<String> {
    span.attributes
        .iter()
        .find(|kv| kv.key.as_str() == key)
        .map(|kv| kv.value.to_string())
}

fn any_value_to_string(value: &AnyValue) -> String {
    match value {
        AnyValue::String(value) => value.to_string(),
        AnyValue::Boolean(value) => value.to_string(),
        AnyValue::Int(value) => value.to_string(),
        AnyValue::Double(value) => value.to_string(),
        other => format!("{other:?}"),
    }
}

fn log_body(record: &SdkLogRecord) -> Option<String> {
    record.body().map(any_value_to_string)
}

fn export_after(debug: bool, run_id: &str, emit: impl FnOnce()) -> TestExport {
    let (export, subscriber) = test_layers(debug, run_id);
    tracing::subscriber::with_default(subscriber, emit);
    export.logger_provider.force_flush().unwrap();
    export.tracer_provider.force_flush().unwrap();
    export
}

fn exported_spans(debug: bool, run_id: &str, emit: impl FnOnce()) -> Vec<SpanData> {
    let export = export_after(debug, run_id, emit);
    export.spans.get_finished_spans().unwrap()
}

macro_rules! exported_logs {
    ($debug:expr, $run_id:expr, $emit:expr) => {{
        let export = export_after($debug, $run_id, $emit);
        export.logs.get_emitted_logs().unwrap()
    }};
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

#[test]
fn exported_log_carries_body_and_attributes() {
    let logs = exported_logs!(false, "run1", || {
        crate::observability::emit_jsonl_event(
            "run1",
            "compact_kind",
            "hello world",
            Some("plan"),
            Some("d"),
        );
    });

    assert_eq!(logs.len(), 1);
    let log = &logs[0];
    assert_eq!(log.record.severity_number(), Some(Severity::Info));
    assert_eq!(log_body(&log.record).as_deref(), Some("hello world"));
    assert_eq!(
        log_attr(&log.record, "kind").as_deref(),
        Some("compact_kind")
    );
    assert_eq!(log_attr(&log.record, "stage").as_deref(), Some("plan"));
    assert_eq!(log_attr(&log.record, "detail").as_deref(), Some("d"));
    assert_eq!(log_attr(&log.record, "run_id").as_deref(), Some("run1"));
    assert_eq!(
        log_attr(&log.record, "event.name").as_deref(),
        Some("compact.kind")
    );
    assert_eq!(
        log_attr(&log.record, "event.outcome").as_deref(),
        Some("success")
    );
    assert_eq!(
        log_attr(&log.record, "jackin.component").as_deref(),
        Some("host")
    );
    assert_eq!(
        log_attr(&log.record, "jackin.operation").as_deref(),
        Some("compact.kind")
    );
    assert_eq!(
        log_attr(&log.record, "jackin.category").as_deref(),
        Some("compact")
    );
    assert_eq!(log_attr(&log.record, "diagnostics_message"), None);
    assert_eq!(log_attr(&log.record, "jackin_jsonl"), None);
}

#[test]
fn exported_log_body_and_detail_are_redacted() {
    let logs = exported_logs!(false, "run1", || {
        crate::observability::emit_jsonl_event(
            "run1",
            "compact_kind",
            "token=ghp_abcdefghijklmnopqrstuvwxyz0123456789",
            Some("plan"),
            Some("authorization: Bearer abcdefghijklmnopqrstuvwxyz0123456789"),
        );
    });

    assert_eq!(logs.len(), 1);
    assert_eq!(log_body(&logs[0].record).as_deref(), Some("<redacted>"));
    assert_eq!(
        log_attr(&logs[0].record, "detail").as_deref(),
        Some("<redacted>")
    );
}

#[test]
fn exported_error_log_is_error_severity() {
    let logs = exported_logs!(false, "run1", || {
        crate::observability::emit_jsonl_error("run1", "failure", "boom", None, None);
    });

    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].record.severity_number(), Some(Severity::Error));
}

#[test]
fn debug_kind_is_debug_severity_and_filtered_at_info() {
    let info_logs = exported_logs!(false, "run1", || {
        crate::observability::emit_jsonl_event("run1", "debug", "debug line", None, Some("docker"));
    });
    assert!(info_logs.is_empty());

    let debug_logs = exported_logs!(true, "run1", || {
        crate::observability::emit_jsonl_event("run1", "debug", "debug line", None, Some("docker"));
    });
    assert_eq!(debug_logs.len(), 1);
    let log = &debug_logs[0];
    assert_eq!(log.record.severity_number(), Some(Severity::Debug));
    assert_eq!(log_attr(&log.record, "stage"), None);
    assert_eq!(log_attr(&log.record, "detail").as_deref(), Some("docker"));
}

#[test]
fn absent_stage_and_detail_are_not_exported_as_sentinels() {
    let logs = exported_logs!(false, "run1", || {
        crate::observability::emit_jsonl_event("run1", "compact_kind", "hello world", None, None);
    });

    assert_eq!(logs.len(), 1);
    assert_eq!(log_attr(&logs[0].record, "stage"), None);
    assert_eq!(log_attr(&logs[0].record, "detail"), None);
    assert_eq!(log_attr(&logs[0].record, "diagnostics_message"), None);
    assert_eq!(log_attr(&logs[0].record, "jackin_jsonl"), None);
}

#[test]
fn manual_launch_stage_span_name_stays_constant_without_otel_name() {
    let spans = exported_spans(false, "run1", || {
        let span = tracing::info_span!("launch_stage", stage = "derived image");
        drop(span.enter());
    });

    assert_eq!(spans.len(), 1);
    let span = &spans[0];
    assert_eq!(span.name.as_ref(), "launch_stage");
    assert_eq!(span_attr(span, "stage").as_deref(), Some("derived image"));
}

#[test]
fn stage_span_duration_covers_stage() {
    let spans = exported_spans(false, "run1", || {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        let run = crate::RunDiagnostics::start(&paths, false, "load").unwrap();
        run.stage("stage_started", "derived image", "building", None);
        #[expect(
            clippy::disallowed_methods,
            reason = "test needs wall time between stage start and end to assert exported duration"
        )]
        std::thread::sleep(std::time::Duration::from_millis(50));
        run.stage("stage_done", "derived image", "built", None);
    });

    let span = spans
        .iter()
        .find(|span| span_attr(span, "stage").as_deref() == Some("derived image"))
        .expect("derived image stage span exported");
    let duration = span.end_time.duration_since(span.start_time).unwrap();
    assert!(
        duration >= std::time::Duration::from_millis(50),
        "stage span duration should cover stage work, got {duration:?}"
    );
}

#[test]
fn stage_span_exported_name_is_stage_specific() {
    let spans = exported_spans(false, "run1", || {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        let run = crate::RunDiagnostics::start(&paths, false, "load").unwrap();
        run.stage("stage_started", "derived image", "building", None);
        run.stage("stage_done", "derived image", "built", None);
    });

    assert!(
        spans
            .iter()
            .any(|span| span.name.as_ref() == "launch.derived_image"),
        "stage span should export under dynamic otel.name: {spans:#?}"
    );
}

#[test]
fn failed_stage_span_has_error_status() {
    let spans = exported_spans(false, "run1", || {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        let run = crate::RunDiagnostics::start(&paths, false, "load").unwrap();
        run.stage("stage_started", "derived image", "building", None);
        run.stage("stage_failed", "derived image", "build failed", None);
    });

    let span = spans
        .iter()
        .find(|span| span.name.as_ref() == "launch.derived_image")
        .expect("failed stage span exported");
    assert_eq!(
        span.status,
        Status::Error {
            description: "build failed".into()
        }
    );
}

#[test]
fn timing_event_inherits_stage_span_context() {
    let export = export_after(false, "run1", || {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        let run = crate::RunDiagnostics::start(&paths, false, "load").unwrap();
        run.stage("stage_started", "derived image", "building", None);
        run.timing_started("derived image", "docker_build", None);
        run.timing_done("derived image", "docker_build", None);
        run.stage("stage_done", "derived image", "built", None);
    });
    let spans = export.spans.get_finished_spans().unwrap();
    let logs = export.logs.get_emitted_logs().unwrap();

    let span = spans
        .iter()
        .find(|span| span.name.as_ref() == "launch.derived_image")
        .expect("stage span exported");
    let timing = logs
        .iter()
        .find(|log| log_attr(&log.record, "kind").as_deref() == Some("timing_done"))
        .expect("timing_done log exported");
    let context = timing
        .record
        .trace_context()
        .expect("timing log should carry trace context");

    assert_eq!(context.span_id, span.span_context.span_id());
    assert_eq!(context.trace_id, span.span_context.trace_id());
}

#[test]
fn dependency_targets_are_filtered_out() {
    let logs = exported_logs!(false, "run1", || {
        tracing::info!(target: "turso_core", "vm step");
        tracing::info!(target: "some_random_crate", "random step");
    });

    assert!(logs.is_empty());
}

#[test]
fn jackin_targets_still_export() {
    let logs = exported_logs!(false, "run1", || {
        crate::observability::emit_jsonl_event("run1", "compact_kind", "hello", None, None);
        tracing::info!(target: "jackin_capsule", "capsule line");
    });

    assert_eq!(logs.len(), 2);
    let bodies = logs
        .iter()
        .filter_map(|log| log_body(&log.record))
        .collect::<Vec<_>>();
    assert!(bodies.iter().any(|body| body == "hello"));
    assert!(bodies.iter().any(|body| body == "capsule line"));
}

#[test]
fn spans_from_workspace_crates_still_export() {
    let spans = exported_spans(false, "run1", || {
        let span = tracing::info_span!("launch_stage", stage = "derived image");
        drop(span.enter());
    });

    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].name.as_ref(), "launch_stage");
}

#[test]
fn subprocess_done_carries_duration_and_exit() {
    let logs = exported_logs!(false, "run1", || {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        let run = crate::RunDiagnostics::start(&paths, false, "load").unwrap();
        run.subprocess_done("git", 42, Some(0));
    });

    let log = logs
        .iter()
        .find(|log| log_attr(&log.record, "kind").as_deref() == Some("subprocess_done"))
        .expect("subprocess_done log exported");
    assert_eq!(log_attr(&log.record, "stage").as_deref(), Some("git"));
    let detail = log_attr(&log.record, "detail").expect("subprocess detail");
    assert!(detail.contains("\"program\":\"git\""), "{detail}");
    assert!(detail.contains("\"elapsed_ms\":42"), "{detail}");
    assert!(detail.contains("\"exit_code\":0"), "{detail}");
}

#[test]
fn export_filter_directive_is_jackin_allowlist() {
    let directive = export_filter_directive_with_internal("info", false);

    assert!(directive.starts_with("off,jackin=info"));
    assert!(directive.contains(",jackin_capsule=info"));
    assert!(directive.contains(",jackin_diagnostics::jsonl=info"));
    assert!(!directive.split(',').any(|part| part == "info"));
    assert!(!directive.contains("hyper=off"));
}

#[test]
fn export_filter_directive_internal_flag_restores_global_level() {
    let directive = export_filter_directive_with_internal("debug", true);

    assert!(directive.starts_with("off,jackin=debug"));
    assert!(directive.split(',').any(|part| part == "debug"));
    assert!(directive.contains("hyper=off"));
    assert!(directive.contains("opentelemetry_sdk=off"));
}

#[test]
fn wire_log_resource_carries_run_id_service_and_component() {
    let logs = exported_logs!(false, "wire-run", || {
        crate::observability::emit_jsonl_event("wire-run", "compact_kind", "hello", None, None);
    });
    assert_eq!(logs.len(), 1);
    let resource = &logs[0].resource;
    assert_eq!(attr(resource, keys::SERVICE_NAME), Some("jackin".into()));
    assert_eq!(attr(resource, keys::COMPONENT), Some("host".into()));
    assert_eq!(attr(resource, keys::RUN_ID), Some("wire-run".into()));
}

#[test]
fn stage_failed_exports_as_error() {
    let logs = exported_logs!(false, "run1", || {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        let run = crate::RunDiagnostics::start(&paths, false, "load").unwrap();
        run.stage("stage_failed", "derived image", "boom", None);
    });

    let failed = logs
        .iter()
        .find(|log| log_attr(&log.record, "kind").as_deref() == Some("stage_failed"))
        .expect("stage_failed log exported");
    assert_eq!(failed.record.severity_number(), Some(Severity::Error));
}

#[test]
fn fatal_error_carries_error_type() {
    let logs = exported_logs!(false, "run1", || {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        let run = crate::RunDiagnostics::start(&paths, false, "load").unwrap();
        run.error_typed("E014", "capsule download failed", Some("E014"));
    });

    let error = logs
        .iter()
        .find(|log| log_attr(&log.record, "kind").as_deref() == Some("E014"))
        .expect("typed error log exported");
    assert_eq!(error.record.severity_number(), Some(Severity::Error));
    assert_eq!(
        log_attr(&error.record, "error_type").as_deref(),
        Some("E014")
    );
}

#[test]
fn direct_diagnostics_events_reach_otlp() {
    let logs = exported_logs!(false, "run1", || {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        let run = crate::RunDiagnostics::start(&paths, false, "load").unwrap();

        run.timing_started("credentials", "operator_env", Some("layers"));
        run.timing_done("credentials", "operator_env", Some("2 vars"));
        run.docker_build_step("12", "DONE", Some(76_500), false);
        run.container_started("jk-test", "/capsule.log");
        run.container_exited("jk-test", 137, true, "/capsule.log", Some("crash tail"));
    });

    let by_kind = |kind: &str| {
        logs.iter()
            .find(|log| log_attr(&log.record, "kind").as_deref() == Some(kind))
            .unwrap_or_else(|| panic!("{kind} log exported"))
    };

    assert_eq!(
        by_kind("timing_done").record.severity_number(),
        Some(Severity::Info)
    );
    assert_eq!(
        by_kind("docker_build_step").record.severity_number(),
        Some(Severity::Info)
    );
    assert_eq!(
        by_kind("container_started").record.severity_number(),
        Some(Severity::Info)
    );
    assert_eq!(
        by_kind("container_crash").record.severity_number(),
        Some(Severity::Error)
    );
    let crash_log = by_kind("container_crash_log");
    assert_eq!(crash_log.record.severity_number(), Some(Severity::Error));
    assert_eq!(
        log_attr(&crash_log.record, "detail").as_deref(),
        Some("crash tail")
    );
}

#[test]
fn crash_evidence_is_redacted_and_capped() {
    let logs = exported_logs!(false, "run1", || {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        let run = crate::RunDiagnostics::start(&paths, false, "load").unwrap();
        let evidence = format!(
            "token=ghp_abcdefghijklmnopqrstuvwxyz0123456789 prefix-{}{}",
            "x".repeat(10 * 1024),
            "tail"
        );

        run.container_exited("jk-test", 1, false, "/capsule.log", Some(&evidence));
    });

    let crash_log = logs
        .iter()
        .find(|log| log_attr(&log.record, "kind").as_deref() == Some("container_crash_log"))
        .expect("container_crash_log exported");
    let detail = log_attr(&crash_log.record, "detail").expect("capped evidence detail");

    assert!(detail.starts_with("(truncated to last 4096 bytes)\n"));
    assert!(!detail.contains("ghp_"));
    assert!(detail.ends_with("tail"));
    assert!(detail.len() <= "(truncated to last 4096 bytes)\n".len() + 4096);
}

#[test]
fn format_parse_traceparent_roundtrip() {
    let trace_id = "0123456789abcdef0123456789abcdef";
    let span_id = "0123456789abcdef";
    let header = format!("00-{trace_id}-{span_id}-01");

    let context = parse_traceparent(&header).unwrap();
    assert_eq!(context.trace_id().to_string(), trace_id);
    assert_eq!(context.span_id().to_string(), span_id);
    assert!(context.trace_flags().is_sampled());

    assert!(parse_traceparent(&format!("01-{trace_id}-{span_id}-01")).is_none());
    assert!(parse_traceparent(&format!("00-{trace_id}-{span_id}")).is_none());
    assert!(parse_traceparent(&format!("00-{trace_id}-{span_id}-01-extra")).is_none());
    assert!(parse_traceparent(&format!("00-not-hex-{span_id}-01")).is_none());
}
