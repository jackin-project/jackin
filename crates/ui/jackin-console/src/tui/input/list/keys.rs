// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Workspace-list key handling.

use super::super::InputOutcome;
use super::{
    clamp_list_scroll_after_key, confirm_purge_outcome, console_instance_action_and_empty_message,
    dispatch_manager, dispatch_workspace_list_delete, dispatch_workspace_list_edit,
    dispatch_workspace_list_settings, handle_list_left_right, handle_list_open_in_github,
    handle_preview_focused_key, instance_action_outcome, open_new_session_picker,
    selected_instance_container,
};
use crossterm::event::{KeyCode, KeyEvent};

use crate::tui::components::error_popup::no_recoverable_instance_selected_message;

use crate::tui::message::ConsoleInstanceAction;

use crate::tui::screens::workspaces::update::{
    WorkspaceListEnterPlan, WorkspaceListKeyPlan, WorkspaceListTopLevelKeyPlan,
    workspace_list_delete_plan, workspace_list_edit_plan, workspace_list_enter_plan,
    workspace_list_prewarm_plan, workspace_list_settings_plan, workspace_list_top_level_key_plan,
};

use crate::tui::state::update::ManagerMessage;
use crate::tui::state::{ManagerEffect, ManagerState};

use jackin_config::AppConfig;
use jackin_core::JackinPaths;

pub(crate) type ConcreteInstanceAction = ConsoleInstanceAction<jackin_core::Agent>;

pub fn handle_list_key(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    _paths: &JackinPaths,
    cwd: &std::path::Path,
    key: KeyEvent,
) -> anyhow::Result<InputOutcome> {
    if key.code == KeyCode::Char('u') {
        if state.usage.screen.is_none() {
            state.usage.screen = Some(crate::tui::state::UsageScreenState::open_with_snapshot(
                state.usage_snapshot.clone(),
            ));
        }
        state.usage.visible = true;
        return Ok(InputOutcome::Continue);
    }
    let selected_row = state.selected_row();
    let selected_preview_pane_count =
        selected_instance_container(state, ConcreteInstanceAction::Reconnect)
            .map(|container| state.flattened_preview_panes(&container).len());
    let plan = workspace_list_top_level_key_plan(
        key.code,
        state.preview_focused,
        selected_row,
        selected_preview_pane_count,
        state.list_scroll_focus().is_some(),
    );
    let plan = match plan {
        WorkspaceListTopLevelKeyPlan::PreviewFocused => {
            return Ok(handle_preview_focused_key(state, key));
        }
        WorkspaceListTopLevelKeyPlan::EnterPreview => {
            dispatch_manager(state, ManagerMessage::EnterPreview);
            return Ok(InputOutcome::Continue);
        }
        WorkspaceListTopLevelKeyPlan::ListKey(plan) => plan,
    };
    match plan {
        WorkspaceListKeyPlan::Exit => Ok(InputOutcome::ExitJackin),
        // Left/Right arrows: tree expand/collapse when the selected row owns
        // that direction, otherwise horizontal scroll if the focused list
        // overflows. h/l remain alternate horizontal-scroll keys.
        WorkspaceListKeyPlan::HorizontalTreeOrScroll { delta } => {
            handle_list_left_right(state, config, cwd, delta);
            Ok(InputOutcome::Continue)
        }
        WorkspaceListKeyPlan::ScrollHorizontal { delta } => {
            dispatch_manager(state, ManagerMessage::ScrollListHorizontal(delta));
            clamp_list_scroll_after_key(state, config, cwd);
            Ok(InputOutcome::Continue)
        }
        WorkspaceListKeyPlan::MoveSelection { delta } => {
            dispatch_manager(state, ManagerMessage::MoveListSelection(delta));
            Ok(InputOutcome::Continue)
        }
        WorkspaceListKeyPlan::ScrollFocusedVertical { delta } => {
            dispatch_manager(state, ManagerMessage::ScrollFocusedListBlockVertical(delta));
            clamp_list_scroll_after_key(state, config, cwd);
            Ok(InputOutcome::Continue)
        }
        WorkspaceListKeyPlan::Enter => match workspace_list_enter_plan(state.selected_row()) {
            WorkspaceListEnterPlan::LaunchCurrentDir => Ok(InputOutcome::LaunchCurrentDir),
            WorkspaceListEnterPlan::CreateNewWorkspace => {
                state.request_effect(ManagerEffect::OpenCreatePreludeFileBrowser);
                Ok(InputOutcome::Continue)
            }
            WorkspaceListEnterPlan::LaunchSavedWorkspace(i) => Ok(state
                .workspaces
                .get(i)
                .map_or(InputOutcome::Continue, |summary| {
                    InputOutcome::LaunchNamed(summary.name.clone())
                })),
            WorkspaceListEnterPlan::InstanceAction => Ok(instance_action_outcome(
                state,
                ConcreteInstanceAction::Reconnect,
                no_recoverable_instance_selected_message(),
            )),
        },
        WorkspaceListKeyPlan::Edit => {
            dispatch_workspace_list_edit(
                state,
                config,
                workspace_list_edit_plan(state.selected_row()),
            );
            Ok(InputOutcome::Continue)
        }
        WorkspaceListKeyPlan::NewSession => {
            open_new_session_picker(state, config)?;
            Ok(InputOutcome::Continue)
        }
        WorkspaceListKeyPlan::Delete => {
            dispatch_workspace_list_delete(state, workspace_list_delete_plan(state.selected_row()));
            Ok(InputOutcome::Continue)
        }
        WorkspaceListKeyPlan::Prewarm => Ok(workspace_list_prewarm_plan(state.selected_row())
            .and_then(|i| state.workspaces.get(i))
            .map_or(InputOutcome::Continue, |summary| {
                InputOutcome::PrewarmNamed(summary.name.clone())
            })),
        WorkspaceListKeyPlan::OpenGithub => Ok(handle_list_open_in_github(state, config)),
        WorkspaceListKeyPlan::InstanceAction(action) => {
            let (action, message) = console_instance_action_and_empty_message(action);
            Ok(instance_action_outcome(state, action, message))
        }
        WorkspaceListKeyPlan::ConfirmPurge => Ok(confirm_purge_outcome(state)),
        WorkspaceListKeyPlan::Settings => {
            dispatch_workspace_list_settings(
                state,
                config,
                workspace_list_settings_plan(state.selected_row()),
            );
            Ok(InputOutcome::Continue)
        }
        WorkspaceListKeyPlan::Continue => Ok(InputOutcome::Continue),
    }
}
