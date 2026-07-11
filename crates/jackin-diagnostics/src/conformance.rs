//! Telemetry conformance scenario driver (plan 044).
//!
//! Replays a representative launch against the in-memory OTLP rig using only
//! public diagnostics APIs. Tests in `conformance/tests.rs` assert the
//! dossier acceptance checks permanently.

#![cfg(all(test, feature = "otlp"))]

use crate::observability::{TestExport, test_layers};
use crate::operation::{OperationLevel, operation_log, operation_span};
use crate::screen::{Screen, enter_screen};
use crate::RunDiagnostics;
use jackin_core::JackinPaths;

/// In-source export-volume budgets (measured seeds + ~20% slack).
/// Migrate into plan 017's ratchet engine when that lands.
pub(crate) const MAX_DEBUG_LOGS: usize = 64;
pub(crate) const MAX_SPANS: usize = 48;

/// Drive the standard launch scenario under an in-memory OTLP export rig.
pub(crate) fn drive_standard_scenario() -> TestExport {
    let (export, subscriber) = test_layers(true, "conformance-run");
    tracing::subscriber::with_default(subscriber, || {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = JackinPaths::for_tests(tmp.path());
        let run = RunDiagnostics::start(&paths, true, "conformance").expect("run start");
        let _guard = run.activate();

        let list = enter_screen(Screen::List);
        list.in_scope(|| {
            operation_log(
                OperationLevel::Info,
                "conformance.list",
                "screen",
                "list entered",
                &[],
            );
        });
        drop(list);

        let launch = enter_screen(Screen::Launch);
        launch.in_scope(|| {
            run.stage("stage_started", "prepare", "preparing", None);
            run.stage("stage_done", "prepare", "ready", None);
            run.stage("stage_started", "derived image", "building", None);
            run.stage("stage_done", "derived image", "built", None);
            run.stage("stage_started", "start container", "starting", None);
            run.stage("stage_done", "start container", "started", None);

            let span = operation_span(
                crate::otel_events::PROCESS_EXECUTE,
                &[(crate::otel_keys::PROCESS_COMMAND, "true".into())],
            );
            let _g = span.enter();
            operation_log(
                OperationLevel::Info,
                "conformance.op",
                "docker",
                "process executed",
                &[],
            );
            drop(_g);

            run.error_typed(
                "E_CONFORM",
                "forced failure for conformance",
                Some("conformance_error"),
            );
            // Expected-shutdown shaped detach (not failure).
            run.compact(crate::otel_events::SESSION_DETACH, "operator detached");

            for _ in 0..100 {
                crate::metrics::record_frame(32, 1, 4);
                crate::metrics::record_render(50, 4);
            }

            // Synthetic secret-shaped value — must not appear unredacted in export.
            operation_log(
                OperationLevel::Info,
                "conformance.secret",
                "security",
                "token=abc123FAKE_not_a_real_secret",
                &[],
            );
        });
        drop(launch);
    });
    drop(export.logger_provider.force_flush());
    drop(export.tracer_provider.force_flush());
    export
}

mod tests;
