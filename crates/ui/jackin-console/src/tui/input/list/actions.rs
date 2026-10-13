// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! List edit, delete, and settings actions.

use super::{ConcreteInstanceAction, selected_instance_container};

use super::super::InputOutcome;

use crate::tui::components::error_popup::{
    instance_unavailable_error_message, instance_unavailable_error_title, no_instance_error_title,
    no_purgeable_instance_for_workspace_message,
};

use crate::tui::layout::list_body_area;

use crate::tui::screens::workspaces::update::{
    SelectedInstanceActionPlan, SelectedInstancePurgeConfirmPlan, WorkspaceInstanceAction,
    WorkspaceListDeletePlan, WorkspaceListEditPlan, WorkspaceListNewSessionOpenPlan,
    WorkspaceListSettingsPlan, selected_instance_action_plan, selected_instance_purge_confirm_plan,
    workspace_instance_empty_message, workspace_list_new_session_open_plan,
    workspace_list_new_session_plan,
};
use crate::tui::screens::workspaces::view::instance_purge_confirm_label;
use crate::tui::state::update::{ManagerMessage, update_manager};
use crate::tui::state::{
    AgentChoiceState, EditorState, ManagerEffect, ManagerState, SettingsState,
};
use crate::tui::update::apply_inline_new_session_picker_plan;
use jackin_config::AppConfig;

/// Open the new-session agent picker for the selected instance row: a live
/// container gets the agent picker with rows read from its admitted manifest,
/// anything else gets the create-workspace or instance-unavailable path.
pub(crate) fn open_new_session_picker(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
) -> anyhow::Result<()> {
    match workspace_list_new_session_open_plan(
        workspace_list_new_session_plan(state.selected_row()),
        |workspace_idx, instance_idx| {
            // Tree rows index the visible list (live + failed). A new
            // session can only attach to a live container, so resolve
            // by visible index but yield a container only when running.
            state
                .workspace_visible_instances(workspace_idx)
                .get(instance_idx)
                .filter(|entry| {
                    matches!(
                        entry.status,
                        jackin_core::InstanceStatus::Active | jackin_core::InstanceStatus::Running
                    )
                })
                .map(|entry| entry.container_base.clone())
        },
    ) {
        WorkspaceListNewSessionOpenPlan::OpenPicker { container } => {
            let picker = AgentChoiceState::with_choices(jackin_core::Agent::ALL.to_vec());
            let accounts = state
                .live_instance_admissions
                .get(&container)
                .map(|admissions| {
                    crate::services::launch::account_choices_for_live_instances(config, admissions)
                })
                .unwrap_or_default();
            apply_inline_new_session_picker_plan(state, container, picker, accounts);
        }
        WorkspaceListNewSessionOpenPlan::OpenCreateWorkspace => {
            state.request_effect(ManagerEffect::OpenCreatePreludeFileBrowser);
        }
        WorkspaceListNewSessionOpenPlan::OpenInstanceUnavailableError => {
            dispatch_manager(
                state,
                ManagerMessage::OpenListErrorPopup {
                    title: instance_unavailable_error_title().into(),
                    message: instance_unavailable_error_message().into(),
                },
            );
        }
    }
    Ok(())
}

pub(crate) fn dispatch_workspace_list_edit(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    plan: WorkspaceListEditPlan,
) {
    let WorkspaceListEditPlan::OpenEditor { workspace_idx } = plan else {
        return;
    };
    let Some(summary) = state.workspaces.get(workspace_idx) else {
        return;
    };
    let name = summary.name.clone();
    if let Some(ws) = config.workspaces.get(&name) {
        dispatch_manager(
            state,
            ManagerMessage::EnterEditor(EditorState::new_edit(name, ws.clone())),
        );
    }
}

pub(crate) fn dispatch_workspace_list_delete(
    state: &mut ManagerState<'_>,
    plan: WorkspaceListDeletePlan,
) {
    let WorkspaceListDeletePlan::ConfirmDelete { workspace_idx } = plan else {
        return;
    };
    if let Some(ws) = state.workspaces.get(workspace_idx) {
        let name = ws.name.clone();
        dispatch_manager(state, ManagerMessage::EnterConfirmDelete { name });
    }
}

pub(crate) fn dispatch_workspace_list_settings(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    plan: WorkspaceListSettingsPlan,
) {
    if matches!(plan, WorkspaceListSettingsPlan::OpenSettings) {
        dispatch_manager(
            state,
            ManagerMessage::EnterSettings(SettingsState::from_config(config)),
        );
    }
}

pub(crate) fn console_instance_action_and_empty_message(
    action: WorkspaceInstanceAction,
) -> (ConcreteInstanceAction, &'static str) {
    let action_message = workspace_instance_empty_message(action);
    let action = match action {
        WorkspaceInstanceAction::Reconnect => ConcreteInstanceAction::Reconnect,
        WorkspaceInstanceAction::NewSession => ConcreteInstanceAction::NewSession,
        WorkspaceInstanceAction::Shell => ConcreteInstanceAction::Shell,
        WorkspaceInstanceAction::Inspect => ConcreteInstanceAction::Inspect,
        WorkspaceInstanceAction::Stop => ConcreteInstanceAction::Stop,
        WorkspaceInstanceAction::Purge => ConcreteInstanceAction::Purge,
    };
    (action, action_message)
}

pub(crate) fn clamp_list_scroll_after_key(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
) {
    let area = state.cached_term_size;
    let body = list_body_area(area);
    crate::tui::layout::list::clamp_list_scroll_for_area(body, state, config, cwd);
}

pub(crate) fn dispatch_manager(state: &mut ManagerState<'_>, message: ManagerMessage) {
    update_manager(state, message);
}

pub(crate) fn instance_action_outcome(
    state: &mut ManagerState<'_>,
    action: ConcreteInstanceAction,
    empty_message: &str,
) -> InputOutcome {
    match selected_instance_action_plan(selected_instance_container(state, action)) {
        SelectedInstanceActionPlan::Start { container } => {
            InputOutcome::InstanceAction { container, action }
        }
        SelectedInstanceActionPlan::OpenError => {
            dispatch_manager(
                state,
                ManagerMessage::OpenListErrorPopup {
                    title: no_instance_error_title().into(),
                    message: empty_message.into(),
                },
            );
            InputOutcome::Continue
        }
    }
}

/// Resolve the container for Purge, then stage a Y/N confirmation
/// modal. Purge now also calls `eject_role` before deleting preserved
/// state (so a mis-keyed `P` on a running instance destroys role +
/// `DinD` + volume + network plus on-disk state in one stroke), so the
/// confirmation step is non-optional. Mirrors the workspace-delete
/// pattern at `handle_list_key` line 158.
pub(crate) fn confirm_purge_outcome(state: &mut ManagerState<'_>) -> InputOutcome {
    match selected_instance_purge_confirm_plan(
        selected_instance_container(state, ConcreteInstanceAction::Purge),
        |container| {
            state
                .instances
                .iter()
                .find(|entry| entry.container_base == container)
                .map_or_else(
                    || instance_purge_confirm_label(container, None),
                    |entry| {
                        instance_purge_confirm_label(&entry.container_base, Some(&entry.role_key))
                    },
                )
        },
    ) {
        SelectedInstancePurgeConfirmPlan::OpenConfirm { container, label } => {
            dispatch_manager(
                state,
                ManagerMessage::EnterConfirmInstancePurge { container, label },
            );
            InputOutcome::Continue
        }
        SelectedInstancePurgeConfirmPlan::OpenError => {
            dispatch_manager(
                state,
                ManagerMessage::OpenListErrorPopup {
                    title: no_instance_error_title().into(),
                    message: no_purgeable_instance_for_workspace_message().into(),
                },
            );
            InputOutcome::Continue
        }
    }
}
