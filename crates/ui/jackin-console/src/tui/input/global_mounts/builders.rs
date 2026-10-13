// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Settings modal builders and after-event.

use super::{SettingsModalOutcome, commit_text, dispatch_manager};

use crate::tui::screens::settings::update as settings_update;
use crate::tui::screens::settings::update::{GlobalMountScopePickerCommitPlan, RolePickerOpenPlan};
use crate::tui::screens::settings::view::{
    global_mount_confirm_state, global_mount_scope_picker_state, global_mount_text_input_state,
    global_mount_text_target_label, settings_env_text_input_state, settings_error_popup_title,
    settings_no_registered_roles_error_message,
};

use crate::tui::state::update::ManagerMessage;
use crate::tui::state::{
    GlobalMountConfirm, GlobalMountTextTarget, ManagerStage, ManagerState, RolePickerState,
    SettingsEnvTextTarget, SettingsModal,
};

/// Promote any pending error from a settings sub-tab to `settings.error_popup`,
/// pop back to the workspace list when a handler set `exit_requested`.
pub fn after_settings_event(state: &mut ManagerState<'_>) {
    let outcome = {
        let ManagerStage::Settings(settings) = &mut state.stage else {
            return;
        };
        settings.take_after_event_outcome()
    };
    if let Some(msg) = outcome.error {
        dispatch_manager(
            state,
            ManagerMessage::OpenSettingsErrorPopup {
                title: settings_error_popup_title().into(),
                message: msg,
            },
        );
    }
    if outcome.exit_requested {
        dispatch_manager(state, ManagerMessage::ReturnToList);
    }
}

pub(crate) fn confirm_modal(action: GlobalMountConfirm) -> SettingsModal<'static> {
    SettingsModal::MountConfirm {
        action,
        state: global_mount_confirm_state(action),
    }
}

pub(crate) fn scope_picker_modal() -> SettingsModal<'static> {
    SettingsModal::MountScopePicker {
        state: global_mount_scope_picker_state(),
    }
}

pub(crate) fn commit_add_scope_choice(
    settings: &mut crate::tui::state::SettingsState<'_>,
    choice: crate::tui::components::scope_picker::ScopeChoice,
) -> SettingsModalOutcome {
    match settings_update::global_mount_scope_picker_commit_plan(choice) {
        GlobalMountScopePickerCommitPlan::ApplyAllAgentsScope => {
            commit_text(&mut settings.mounts, &GlobalMountTextTarget::AddScope, "")
        }
        GlobalMountScopePickerCommitPlan::OpenRolePicker => {
            open_global_mount_role_picker(settings);
            SettingsModalOutcome::Continue
        }
    }
}

pub(crate) fn open_global_mount_role_picker(settings: &mut crate::tui::state::SettingsState<'_>) {
    match settings_update::global_mount_role_picker_open_plan(&settings.trust.pending) {
        RolePickerOpenPlan::NoRoles => {
            settings
                .mounts
                .set_error(settings_no_registered_roles_error_message());
        }
        RolePickerOpenPlan::Open(roles) => {
            settings
                .mounts
                .open_sub_modal(SettingsModal::MountRolePicker {
                    state: RolePickerState::new(roles),
                });
        }
    }
}

pub(crate) fn text_modal(
    target: GlobalMountTextTarget,
    label: &str,
    initial: &str,
) -> SettingsModal<'static> {
    SettingsModal::MountText {
        target,
        state: Box::new(global_mount_text_input_state(label, initial)),
    }
}

pub(crate) fn text_modal_for_target(
    target: GlobalMountTextTarget,
    initial: &str,
) -> SettingsModal<'static> {
    let label = global_mount_text_target_label(&target).unwrap_or("Value");
    text_modal(target, label, initial)
}

pub(crate) fn env_text_modal(
    target: SettingsEnvTextTarget,
    label: impl Into<String>,
    initial: impl Into<String>,
) -> SettingsModal<'static> {
    let state = settings_env_text_input_state(&target, label, initial);
    SettingsModal::EnvText {
        target,
        pending_value: None,
        state: Box::new(state),
    }
}
