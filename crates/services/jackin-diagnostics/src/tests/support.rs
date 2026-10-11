// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) const MAX_DEBUG_LOGS: usize = 64;

pub(super) const MAX_SPANS: usize = 48;

pub(super) const CONFORMANCE_ARGV_CANARY: &str = "--password=conformance-argv-secret";

pub(super) const CONFORMANCE_URL_CANARY: &str =
    "https://example.invalid/api?token=conformance-query-secret";

pub(super) const CONFORMANCE_INSPECT_CANARY: &str =
    r#"{"Config":{"Env":["TOKEN=conformance-inspect-secret"]}}"#;

pub(super) const CONFORMANCE_TERMINAL_CANARY: &str =
    "\u{1b}[31mconformance-terminal-bytes\u{1b}[0m";

pub(super) struct ConformanceExport {
    host: crate::observability::TestExport,
    pub(super) capsule: crate::observability::TestExport,
}

impl ConformanceExport {
    pub(super) fn all_logs(
        &self,
    ) -> Vec<opentelemetry_sdk::logs::in_memory_exporter::LogDataWithResource> {
        let mut logs = self.host.logs.get_emitted_logs().unwrap_or_default();
        logs.extend(self.capsule.logs.get_emitted_logs().unwrap_or_default());
        logs
    }

    pub(super) fn all_spans(&self) -> Vec<opentelemetry_sdk::trace::SpanData> {
        let mut spans = self.host.spans.get_finished_spans().unwrap_or_default();
        spans.extend(self.capsule.spans.get_finished_spans().unwrap_or_default());
        spans
    }
}

pub(super) fn collect_files(root: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
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

pub(super) fn telemetry_volume_artifact_path() -> std::path::PathBuf {
    if let Some(path) = std::env::var_os("JACKIN_TELEMETRY_VOLUME_PATH") {
        return path.into();
    }
    // nextest CWD is the package dir; always write to the workspace target so
    // `cargo xtask lint ratchet` (repo root) consumes the same file.
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../target/telemetry-volume.json")
}

pub(super) fn conformance_operation_log(body: &str) {
    let attrs = [jackin_telemetry::Attr {
        key: jackin_telemetry::schema::attrs::OUTCOME,
        value: jackin_telemetry::Value::Str("success"),
    }];
    jackin_telemetry::emit_event(
        &jackin_telemetry::event::OPERATION_LOG,
        jackin_telemetry::FieldSet::new(&attrs, Some(body)),
    )
    .expect("registered conformance event");
}

pub(super) fn conformance_operation_warning(body: &str) {
    let attrs = [
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::OUTCOME,
            value: jackin_telemetry::Value::Str(
                jackin_telemetry::schema::enums::OutcomeValue::Success.as_str(),
            ),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::ERROR_TYPE,
            value: jackin_telemetry::Value::Str(
                jackin_telemetry::schema::enums::ErrorType::RecoveredDegradation.as_str(),
            ),
        },
    ];
    jackin_telemetry::emit_event(
        &jackin_telemetry::event::OPERATION_WARN,
        jackin_telemetry::FieldSet::new(&attrs, Some(body)),
    )
    .expect("registered conformance warning");
}

pub(super) fn conformance_operation_error(body: &str) {
    let attrs = [
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::OUTCOME,
            value: jackin_telemetry::Value::Str(
                jackin_telemetry::schema::enums::OutcomeValue::Error.as_str(),
            ),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::ERROR_TYPE,
            value: jackin_telemetry::Value::Str(
                jackin_telemetry::schema::enums::ErrorType::RpcError.as_str(),
            ),
        },
    ];
    jackin_telemetry::emit_event(
        &jackin_telemetry::event::ERROR_TYPED,
        jackin_telemetry::FieldSet::new(&attrs, Some(body)),
    )
    .expect("registered conformance error");
}

pub(super) fn drive_standard_conformance_scenario() -> ConformanceExport {
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

        conformance_operation_log("list entered");
        conformance_operation_warning("process retry exhausted");
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
        conformance_operation_log("process executed");
        // Representative host failure; the actual attach failure seam is
        // asserted in jackin-capsule's conformance test.
        conformance_operation_error("forced attach failure for conformance");
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

        conformance_operation_log(&format!(
            "argv={CONFORMANCE_ARGV_CANARY} url={CONFORMANCE_URL_CANARY} inspect={CONFORMANCE_INSPECT_CANARY}"
        ));
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

pub(super) fn conformance_log_body(
    record: &opentelemetry_sdk::logs::SdkLogRecord,
) -> Option<String> {
    use opentelemetry::logs::AnyValue;

    record.body().map(|value| match value {
        AnyValue::String(value) => value.to_string(),
        other => format!("{other:?}"),
    })
}

pub(super) fn conformance_log_attr(
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
