// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_workflow::{WorkflowBackend, WorkflowService};

fn round_trip(
    layout: &DaemonLayout,
    attention: &mut impl AttentionNotifierAdapter,
    workflow: &mut dyn WorkflowBackend,
    kind: DaemonRequestKind,
) -> DaemonResponseKind {
    let request = DaemonRequest {
        id: "workflow".to_owned(),
        protocol_version: DAEMON_PROTOCOL_VERSION,
        build_id: "test-build".to_owned(),
        ctx: TelemetryContext::v1(),
        kind,
    };
    handle_request_line_with_workflow(
        &serde_json::to_string(&request).unwrap(),
        layout,
        "test-build",
        &CoredumpPolicy::Disabled,
        attention,
        workflow,
    )
    .kind
}

#[test]
fn workflow_run_control_round_trip() {
    let (temp, _paths, layout) = layout();
    let mut attention = AttentionAdapter::new(RecordingNotifier::default());
    let mut workflow = WorkflowService::open(
        &temp.path().join("workflow-runs.db"),
        &temp.path().join("workflow-artifacts"),
    )
    .unwrap();

    let DaemonResponseKind::WorkflowRun { summary } = round_trip(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowStart {
            plan_hash: "ebc1278f".to_owned(),
            max_repair_turns: 2,
            idempotency_key: "key-1".to_owned(),
        },
    ) else {
        panic!("expected a workflow run summary");
    };
    assert_eq!(summary.status, "active");
    assert_eq!(summary.stage, "prepare");
    assert_eq!(summary.round, 0);
    let run_id = summary.run_id.clone();

    let busy = round_trip(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowStart {
            plan_hash: "ebc1278f".to_owned(),
            max_repair_turns: 2,
            idempotency_key: "key-2".to_owned(),
        },
    );
    assert!(
        matches!(busy, DaemonResponseKind::Error { .. }),
        "second key while active must be busy"
    );

    let DaemonResponseKind::WorkflowRun { summary } = round_trip(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowStatus {
            run_id: Some(run_id.clone()),
        },
    ) else {
        panic!("expected a workflow status summary");
    };
    assert_eq!(summary.stage, "prepare");

    let DaemonResponseKind::WorkflowRun { summary } = round_trip(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowPause {
            run_id: run_id.clone(),
            reason: "hold".to_owned(),
            checkpoint: false,
        },
    ) else {
        panic!("expected a paused summary");
    };
    assert_eq!(summary.status, "paused");

    let DaemonResponseKind::WorkflowRun { summary } = round_trip(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowResume {
            run_id: run_id.clone(),
        },
    ) else {
        panic!("expected a resumed summary");
    };
    assert_eq!(summary.status, "active");

    let DaemonResponseKind::WorkflowRun { summary } = round_trip(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowCancel {
            run_id: run_id.clone(),
            reason: "done".to_owned(),
        },
    ) else {
        panic!("expected a cancelled summary");
    };
    assert_eq!(summary.status, "cancelled");
}
