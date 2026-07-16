// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Governed OpenTelemetry conformance tests.

use std::fs;

use jackin_core::JackinPaths;

use crate::DIAGNOSTICS_TEST_LOCK;
use crate::run::RunDiagnostics;

// ── OpenTelemetry conformance tests ─────────────────────────────────────────

const MAX_DEBUG_LOGS: usize = 64;
const MAX_SPANS: usize = 48;
const CONFORMANCE_ARGV_CANARY: &str = "--password=conformance-argv-secret";
const CONFORMANCE_URL_CANARY: &str = "https://example.invalid/api?token=conformance-query-secret";
const CONFORMANCE_INSPECT_CANARY: &str =
    r#"{"Config":{"Env":["TOKEN=conformance-inspect-secret"]}}"#;
const CONFORMANCE_TERMINAL_CANARY: &str = "\u{1b}[31mconformance-terminal-bytes\u{1b}[0m";

/// Combined host + capsule export from the dual-bootstrap conformance scenario.
struct ConformanceExport {
    host: crate::observability::TestExport,
    capsule: crate::observability::TestExport,
}

impl ConformanceExport {
    fn all_logs(&self) -> Vec<opentelemetry_sdk::logs::in_memory_exporter::LogDataWithResource> {
        let mut logs = self.host.logs.get_emitted_logs().unwrap_or_default();
        logs.extend(self.capsule.logs.get_emitted_logs().unwrap_or_default());
        logs
    }

    fn all_spans(&self) -> Vec<opentelemetry_sdk::trace::SpanData> {
        let mut spans = self.host.spans.get_finished_spans().unwrap_or_default();
        spans.extend(self.capsule.spans.get_finished_spans().unwrap_or_default());
        spans
    }
}

fn collect_files(root: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, files);
        } else {
            files.push(path);
        }
    }
}

#[test]
fn conformance_no_local_artifacts() -> anyhow::Result<()> {
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path().join("home");
    let data = temp.path().join("data");
    let config = temp.path().join("config");
    let xdg_data = temp.path().join("xdg-data");
    let xdg_config = temp.path().join("xdg-config");
    let work = temp.path().join("work");
    for directory in [&home, &data, &config, &xdg_data, &xdg_config, &work] {
        fs::create_dir_all(directory)?;
    }

    let retained_usage = data.join("state/usage/snapshots.db");
    fs::create_dir_all(retained_usage.parent().expect("usage store parent"))
        .expect("usage store directory");
    fs::write(&retained_usage, b"retained application state").expect("usage state fixture");
    let ratchet = temp.path().join("target/telemetry-volume.json");
    fs::create_dir_all(ratchet.parent().expect("ratchet parent")).expect("ratchet directory");
    fs::write(&ratchet, b"{}").expect("ratchet fixture");
    let fixture_capture = temp.path().join("fixtures/pty-capture.bin");
    fs::create_dir_all(fixture_capture.parent().expect("capture parent"))?;
    fs::write(&fixture_capture, b"explicit test fixture capture")?;

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let testbed = runtime.block_on(async { jackin_otlp_testbed::Testbed::start() })?;
    for identity in ["host", "daemon", "capsule"] {
        let output = std::process::Command::new(std::env::current_exe()?)
            .args(["--exact", "tests::artifact_lifecycle_child", "--nocapture"])
            .env_clear()
            .env("HOME", &home)
            .env("JACKIN_HOME_DIR", &data)
            .env("JACKIN_CONFIG_DIR", &config)
            .env("XDG_DATA_HOME", &xdg_data)
            .env("XDG_CONFIG_HOME", &xdg_config)
            .env("JACKIN_PTY_FIXTURE_CAPTURE", &fixture_capture)
            .env("OTEL_EXPORTER_OTLP_ENDPOINT", testbed.endpoint())
            .env("OTEL_EXPORTER_OTLP_PROTOCOL", "grpc")
            .env("JACKIN_ARTIFACT_LIFECYCLE", identity)
            .current_dir(&work)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?
            .wait_with_output()?;
        assert!(
            output.status.success(),
            "{identity} lifecycle failed:\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    assert!(
        runtime.block_on(testbed.wait_for_all_signals(std::time::Duration::from_secs(2))),
        "production lifecycles did not export all signals"
    );
    let traces = testbed.traces();
    let service_names = traces
        .iter()
        .flat_map(|request| &request.resource_spans)
        .filter_map(|batch| batch.resource.as_ref())
        .flat_map(|resource| &resource.attributes)
        .filter(|attribute| attribute.key == "service.name")
        .filter_map(|attribute| attribute.value.as_ref())
        .filter_map(|value| value.value.as_ref())
        .filter_map(|value| match value {
            opentelemetry_proto::tonic::common::v1::any_value::Value::StringValue(name) => {
                Some(name.as_str())
            }
            _ => None,
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        service_names,
        std::collections::BTreeSet::from(["jackin", "jackin-capsule", "jackin-daemon"]),
        "artifact test did not exercise every production identity"
    );

    let mut files = Vec::new();
    collect_files(temp.path(), &mut files);
    let unexpected = files
        .into_iter()
        .filter(|path| path != &retained_usage && path != &ratchet && path != &fixture_capture)
        .collect::<Vec<_>>();
    assert!(
        unexpected.is_empty(),
        "telemetry lifecycle created local artifacts: {unexpected:?}"
    );
    Ok(())
}

#[test]
fn artifact_lifecycle_child() -> anyhow::Result<()> {
    let Ok(identity) = std::env::var("JACKIN_ARTIFACT_LIFECYCLE") else {
        return Ok(());
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let runtime_guard = runtime.enter();
    let paths = JackinPaths::detect()?;
    paths.ensure_base_dirs()?;

    let active_run = match identity.as_str() {
        "host" => Some(RunDiagnostics::start(
            &paths,
            false,
            "diagnostics",
            crate::ServiceIdentity::HOST_ONE_SHOT,
        )?),
        "daemon" => Some(RunDiagnostics::start(
            &paths,
            false,
            "daemon",
            crate::ServiceIdentity::DAEMON,
        )?),
        "capsule" => {
            assert!(crate::init_capsule_tracing(None)?);
            None
        }
        other => anyhow::bail!("unknown artifact lifecycle identity {other}"),
    };
    assert_eq!(crate::telemetry_health_snapshot().active_signals, 3);
    let active_guard = active_run.as_ref().map(RunDiagnostics::activate);

    let operation =
        jackin_telemetry::root_operation(&jackin_telemetry::operation::TELEMETRY_VALIDATE, &[])
            .map_err(|error| anyhow::anyhow!("artifact operation rejected: {error:?}"))?;
    let span_guard = operation.span().enter();
    jackin_telemetry::emit_event(
        &jackin_telemetry::event::TELEMETRY_VALIDATE,
        jackin_telemetry::FieldSet::default(),
    )
    .map_err(|error| anyhow::anyhow!("artifact event rejected: {error:?}"))?;
    jackin_telemetry::counter(&jackin_telemetry::metric::TELEMETRY_VALIDATE)
        .add(1, &[])
        .map_err(|error| anyhow::anyhow!("artifact metric rejected: {error:?}"))?;
    drop(span_guard);
    operation.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);

    drop(active_guard);
    if identity == "capsule" {
        crate::shutdown_capsule_tracing();
    }
    assert_eq!(crate::telemetry_health_snapshot().active_signals, 0);
    drop(runtime_guard);
    Ok(())
}

/// Workspace `target/telemetry-volume.json` (plan 009 measured export-volume).
fn telemetry_volume_artifact_path() -> std::path::PathBuf {
    if let Some(path) = std::env::var_os("JACKIN_TELEMETRY_VOLUME_PATH") {
        return path.into();
    }
    // nextest CWD is the package dir; always write to the workspace target so
    // `cargo xtask lint ratchet` (repo root) consumes the same file.
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/telemetry-volume.json")
}

/// Dual-bootstrap host→capsule conformance scenario (plan 009).
///
/// Host phase: host `test_layers` + run/stage/process facades.
/// Capsule phase: separate `test_capsule_layers` bootstrap (no host-only layers)
/// driving production [`emit_session_start_for_test`] plus capsule-target
/// breadcrumbs — not synthetic events on the host subscriber.
fn drive_standard_conformance_scenario() -> ConformanceExport {
    use crate::operation::{OperationLevel, telemetry_error_line, telemetry_line};

    const RUN_ID: &str = "conformance-run";
    const SESSION_ID: &str = "conformance-session";

    assert!(
        crate::metrics::ensure_hot_path_test_rig(),
        "conformance scenario must own the in-memory metric exporter"
    );

    // ── Host bootstrap ──────────────────────────────────────────────────
    let (host, host_sub) = crate::observability::test_layers(false, RUN_ID);
    tracing::subscriber::with_default(host_sub, || {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = JackinPaths::for_tests(tmp.path());
        let invocation = tracing::info_span!(
            target: jackin_telemetry::TELEMETRY_TARGET,
            parent: None,
            "cli.command"
        );
        let _invocation_entered = invocation.enter();
        let run = RunDiagnostics::start(
            &paths,
            true,
            "conformance",
            crate::ServiceIdentity::HOST_ONE_SHOT,
        )
        .expect("run start");
        let _guard = run.activate();

        telemetry_line(OperationLevel::Info, "screen", "list entered");
        telemetry_line(OperationLevel::Warn, "docker", "process retry exhausted");
        let launch_attrs = [jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::LAUNCH_TARGET_KIND,
            value: jackin_telemetry::Value::Str("workspace"),
        }];
        let launch =
            jackin_telemetry::operation(&jackin_telemetry::operation::LAUNCH, &launch_attrs)
                .expect("registered launch operation");
        let launch_scope = launch.span().enter();
        let stage_attrs = [jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::LAUNCH_STAGE_NAME,
            value: jackin_telemetry::Value::Str("derived_image"),
        }];
        jackin_telemetry::operation(&jackin_telemetry::operation::LAUNCH_STAGE, &stage_attrs)
            .expect("registered launch stage")
            .complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);

        let operation =
            jackin_telemetry::operation(&jackin_telemetry::operation::PROCESS_COMMAND, &[])
                .expect("registered process operation");
        let guard = operation.span().enter();
        telemetry_line(OperationLevel::Info, "docker", "process executed");
        // Representative host failure; the actual attach failure seam is
        // asserted in jackin-capsule's conformance test.
        telemetry_error_line(
            jackin_telemetry::schema::enums::ErrorType::RpcError,
            "forced attach failure for conformance",
        );
        drop(guard);
        operation.complete(
            jackin_telemetry::schema::enums::OutcomeValue::Failure,
            Some(jackin_telemetry::schema::enums::ErrorType::RpcError),
        );
        drop(launch_scope);
        launch.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);

        for _ in 0..100 {
            crate::metrics::record_frame(32, 1, 4);
            crate::metrics::record_render(50, 4);
        }

        telemetry_line(
            OperationLevel::Info,
            "security",
            &format!(
                "argv={CONFORMANCE_ARGV_CANARY} url={CONFORMANCE_URL_CANARY} inspect={CONFORMANCE_INSPECT_CANARY}"
            ),
        );
    });
    drop(host.logger_provider.force_flush());
    drop(host.tracer_provider.force_flush());

    // ── Capsule bootstrap (separate provider, production session-start) ─
    let (capsule, capsule_sub) = crate::observability::test_capsule_layers(false);
    tracing::subscriber::with_default(capsule_sub, || {
        // Same code path as init_capsule → emit_session_start after attach.
        crate::observability::emit_session_start_for_test(SESSION_ID, Some(RUN_ID), None);

        let attach = tracing::info_span!(
            target: jackin_telemetry::TELEMETRY_TARGET,
            "rpc.server"
        );
        let _attach_guard = attach.enter();

        let session_attr = jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::SESSION_ID,
            value: jackin_telemetry::Value::Str(SESSION_ID),
        };
        let detach_attrs = [
            session_attr,
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::OUTCOME,
                value: jackin_telemetry::Value::Str("cancellation"),
            },
        ];
        jackin_telemetry::emit_event(
            &jackin_telemetry::event::OPERATION_LOG,
            jackin_telemetry::FieldSet::new(
                &[
                    session_attr,
                    jackin_telemetry::Attr {
                        key: jackin_telemetry::schema::attrs::OUTCOME,
                        value: jackin_telemetry::Value::Str("success"),
                    },
                ],
                Some("capsule breadcrumb"),
            ),
        )
        .unwrap();

        // Expected detach (not a failure): registry-validated session.detach.
        jackin_telemetry::emit_event(
            &jackin_telemetry::event::CAPSULE_SESSION_DETACH,
            jackin_telemetry::FieldSet::new(&detach_attrs, Some("operator detached")),
        )
        .unwrap();
    });
    drop(capsule.logger_provider.force_flush());
    drop(capsule.tracer_provider.force_flush());

    ConformanceExport { host, capsule }
}

fn conformance_log_body(record: &opentelemetry_sdk::logs::SdkLogRecord) -> Option<String> {
    use opentelemetry::logs::AnyValue;

    record.body().map(|value| match value {
        AnyValue::String(value) => value.to_string(),
        other => format!("{other:?}"),
    })
}

fn conformance_log_attr(
    record: &opentelemetry_sdk::logs::SdkLogRecord,
    key: &str,
) -> Option<String> {
    use opentelemetry::logs::AnyValue;

    record
        .attributes_iter()
        .find(|(name, _)| name.as_str() == key)
        .map(|(_, value)| match value {
            AnyValue::String(value) => value.to_string(),
            other => format!("{other:?}"),
        })
}

#[test]
fn conformance_exported_bodies_have_no_bracket_prefix() {
    let _lock = DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let export = drive_standard_conformance_scenario();
    for log in export.all_logs() {
        if let Some(body) = conformance_log_body(&log.record) {
            assert!(
                !body.contains("[jackin debug") && !body.contains("[jackin-capsule"),
                "exported body must be prefix-free: {body}"
            );
        }
    }
}

#[test]
fn conformance_records_have_complete_otlp_shape() {
    use opentelemetry::logs::Severity;

    let _lock = DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let export = drive_standard_conformance_scenario();
    let logs = export.all_logs();
    let mut observed = std::collections::BTreeSet::new();
    for log in logs.iter().filter(|log| {
        matches!(
            log.record.event_name(),
            Some("operation.log" | "operation.warn" | "error.typed" | "capsule.session.detach")
        )
    }) {
        let event_name = log.record.event_name().expect("top-level EventName");
        assert_eq!(conformance_log_attr(&log.record, "event.name"), None);
        assert!(
            log.record.timestamp().is_some() || log.record.observed_timestamp().is_some(),
            "{event_name} must carry a timestamp"
        );
        assert!(log.record.severity_number().is_some());
        assert!(log.record.severity_text().is_some());
        assert!(conformance_log_body(&log.record).is_some());
        let trace = log
            .record
            .trace_context()
            .unwrap_or_else(|| panic!("{event_name} missing active trace context"));
        assert_ne!(trace.trace_id, opentelemetry::TraceId::INVALID);
        assert_ne!(trace.span_id, opentelemetry::SpanId::INVALID);
        assert!(trace.trace_flags.is_some());
        observed.insert(log.record.severity_number().unwrap());
    }
    assert!(observed.contains(&Severity::Info));
    assert!(observed.contains(&Severity::Warn));
    assert!(observed.contains(&Severity::Error));
}

#[test]
fn conformance_export_invokes_sensitive_boundary_canary_gate() {
    let _lock = DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let export = drive_standard_conformance_scenario();
    let logs = export.all_logs();
    let dump = format!("{logs:?}");
    for canary in [
        CONFORMANCE_ARGV_CANARY,
        CONFORMANCE_URL_CANARY,
        CONFORMANCE_INSPECT_CANARY,
        CONFORMANCE_TERMINAL_CANARY,
    ] {
        assert!(
            !dump.contains(canary),
            "sensitive-boundary canary leaked into export: {canary:?}"
        );
    }
    assert_eq!(
        crate::redact::redact_text("token=conformance-direct-canary"),
        "<redacted>",
        "matrix must invoke the production redaction helper"
    );
}

#[test]
fn conformance_forced_failure_is_typed_and_detach_is_not_failure() {
    use opentelemetry::logs::Severity;

    let _lock = DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let export = drive_standard_conformance_scenario();
    let logs = export.all_logs();
    let errors: Vec<_> = logs
        .iter()
        .filter(|log| log.record.severity_number() == Some(Severity::Error))
        .collect();
    assert!(!errors.is_empty(), "expected an ERROR log");
    assert!(errors.iter().any(|log| {
        conformance_log_attr(&log.record, "error_type").as_deref() == Some("rpc_error")
            || conformance_log_attr(&log.record, "error.type").as_deref() == Some("rpc_error")
    }));
    // Detach is emitted on the capsule bootstrap, not the host facade.
    assert!(
        logs.iter().any(|log| {
            log.record.event_name() == Some("capsule.session.detach")
                && conformance_log_attr(&log.record, "outcome").as_deref() == Some("cancellation")
        }),
        "expected cancellation detach must come from capsule bootstrap"
    );
    assert!(
        export
            .capsule
            .logs
            .get_emitted_logs()
            .unwrap()
            .iter()
            .any(|log| log.record.event_name() == Some("operation.log")),
        "capsule bootstrap must export governed breadcrumbs"
    );
}

#[test]
fn conformance_waterfall_has_distinct_rows() {
    let _lock = DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let export = drive_standard_conformance_scenario();
    let spans = export.all_spans();
    let names: std::collections::BTreeSet<_> =
        spans.iter().map(|span| span.name.to_string()).collect();
    assert!(
        names.len() >= 3,
        "expected at least three span names: {names:?}"
    );
    // Capsule lifecycle is an event, never a session-lifetime span.
    assert!(
        export
            .capsule
            .logs
            .get_emitted_logs()
            .unwrap()
            .iter()
            .any(|log| log.record.event_name() == Some("session.start")),
        "capsule bootstrap must export session.start"
    );
    assert!(
        !export
            .capsule
            .spans
            .get_finished_spans()
            .unwrap()
            .iter()
            .any(|span| span.name.as_ref().contains("session")),
        "capsule bootstrap must not export a session span"
    );
}

#[test]
fn conformance_logs_correlate_to_traces() {
    let _lock = DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let export = drive_standard_conformance_scenario();
    let spans = export.all_spans();
    let logs = export.all_logs();
    assert!(!spans.is_empty(), "scenario must export spans");
    assert!(
        logs.iter()
            .filter_map(|log| log.record.trace_context())
            .count()
            > 0,
        "expected logs with active span context"
    );
}

#[test]
fn conformance_export_volume_stays_within_budget() {
    let _lock = DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let export = drive_standard_conformance_scenario();
    let logs = export.all_logs();
    let spans = export.all_spans();
    let metrics = crate::metrics::collect_hot_path_metric_count()
        .expect("collect conformance metric streams");
    // In-test guardrails only (not ratchet input — plan 009 measured path).
    assert!(logs.len() <= MAX_DEBUG_LOGS);
    assert!(spans.len() <= MAX_SPANS);
    // Measured volume artifact for the export-volume ratchet. Only measured
    // counts — no MAX_* ceilings (those stay as test-local guardrails above).
    let volume = serde_json::json!({
        "default_mode_logs": logs.len(),
        "default_mode_spans": spans.len(),
        "default_mode_metrics": metrics,
    });
    let path = telemetry_volume_artifact_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create target/ for volume artifact");
    }
    fs::write(
        &path,
        serde_json::to_string_pretty(&volume).expect("serialize volume"),
    )
    .unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
    // Dropped attribute counts must stay zero under the configured limits.
    for span in &spans {
        assert_eq!(
            span.dropped_attributes_count, 0,
            "span {} dropped attributes",
            span.name
        );
    }
}

#[test]
fn conformance_no_prohibited_keys_or_bracket_bodies_on_records() {
    let _lock = DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let export = drive_standard_conformance_scenario();
    for log in export.all_logs() {
        if let Some(body) = conformance_log_body(&log.record) {
            assert!(!body.starts_with('['), "body has bracket prefix: {body}");
        }
        for key in ["kind", "stage", "detail", "run_id"] {
            assert!(conformance_log_attr(&log.record, key).is_none());
        }
        // Resource excludes run/session/component (plan 002) on host and capsule.
        assert!(
            log.resource
                .get(&opentelemetry::Key::from_static_str("parallax.run.id"))
                .is_none()
        );
        assert!(
            log.resource
                .get(&opentelemetry::Key::from_static_str("jackin.component"))
                .is_none()
        );
    }
}

#[test]
fn conformance_has_no_legacy_screen_span_attributes() {
    let _lock = DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let export = drive_standard_conformance_scenario();
    let spans = export.all_spans();
    assert!(spans.iter().all(|span| {
        span.attributes
            .iter()
            .all(|attribute| attribute.key.as_str() != "jackin.screen.name")
    }));
}

#[test]
fn conformance_has_no_screen_lifetime_spans() {
    let _lock = DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let export = drive_standard_conformance_scenario();
    let spans = export.all_spans();
    assert!(spans.iter().all(|span| {
        !matches!(
            span.name.as_ref(),
            "screen" | "capsule.tab" | "screen.list" | "screen.launch"
        )
    }));
}
