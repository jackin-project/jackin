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
mod tests;
