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

#[test]
fn workflow_list_detail_and_typed_errors() {
    let (temp, _paths, layout) = layout();
    let mut attention = AttentionAdapter::new(RecordingNotifier::default());
    let mut workflow = WorkflowService::open(
        &temp.path().join("workflow-runs.db"),
        &temp.path().join("workflow-artifacts"),
    )
    .unwrap();

    // Empty store: the list is empty, not an error.
    let DaemonResponseKind::WorkflowRuns { summaries } = round_trip(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowList { limit: 50 },
    ) else {
        panic!("expected a workflow run list");
    };
    assert!(summaries.is_empty());

    let DaemonResponseKind::WorkflowRun { summary } = round_trip(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowStart {
            plan_hash: "ebc1278f".to_owned(),
            max_repair_turns: 2,
            idempotency_key: "key-list".to_owned(),
        },
    ) else {
        panic!("expected a workflow run summary");
    };
    let run_id = summary.run_id.clone();

    let DaemonResponseKind::WorkflowRuns { summaries } = round_trip(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowList { limit: 50 },
    ) else {
        panic!("expected a workflow run list");
    };
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].run_id, run_id);

    let DaemonResponseKind::WorkflowRunDetail { detail } = round_trip(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowDetail {
            run_id: run_id.clone(),
            after_seq: -1,
            event_limit: 200,
        },
    ) else {
        panic!("expected a workflow run detail");
    };
    assert_eq!(detail.summary.run_id, run_id);
    assert!(
        detail
            .events
            .iter()
            .any(|event| event.kind == "run_started"),
        "detail carries the durable event history"
    );

    // Unknown run: typed not_found (the CLI maps it to exit 2).
    let DaemonResponseKind::Error { message, code } = round_trip(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowDetail {
            run_id: "run-missing".to_owned(),
            after_seq: -1,
            event_limit: 200,
        },
    ) else {
        panic!("expected a typed workflow error");
    };
    assert_eq!(code, "not_found", "unexpected message: {message}");

    // Second start while active: typed busy (CLI exit 4).
    let DaemonResponseKind::Error { code, .. } = round_trip(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowStart {
            plan_hash: "ebc1278f".to_owned(),
            max_repair_turns: 2,
            idempotency_key: "key-other".to_owned(),
        },
    ) else {
        panic!("expected a busy error");
    };
    assert_eq!(code, "busy");
}
