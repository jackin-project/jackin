// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{StepCounter, stage_index, telemetry_stage};
use crate::runtime::progress::LaunchProgress;
use jackin_launch::{LaunchCancelled, LaunchDiagnostics};
use std::sync::Arc;

const LAUNCH_WIRE_CHILD: &str = "JACKIN_LAUNCH_WIRE_CHILD";

struct TestDiagnostics;

impl LaunchDiagnostics for TestDiagnostics {
    fn run_id(&self) -> &'static str {
        "test-run"
    }
    fn compact(&self, _kind: &str, _message: &str) {}
    fn error(&self, _kind: &str, _message: &str, _error_type: Option<&str>) {}
    fn stage(
        &self,
        _kind: &str,
        _stage: jackin_core::LaunchStage,
        _message: &str,
        _detail: Option<&str>,
    ) {
    }
}

/// A `StepCounter` carrying a rich-surface token, optionally pre-cancelled
/// to stand in for an operator who has already hit Ctrl+C.
fn steps_with_progress(cancelled: bool) -> StepCounter {
    let progress = LaunchProgress::for_test(Arc::new(TestDiagnostics));
    if cancelled {
        progress.cancel_token().cancel();
    }
    let mut steps = StepCounter::new(
        "test-role",
        jackin_telemetry::schema::enums::LaunchTargetKind::Directory,
    );
    steps.start_progress(progress);
    steps
}

#[tokio::test]
async fn next_bails_at_checkpoint_when_cancelled() {
    let mut steps = steps_with_progress(true);
    let err = steps
        .next("Launching role")
        .await
        .expect_err("a cancelled token must abort at the step boundary");
    assert!(
        LaunchCancelled::is_cancel(&err),
        "cancel must carry the typed sentinel, not a generic error: {err}"
    );
}

#[tokio::test]
async fn next_proceeds_when_not_cancelled() {
    let mut steps = steps_with_progress(false);
    steps
        .next("Launching role")
        .await
        .expect("an un-cancelled step boundary must proceed");
    steps.stage_done(jackin_core::LaunchStage::Capsule, "ready");
}

#[test]
fn launch_stage_registry_mapping_is_exhaustive_and_ordered() {
    for (index, stage) in jackin_core::LaunchStage::ALL.into_iter().enumerate() {
        assert_eq!(stage_index(stage), index);
        assert_eq!(
            telemetry_stage(stage),
            jackin_telemetry::schema::enums::LaunchStageName::ALL[index]
        );
    }
}

#[test]
fn renderer_teardown_does_not_end_hardline_telemetry() {
    let mut steps = steps_with_progress(false);
    steps.opening_hardline();
    let index = stage_index(jackin_core::LaunchStage::Hardline);
    assert!(steps.stage_telemetry[index].is_some());

    steps.finish_progress();
    assert!(steps.stage_telemetry[index].is_some());

    steps.stage_done(jackin_core::LaunchStage::Hardline, "open");
    assert!(steps.stage_telemetry[index].is_none());
}

#[test]
fn conformance_wire_representative_launch_exports_complete_private_pipeline() -> anyhow::Result<()>
{
    if std::env::var_os(LAUNCH_WIRE_CHILD).is_none() {
        let status = std::process::Command::new(std::env::current_exe()?)
            .arg("--exact")
            .arg(
                "runtime::launch::progress_helpers::tests::conformance_wire_representative_launch_exports_complete_private_pipeline",
            )
            .arg("--nocapture")
            .env(LAUNCH_WIRE_CHILD, "1")
            .status()?;
        anyhow::ensure!(status.success(), "isolated launch wire test failed");
        return Ok(());
    }

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let testbed = runtime.block_on(async { jackin_otlp_testbed::Testbed::start() })?;
    jackin_diagnostics::init_wire_test_export(
        &testbed.endpoint(),
        jackin_diagnostics::ServiceIdentity::HOST_ONE_SHOT,
    )?;

    let private_role = "wire-private-launch-role";
    let private_failure = jackin_core::LaunchFailure {
        title: "wire-private-launch-title".to_owned(),
        summary: "wire-private-launch-summary".to_owned(),
        detail: Some("wire-private-launch-detail".to_owned()),
        next_step: Some("wire-private-launch-next-step".to_owned()),
        stage: jackin_core::LaunchStage::Capsule,
    };
    let root_attrs = [jackin_telemetry::Attr {
        key: jackin_telemetry::schema::attrs::LAUNCH_TARGET_KIND,
        value: jackin_telemetry::Value::Str("directory"),
    }];
    let root =
        jackin_telemetry::operation_or_disabled(&jackin_telemetry::operation::LAUNCH, &root_attrs);
    {
        let root_span = root.span().clone();
        let _entered = root_span.enter();
        let mut steps = StepCounter::new(
            private_role,
            jackin_telemetry::schema::enums::LaunchTargetKind::Directory,
        );
        for stage in jackin_core::LaunchStage::ALL {
            steps.stage_started(stage, format!("wire-private-stage-{}", stage.label()));
            if stage == jackin_core::LaunchStage::Sidecar {
                steps.stage_skipped(stage, "wire-private-skip-reason");
            } else if stage == jackin_core::LaunchStage::Capsule {
                runtime.block_on(steps.stage_failed(private_failure.clone()));
            } else {
                steps.stage_done(stage, "wire-private-stage-done");
            }
        }
    }
    root.complete(
        jackin_telemetry::schema::enums::OutcomeValue::Failure,
        Some(jackin_telemetry::schema::enums::ErrorType::LaunchFailed),
    );
    jackin_diagnostics::flush_wire_test_export()?;

    let delivered =
        runtime.block_on(testbed.wait_for_all_signals(std::time::Duration::from_secs(2)));
    assert!(
        delivered,
        "launch trace, event, and metric signals must arrive"
    );
    let spans = testbed.spans();
    let launch_roots = spans
        .iter()
        .filter(|span| span.name == "launch")
        .collect::<Vec<_>>();
    let stages = spans
        .iter()
        .filter(|span| span.name == "launch.stage")
        .collect::<Vec<_>>();
    assert_eq!(
        launch_roots.len(),
        1,
        "launch root must export exactly once"
    );
    assert_eq!(
        stages.len(),
        11,
        "every launch stage must export exactly once"
    );
    let root_span = launch_roots[0];
    for stage in &stages {
        assert_eq!(stage.trace_id, root_span.trace_id);
        assert_eq!(stage.parent_span_id, root_span.span_id);
    }
    let stage_wire = format!("{stages:?}");
    for expected in jackin_telemetry::schema::enums::LaunchStageName::ALL {
        assert!(
            stage_wire.contains(expected.as_str()),
            "missing governed launch stage {}: {stage_wire}",
            expected.as_str()
        );
    }
    assert!(stage_wire.contains("launch_stage_failed"));

    let events = testbed.log_records();
    assert_eq!(
        events
            .iter()
            .filter(|event| event.event_name == "launch.stage.started")
            .count(),
        11
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.event_name == "launch.stage.done")
            .count(),
        9
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.event_name == "launch.stage.skipped")
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.event_name == "launch.stage.failed")
            .count(),
        1
    );
    let metric_names = testbed.metric_names();
    for expected in [
        "launch.stage.active",
        "launch.stage.duration",
        "launch.stage.executions",
    ] {
        assert!(metric_names.iter().any(|name| name == expected));
    }
    let prohibited = [
        private_role,
        "wire-private-launch-title",
        "wire-private-launch-summary",
        "wire-private-launch-detail",
        "wire-private-launch-next-step",
        "wire-private-skip-reason",
        "wire-private-stage-done",
    ];
    assert_eq!(
        testbed.prohibited_value_violations(&prohibited),
        Vec::<String>::new()
    );
    assert_eq!(testbed.legacy_namespace_violations(), Vec::<String>::new());
    jackin_diagnostics::shutdown_capsule_tracing();
    Ok(())
}
