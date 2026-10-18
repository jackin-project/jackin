// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_workflow::{RunDetail, RunSummary, StoredSession};

use super::super::message::WorkflowsMessage;
use super::super::model::{ObserveTarget, WorkflowAction, WorkflowScreen};
use super::apply_workflows_message;

fn summary(run_id: &str) -> RunSummary {
    RunSummary {
        run_id: run_id.to_owned(),
        plan_hash: "plan".to_owned(),
        status: "active".to_owned(),
        stage: "implement".to_owned(),
        round: 0,
        candidate: None,
        candidate_generation: 0,
        version: 0,
        owner_epoch: 1,
        pause_kind: None,
        max_repair_turns: 3,
    }
}

fn detail_with_session() -> RunDetail {
    RunDetail {
        summary: summary("run-1"),
        sessions: vec![StoredSession {
            run_id: "run-1".to_owned(),
            worker_id: "implementer".to_owned(),
            instance_id: Some("inst-1".to_owned()),
            capsule_epoch: Some(2),
            session_id: Some(7),
            process_identity: Some("pid:9".to_owned()),
            agent_profile: None,
            auth_profile: None,
        }],
        effects: Vec::new(),
        events: Vec::new(),
        evidence: Vec::new(),
        checkpoints: Vec::new(),
    }
}

#[test]
fn destructive_actions_require_confirmation() {
    let mut screen = WorkflowScreen::new(vec![summary("run-1")]);
    screen.set_detail(detail_with_session());
    // Request alone emits nothing.
    assert!(
        apply_workflows_message(
            &mut screen,
            WorkflowsMessage::RequestAction(WorkflowAction::Cancel)
        )
        .is_empty()
    );
    assert_eq!(screen.confirm(), Some(WorkflowAction::Cancel));
    let effects = apply_workflows_message(&mut screen, WorkflowsMessage::ConfirmAction);
    assert_eq!(effects.len(), 1);
    assert!(screen.confirm().is_none());
}

#[test]
fn observe_resolves_the_recorded_session() {
    let mut screen = WorkflowScreen::new(vec![summary("run-1")]);
    screen.set_detail(detail_with_session());
    let effects = apply_workflows_message(
        &mut screen,
        WorkflowsMessage::Observe("implementer".to_owned()),
    );
    assert_eq!(effects.len(), 1);
    let target = ObserveTarget {
        run_id: "run-1".to_owned(),
        worker_id: "implementer".to_owned(),
        instance_id: Some("inst-1".to_owned()),
        session_id: Some(7),
        capsule_epoch: Some(2),
        process_identity: Some("pid:9".to_owned()),
    };
    assert_eq!(screen.observing(), Some(&target));
}

#[test]
fn observe_unknown_worker_sets_error() {
    let mut screen = WorkflowScreen::new(vec![summary("run-1")]);
    screen.set_detail(detail_with_session());
    let effects = apply_workflows_message(
        &mut screen,
        WorkflowsMessage::Observe("reviewer".to_owned()),
    );
    assert!(effects.is_empty());
    assert!(screen.error().is_some());
}
