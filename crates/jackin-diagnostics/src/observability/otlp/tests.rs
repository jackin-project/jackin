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
fn resource_carries_service_identity_only() {
    let resource = build_resource();
    assert_eq!(attr(&resource, keys::SERVICE_NAME), Some("jackin".into()));
    assert_eq!(
        attr(&resource, keys::SERVICE_VERSION),
        Some(env!("CARGO_PKG_VERSION").into())
    );
    // Run/session/component identity must never live on the Resource.
    assert_eq!(keys::RUN_ID, "parallax.run.id");
    assert_eq!(attr(&resource, keys::RUN_ID), None);
    assert_eq!(attr(&resource, keys::SESSION_ID), None);
    assert_eq!(attr(&resource, keys::COMPONENT), None);
}

#[test]
fn two_resources_share_stable_build_identity() {
    let a = build_resource();
    let b = build_resource();
    assert_eq!(attr(&a, keys::SERVICE_NAME), attr(&b, keys::SERVICE_NAME));
    assert_eq!(
        attr(&a, keys::SERVICE_VERSION),
        attr(&b, keys::SERVICE_VERSION)
    );
    assert_eq!(attr(&a, keys::RUN_ID), None);
    assert_eq!(attr(&b, keys::RUN_ID), None);
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
    assert_eq!(log_attr(&log.record, "kind"), None);
    assert_eq!(
        log_attr(&log.record, "event.name").as_deref(),
        Some("compact_kind")
    );
    assert_eq!(log_attr(&log.record, "jackin.detail").as_deref(), Some("d"));
    assert_eq!(
        log_attr(&log.record, "parallax.run.id").as_deref(),
        Some("run1")
    );
    assert_eq!(log_attr(&log.record, "run_id"), None);
    assert_eq!(
        log_attr(&log.record, "jackin.component").as_deref(),
        Some("host")
    );
    assert_eq!(
        log_attr(&log.record, "event.name").as_deref(),
        Some("compact_kind")
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
        Some("compact_kind")
    );
    assert_eq!(
        log_attr(&log.record, "jackin.category").as_deref(),
        Some("compact")
    );
    assert_eq!(
        log_attr(&log.record, "jackin.stage").as_deref(),
        Some("plan")
    );
    assert_eq!(log_attr(&log.record, "stage"), None);
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
        log_attr(&logs[0].record, "jackin.detail").as_deref(),
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
    assert_eq!(
        log_attr(&log.record, "jackin.detail").as_deref(),
        Some("docker")
    );
}

#[test]
fn absent_stage_and_detail_are_not_exported_as_sentinels() {
    let logs = exported_logs!(false, "run1", || {
        crate::observability::emit_jsonl_event("run1", "compact_kind", "hello world", None, None);
    });

    assert_eq!(logs.len(), 1);
    assert_eq!(log_attr(&logs[0].record, "stage"), None);
    assert_eq!(log_attr(&logs[0].record, "jackin.detail"), None);
    assert_eq!(log_attr(&logs[0].record, "diagnostics_message"), None);
    assert_eq!(log_attr(&logs[0].record, "jackin_jsonl"), None);
}

#[test]
fn manual_launch_stage_span_name_stays_constant_without_otel_name() {
    let spans = exported_spans(false, "run1", || {
        let span = tracing::info_span!("launch_stage", "jackin.stage" = "derived image");
        drop(span.enter());
    });

    assert_eq!(spans.len(), 1);
    let span = &spans[0];
    assert_eq!(span.name.as_ref(), "launch_stage");
    assert_eq!(
        span_attr(span, "jackin.stage").as_deref(),
        Some("derived image")
    );
}

#[test]
fn stage_span_duration_covers_stage() {
    let spans = exported_spans(false, "run1", || {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        let run = crate::RunDiagnostics::start(&paths, false, "load").unwrap();
        run.stage(
            "stage_started",
            crate::DiagnosticStage::DerivedImage,
            "building",
            None,
        );
        #[expect(
            clippy::disallowed_methods,
            reason = "test needs wall time between stage start and end to assert exported duration"
        )]
        std::thread::sleep(std::time::Duration::from_millis(50));
        run.stage(
            "stage_done",
            crate::DiagnosticStage::DerivedImage,
            "built",
            None,
        );
    });

    let span = spans
        .iter()
        .find(|span| span_attr(span, "jackin.stage").as_deref() == Some("derived image"))
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
        run.stage(
            "stage_started",
            crate::DiagnosticStage::DerivedImage,
            "building",
            None,
        );
        run.stage(
            "stage_done",
            crate::DiagnosticStage::DerivedImage,
            "built",
            None,
        );
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
        run.stage(
            "stage_started",
            crate::DiagnosticStage::DerivedImage,
            "building",
            None,
        );
        run.stage(
            "stage_failed",
            crate::DiagnosticStage::DerivedImage,
            "build failed",
            None,
        );
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
        run.stage(
            "stage_started",
            crate::DiagnosticStage::DerivedImage,
            "building",
            None,
        );
        run.timing_started(crate::DiagnosticStage::DerivedImage, "docker_build", None);
        run.timing_done(crate::DiagnosticStage::DerivedImage, "docker_build", None);
        run.stage(
            "stage_done",
            crate::DiagnosticStage::DerivedImage,
            "built",
            None,
        );
    });
    let spans = export.spans.get_finished_spans().unwrap();
    let logs = export.logs.get_emitted_logs().unwrap();

    let span = spans
        .iter()
        .find(|span| span.name.as_ref() == "launch.derived_image")
        .expect("stage span exported");
    let timing = logs
        .iter()
        .find(|log| log_attr(&log.record, "event.name").as_deref() == Some("timing.done"))
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
        // Prefix-free capsule body with schema fields (plan 004 bridge shape).
        tracing::event!(
            target: "jackin_capsule",
            tracing::Level::INFO,
            "event.name" = "capsule.log",
            "jackin.category" = "capsule",
            "jackin.component" = "capsule",
            "event.outcome" = "success",
            "session.id" = "sess-test",
            "parallax.run.id" = "run1",
            "capsule line"
        );
    });

    assert_eq!(logs.len(), 2);
    let bodies = logs
        .iter()
        .filter_map(|log| log_body(&log.record))
        .collect::<Vec<_>>();
    assert!(bodies.iter().any(|body| body == "hello"));
    assert!(bodies.iter().any(|body| body == "capsule line"));
    for log in &logs {
        if let Some(body) = log_body(&log.record) {
            assert!(
                !body.starts_with('['),
                "exported body must be prefix-free: {body}"
            );
        }
    }
    let capsule = logs
        .iter()
        .find(|log| log_body(&log.record).as_deref() == Some("capsule line"))
        .expect("capsule log");
    assert_eq!(
        log_attr(&capsule.record, "event.name").as_deref(),
        Some("capsule.log")
    );
    assert_eq!(
        log_attr(&capsule.record, "jackin.component").as_deref(),
        Some("capsule")
    );
    assert_eq!(
        log_attr(&capsule.record, "jackin.category").as_deref(),
        Some("capsule")
    );
    assert_eq!(
        log_attr(&capsule.record, "session.id").as_deref(),
        Some("sess-test")
    );
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
        .find(|log| {
            log_attr(&log.record, "event.name").as_deref() == Some("process.subprocess.done")
        })
        .expect("subprocess_done log exported");
    assert_eq!(
        log_attr(&log.record, "jackin.stage").as_deref(),
        Some("git")
    );
    assert_eq!(log_attr(&log.record, "stage"), None);
    let detail = log_attr(&log.record, "jackin.detail").expect("subprocess detail");
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
fn wire_log_resource_excludes_run_and_component() {
    let logs = exported_logs!(false, "wire-run", || {
        crate::observability::emit_jsonl_event("wire-run", "compact_kind", "hello", None, None);
    });
    assert_eq!(logs.len(), 1);
    let resource = &logs[0].resource;
    assert_eq!(attr(resource, keys::SERVICE_NAME), Some("jackin".into()));
    assert_eq!(
        attr(resource, keys::SERVICE_VERSION),
        Some(env!("CARGO_PKG_VERSION").into())
    );
    assert_eq!(attr(resource, keys::COMPONENT), None);
    assert_eq!(attr(resource, keys::RUN_ID), None);
    assert_eq!(attr(resource, keys::SESSION_ID), None);
    // Identity lives on the record attributes.
    assert_eq!(
        log_attr(&logs[0].record, keys::RUN_ID).as_deref(),
        Some("wire-run")
    );
    assert_eq!(
        log_attr(&logs[0].record, keys::COMPONENT).as_deref(),
        Some("host")
    );
}

#[test]
fn stage_failed_exports_as_error() {
    let logs = exported_logs!(false, "run1", || {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        let run = crate::RunDiagnostics::start(&paths, false, "load").unwrap();
        run.stage(
            "stage_failed",
            crate::DiagnosticStage::DerivedImage,
            "boom",
            None,
        );
    });

    let failed = logs
        .iter()
        .find(|log| log_attr(&log.record, "event.name").as_deref() == Some("launch.stage.failed"))
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
        .find(|log| log_attr(&log.record, "event.name").as_deref() == Some("E014"))
        .expect("typed error log exported");
    assert_eq!(error.record.severity_number(), Some(Severity::Error));
    assert_eq!(
        log_attr(&error.record, "error.type").as_deref(),
        Some("E014")
    );
    assert_eq!(log_attr(&error.record, "error_type"), None);
}

#[test]
fn volatile_failure_evidence_does_not_split_fingerprint() {
    let export = export_after(false, "fingerprint-run", || {
        for body in [
            "container jk-random-a failed under /workspace/one",
            "container jk-random-b failed under /workspace/two",
        ] {
            let span = crate::operation_span("launch.prepare", &[]);
            span.in_scope(|| {
                crate::operation_error("launch.prepare", "agent_binary_download_failed", body, &[]);
            });
        }
    });
    let logs = export.logs.get_emitted_logs().unwrap();
    let fingerprints: Vec<_> = logs
        .iter()
        .filter_map(|log| {
            let event = log_attr(&log.record, "event.name")?;
            let error_type = log_attr(&log.record, "error.type")?;
            Some((event, error_type))
        })
        .collect();
    assert_eq!(
        fingerprints,
        vec![
            (
                "launch.prepare".into(),
                "agent_binary_download_failed".into()
            ),
            (
                "launch.prepare".into(),
                "agent_binary_download_failed".into()
            ),
        ]
    );
    let spans = export.spans.get_finished_spans().unwrap();
    assert_eq!(spans.len(), 2);
    assert!(
        spans
            .iter()
            .all(|span| matches!(span.status, Status::Error { .. }))
    );
}

#[test]
fn direct_diagnostics_events_reach_otlp() {
    let logs = exported_logs!(false, "run1", || {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        let run = crate::RunDiagnostics::start(&paths, false, "load").unwrap();

        run.timing_started(
            crate::DiagnosticStage::Credentials,
            "operator_env",
            Some("layers"),
        );
        run.timing_done(
            crate::DiagnosticStage::Credentials,
            "operator_env",
            Some("2 vars"),
        );
        run.docker_build_step("12", "DONE", Some(76_500), false);
        run.container_started("jk-test", "/capsule.log");
        run.container_exited("jk-test", 137, true, "/capsule.log", Some("crash tail"));
    });

    let by_kind = |kind: &str| {
        let event_name = crate::registry::lookup(kind).map_or(kind, |d| d.name);
        logs.iter()
            .find(|log| {
                let name = log_attr(&log.record, "event.name");
                name.as_deref() == Some(event_name) || name.as_deref() == Some(kind)
            })
            .unwrap_or_else(|| panic!("{kind} / {event_name} log exported"))
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
        log_attr(&crash_log.record, "jackin.detail").as_deref(),
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
        .find(|log| log_attr(&log.record, "event.name").as_deref() == Some("container_crash_log"))
        .expect("container_crash_log exported");
    let detail = log_attr(&crash_log.record, "jackin.detail").expect("capped evidence detail");

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

#[test]
fn jsonl_trace_id_matches_in_memory_exporter() {
    // With the OTLP test subscriber installed, a JSONL event written under an
    // active span must carry the same 32-hex/16-hex ids the in-memory exporter
    // records for that span (the Step 3 correlation contract).
    let (export, subscriber) = test_layers(false, "run-jsonl-corr");
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    let mut jsonl_trace = String::new();
    let mut jsonl_span = String::new();
    tracing::subscriber::with_default(subscriber, || {
        let run = crate::RunDiagnostics::start(&paths, true, "load").unwrap();
        assert!(
            run.persists(),
            "file sink must be on for the JSONL assertion"
        );
        let span = tracing::info_span!("correlation_probe");
        {
            let _entered = span.enter();
            run.compact("breadcrumb", "correlate me");
            run.flush_writer();
        }
        drop(span);
        let contents = std::fs::read_to_string(run.path()).unwrap();
        let line = contents
            .lines()
            .find(|line| line.contains("correlate me"))
            .expect("breadcrumb JSONL line");
        let event: serde_json::Value = serde_json::from_str(line).unwrap();
        jsonl_trace = event["trace_id"].as_str().unwrap().to_owned();
        jsonl_span = event["span_id"].as_str().unwrap().to_owned();
    });
    export.tracer_provider.force_flush().unwrap();
    let spans = export.spans.get_finished_spans().unwrap();
    let span = spans
        .iter()
        .find(|span| span.name.as_ref() == "correlation_probe")
        .expect("correlation_probe span exported");
    assert_eq!(
        jsonl_trace,
        span.span_context.trace_id().to_string(),
        "JSONL trace_id must match the exporter's OTel hex trace id"
    );
    assert_eq!(
        jsonl_span,
        span.span_context.span_id().to_string(),
        "JSONL span_id must match the exporter's OTel hex span id"
    );
    assert_eq!(jsonl_trace.len(), 32);
    assert_eq!(jsonl_span.len(), 16);
    assert!(
        jsonl_trace.chars().all(|c| c.is_ascii_hexdigit()),
        "trace_id must be hex: {jsonl_trace}"
    );
}

#[test]
fn bridge_populates_top_level_event_name_from_attribute() {
    let logs = exported_logs!(false, "run1", || {
        crate::observability::emit_jsonl_event(
            "run1",
            "session_detach",
            "operator detached",
            None,
            None,
        );
    });
    let log = logs
        .iter()
        .find(|log| {
            log_attr(&log.record, "event.name").as_deref() == Some("capsule.session.detach")
        })
        .expect("session detach log");
    assert_eq!(
        log.record.event_name(),
        Some("capsule.session.detach"),
        "top-level EventName must equal the registered dotted name"
    );
    assert_eq!(
        log_attr(&log.record, "event.name").as_deref(),
        log.record.event_name(),
        "attribute mirror must equal top-level EventName"
    );
}

#[test]
fn top_level_event_name_matches_attribute_and_registry() {
    let logs = exported_logs!(false, "run1", || {
        crate::observability::emit_jsonl_event(
            "run1",
            "stage_started",
            "building",
            Some("image"),
            None,
        );
        crate::observability::emit_jsonl_event("run1", "stage_done", "built", Some("image"), None);
        crate::observability::emit_jsonl_event(
            "run1",
            "session_detach",
            "operator detached",
            None,
            None,
        );
        crate::observability::emit_jsonl_event(
            "run1",
            "process.execute",
            "host process execute",
            None,
            None,
        );
    });
    let mut checked = 0usize;
    for log in &logs {
        let Some(attr) = log_attr(&log.record, "event.name") else {
            continue;
        };
        let top = log
            .record
            .event_name()
            .expect("top-level EventName must be populated when event.name attr is present");
        assert!(!top.is_empty(), "EventName must be non-empty");
        assert_eq!(
            top,
            attr.as_str(),
            "EventName must equal event.name attribute"
        );
        if let Some(def) = crate::registry::lookup(top) {
            assert_eq!(def.name, top);
            checked += 1;
        }
    }
    assert!(
        checked >= 3,
        "expected at least three registry-validated EventName values, got {checked}"
    );
}
