// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Confirm and keyboard-help key handling.

use crossterm::event::KeyEvent;

use super::super::InputOutcome;

use crate::tui::screens::workspaces::update::{
    InstancePurgeKeyPlan, WorkspaceDeleteKeyPlan, instance_purge_key_plan,
    workspace_delete_key_plan,
};
use crate::tui::state::update::{ManagerMessage, update_manager};
use crate::tui::state::{ManagerEffect, ManagerStage, ManagerState};

pub(crate) fn handle_confirm_instance_purge_key(
    state: &mut ManagerState<'_>,
    key: KeyEvent,
) -> InputOutcome {
    let ManagerStage::ConfirmInstancePurge {
        container,
        state: confirm_state,
        ..
    } = &mut state.stage
    else {
        return InputOutcome::Continue;
    };
    let plan = instance_purge_key_plan(confirm_state.handle_key(key.into()), container.clone());
    match plan {
        InstancePurgeKeyPlan::Purge { container } => {
            update_manager(state, ManagerMessage::ReturnToList);
            crate::tui::state::update::record_manager_action(
                state,
                jackin_telemetry::schema::enums::UiActionName::InstancePurge,
            );
            InputOutcome::InstanceAction {
                container,
                action: crate::tui::message::ConsoleInstanceAction::Purge,
            }
        }
        InstancePurgeKeyPlan::ReturnToList => {
            update_manager(state, ManagerMessage::ReturnToList);
            InputOutcome::Continue
        }
        InstancePurgeKeyPlan::Continue => InputOutcome::Continue,
    }
}

pub(crate) fn handle_confirm_delete_key(
    state: &mut ManagerState<'_>,
    cwd: &std::path::Path,
    key: KeyEvent,
) -> InputOutcome {
    let ManagerStage::ConfirmDelete {
        name,
        state: confirm_state,
    } = &mut state.stage
    else {
        return InputOutcome::Continue;
    };
    let plan = workspace_delete_key_plan(confirm_state.handle_key(key.into()), name.clone());
    match plan {
        WorkspaceDeleteKeyPlan::RemoveWorkspace { name } => {
            update_manager(state, ManagerMessage::ReturnToList);
            crate::tui::state::update::record_manager_action(
                state,
                jackin_telemetry::schema::enums::UiActionName::WorkspaceDelete,
            );
            state.request_effect(ManagerEffect::RemoveWorkspace {
                name,
                cwd: cwd.to_path_buf(),
            });
            InputOutcome::Continue
        }
        WorkspaceDeleteKeyPlan::ReturnToList => {
            update_manager(state, ManagerMessage::ReturnToList);
            InputOutcome::Continue
        }
        WorkspaceDeleteKeyPlan::Continue => InputOutcome::Continue,
    }
}

/// Route a key to the open keyboard-help overlay. Closing (Esc, per the
/// upstream modal state) clears the overlay slot so the next key falls
/// through to the stage underneath — focus restore is automatic.
pub(crate) fn handle_keyboard_help(state: &mut ManagerState<'_>, key: KeyEvent) -> InputOutcome {
    if state.keyboard_help.is_none() {
        return InputOutcome::Continue;
    }
    let entries = crate::tui::components::keyboard_help::console_help_entries(
        state,
        &termrock::style::DesignSystem::default(),
    );
    let Some(help) = state.keyboard_help.as_mut() else {
        return InputOutcome::Continue;
    };
    if matches!(
        help.handle_key(key.into(), &entries),
        termrock::widgets::KeyboardHelpOutcome::Closed
    ) {
        state.keyboard_help = None;
    }
    InputOutcome::Continue
}
