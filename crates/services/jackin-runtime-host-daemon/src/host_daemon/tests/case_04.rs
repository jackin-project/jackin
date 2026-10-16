// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_workflow::{InstanceInventory, UnavailableBackend, WorkflowBackend, WorkflowService};

fn round_trip(
    layout: &DaemonLayout,
    attention: &mut impl AttentionNotifierAdapter,
    workflow: &mut dyn WorkflowBackend,
    kind: DaemonRequestKind,
) -> DaemonResponseKind {
    let request = DaemonRequest {
        id: "recovery-wire".to_owned(),
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
fn dispatch_intent_rpc_records_without_transport_and_startup_recovery_marks_uncertain() {
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
            idempotency_key: "key-dispatch".to_owned(),
        },
    ) else {
        panic!("expected a workflow run summary");
    };
    let run_id = summary.run_id.clone();

    // The dispatch-intent RPC commits queued->dispatching with no
    // transport send; the response carries the command identity.
    let DaemonResponseKind::WorkflowDispatchIntent { summary, dispatch } = round_trip(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowRecordDispatchIntent {
            run_id: run_id.clone(),
            session_ref: "session:7".to_owned(),
            prompt: "implement the fix".to_owned(),
            turn: 0,
            lease_generation: 0,
        },
    ) else {
        panic!("expected a dispatch intent response");
    };
    assert_eq!(summary.run_id, run_id);
    assert!(!dispatch.command_id.is_empty());
    assert!(!dispatch.prompt_digest.is_empty());

    // The startup pass (the same `startup_recovery` call `serve()`
    // runs after the epoch bump) marks the pending dispatch
    // delivery_unknown and escalates the run.
    let report = startup_recovery(&mut workflow, &InstanceInventory::default())
        .expect("startup recovery runs");
    assert!(
        report
            .actions
            .iter()
            .any(|action| action.contains("delivery_unknown")),
        "startup recovery marks the intent, got: {:?}",
        report.actions
    );
    let saved = workflow
        .run_status(Some(run_id.as_str()))
        .expect("reload saved run");
    assert_eq!(saved.status, "needs_operator");
}

#[test]
fn startup_recovery_failure_returns_none_so_serving_continues() {
    let mut workflow = UnavailableBackend::new("no store");
    assert!(
        startup_recovery(&mut workflow, &InstanceInventory::default()).is_none(),
        "a failed startup pass must not prevent serving"
    );
}
