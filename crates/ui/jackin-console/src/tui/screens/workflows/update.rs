// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Pure workflow-screen transitions: each message maps to state
//! changes plus host-executed effects. Resume/cancel/takeover only
//! emit after an explicit confirmation.

use super::effect::WorkflowsEffect;
use super::message::WorkflowsMessage;
use super::model::{WorkflowAction, WorkflowScreen};

/// Apply one message; returns the host effects to execute.
pub fn apply_workflows_message(
    screen: &mut WorkflowScreen,
    message: WorkflowsMessage,
) -> Vec<WorkflowsEffect> {
    match message {
        WorkflowsMessage::SelectNext => {
            screen.move_selection(1);
            Vec::new()
        }
        WorkflowsMessage::SelectPrevious => {
            screen.move_selection(-1);
            Vec::new()
        }
        WorkflowsMessage::OpenDetail => Vec::new(),
        WorkflowsMessage::CloseDetail => {
            screen.close_detail();
            Vec::new()
        }
        WorkflowsMessage::Observe(worker_id) => {
            if let Some(target) = screen.observe_target(&worker_id) {
                screen.set_observing(Some(target.clone()));
                screen.set_error(None);
                vec![WorkflowsEffect::Observe(target)]
            } else {
                screen.set_error(Some(format!(
                    "no recorded session for worker `{worker_id}`"
                )));
                Vec::new()
            }
        }
        WorkflowsMessage::RequestAction(action) => {
            if screen.detail().is_none() {
                screen.set_error(Some("open a run detail first".to_owned()));
                return Vec::new();
            }
            screen.open_confirm(action);
            Vec::new()
        }
        WorkflowsMessage::ConfirmAction => {
            let Some(action) = screen.confirm() else {
                return Vec::new();
            };
            screen.dismiss_confirm();
            let Some(detail) = screen.detail() else {
                return Vec::new();
            };
            let run_id = detail.summary.run_id.clone();
            match action {
                WorkflowAction::Resume => vec![WorkflowsEffect::Resume { run_id }],
                WorkflowAction::Cancel => vec![WorkflowsEffect::Cancel { run_id }],
                WorkflowAction::TakeControl => {
                    if let Some(target) = screen.observe_target("implementer") {
                        vec![WorkflowsEffect::TakeControl(target)]
                    } else {
                        screen.set_error(Some(
                            "no recorded implementer session to take control of".to_owned(),
                        ));
                        Vec::new()
                    }
                }
            }
        }
        WorkflowsMessage::DismissConfirm => {
            screen.dismiss_confirm();
            Vec::new()
        }
        WorkflowsMessage::StopObserving { return_control } => {
            let target = screen.observing().cloned();
            screen.set_observing(None);
            match (target, return_control) {
                (Some(target), true) => vec![WorkflowsEffect::ReleaseControl(target)],
                _ => Vec::new(),
            }
        }
        WorkflowsMessage::RequestExport(out_dir) => {
            let Some(detail) = screen.detail() else {
                screen.set_error(Some("open a run detail first".to_owned()));
                return Vec::new();
            };
            vec![WorkflowsEffect::Export {
                run_id: detail.summary.run_id.clone(),
                out_dir,
            }]
        }
    }
}

#[cfg(test)]
mod tests {
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
}
