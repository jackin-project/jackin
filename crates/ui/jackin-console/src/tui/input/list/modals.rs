// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! List modal and github-open handling.

use super::super::InputOutcome;
use super::dispatch_manager;
use crossterm::event::{KeyCode, KeyEvent};

use crate::tui::components::github_picker::GithubOpenPlan;

use crate::tui::screens::workspaces::update::workspace_list_github_open_plan;

use crate::tui::state::update::ManagerMessage;
use crate::tui::state::{ManagerEffect, ManagerState, Modal};
use crate::tui::update::{
    DismissibleModalPlan, ListGithubPickerPlan, ListModalKeyTarget, ListRolePickerPlan,
    dismissible_modal_plan, list_github_picker_plan, list_role_picker_plan,
};
use jackin_config::AppConfig;

/// Dispatch the `o` key on the workspace list view.
pub(crate) fn handle_list_open_in_github(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
) -> InputOutcome {
    let selected_workspace_name = state
        .selected_workspace_summary()
        .map(|summary| summary.name.as_str());
    match workspace_list_github_open_plan(selected_workspace_name, config, &state.mount_info_cache)
    {
        GithubOpenPlan::Continue => InputOutcome::Continue,
        GithubOpenPlan::OpenUrl(url) => {
            state.request_effect(ManagerEffect::OpenUrl(url));
            InputOutcome::Continue
        }
        GithubOpenPlan::Pick(picker_state) => {
            dispatch_manager(
                state,
                ManagerMessage::OpenListGithubPicker {
                    state: *picker_state,
                },
            );
            InputOutcome::Continue
        }
    }
}

/// Dispatch a key into whatever modal currently sits on `state.list_modal`.
pub fn handle_list_modal(state: &mut ManagerState<'_>, key: KeyEvent) -> InputOutcome {
    // Pre-compute the Debug-info dialog rect (immutable borrow) so the scroll
    // can be clamped to the content after the key is handled — without this the
    // offset inflates past the end and the opposite key feels dead while it
    // unwinds.
    let container_info_rect = state
        .list_modal
        .as_ref()
        .and_then(|modal| modal.container_info_rect(state.cached_term_size));
    let Some(modal) = state.list_modal.as_mut() else {
        return InputOutcome::Continue;
    };
    let target = modal.list_key_target();
    match (target, modal) {
        (ListModalKeyTarget::GithubPicker, Modal::GithubPicker { state: picker }) => {
            match list_github_picker_plan(picker.handle_key(key)) {
                ListGithubPickerPlan::OpenUrl(url) => {
                    dispatch_manager(state, ManagerMessage::DismissListModal);
                    state.request_effect(ManagerEffect::OpenUrl(url));
                    InputOutcome::Continue
                }
                ListGithubPickerPlan::Dismiss => {
                    dispatch_manager(state, ManagerMessage::DismissListModal);
                    InputOutcome::Continue
                }
                ListGithubPickerPlan::Continue => InputOutcome::Continue,
            }
        }
        (ListModalKeyTarget::RolePicker, Modal::RolePicker { state: picker }) => {
            match list_role_picker_plan(picker.handle_key(key)) {
                ListRolePickerPlan::Launch(role) => {
                    dispatch_manager(state, ManagerMessage::DismissListModal);
                    InputOutcome::LaunchWithAgent(role)
                }
                ListRolePickerPlan::Dismiss => {
                    dispatch_manager(state, ManagerMessage::DismissListModal);
                    InputOutcome::Continue
                }
                ListRolePickerPlan::Continue => InputOutcome::Continue,
            }
        }
        (ListModalKeyTarget::ErrorPopup, Modal::ErrorPopup { state: popup }) => {
            match dismissible_modal_plan(popup.handle_key(key.into())) {
                DismissibleModalPlan::Dismiss => {
                    dispatch_manager(state, ManagerMessage::DismissListModal);
                    InputOutcome::Continue
                }
                DismissibleModalPlan::Continue => InputOutcome::Continue,
            }
        }
        (ListModalKeyTarget::ContainerInfo, Modal::ContainerInfo { state: info }) => {
            if matches!(key.code, KeyCode::Enter)
                && let Some((row, payload)) = info.keyboard_copy_payload()
            {
                state.request_effect(ManagerEffect::CopyContainerInfoValue { row, payload });
                return InputOutcome::Continue;
            }
            let outcome = if let Some(rect) = container_info_rect {
                info.set_viewport(rect);
                info.handle_key(key)
            } else {
                info.handle_key(key)
            };
            if let Some(rect) = container_info_rect {
                info.clamp_scroll(rect);
            }
            match outcome {
                jackin_tui::operator_info::OperatorInfoOutcome::Cancel => {
                    dispatch_manager(state, ManagerMessage::DismissListModal);
                    InputOutcome::Continue
                }
                jackin_tui::operator_info::OperatorInfoOutcome::Continue => InputOutcome::Continue,
            }
        }
        (ListModalKeyTarget::Dismiss, _) => {
            dispatch_manager(state, ManagerMessage::DismissListModal);
            InputOutcome::Continue
        }
        _ => InputOutcome::Continue,
    }
}
