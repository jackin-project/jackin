// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Settings text and picker commits.

use super::{
    SettingsModalOutcome, apply_global_mount_add_text, env_text_modal, set_settings_env_value_typed,
};

use crate::tui::screens::settings::update as settings_update;
use crate::tui::screens::settings::update::{
    GlobalMountEditTextApplyPlan, GlobalMountTextCommitPlan, RolePickerOpenPlan,
    SettingsEnvScopePickerCommitPlan, SettingsEnvScopePickerSelection,
    SettingsEnvSourcePickerCommitPlan, SettingsEnvSourcePickerSelection, SettingsEnvTextCommitPlan,
};
use crate::tui::screens::settings::view::{
    global_mount_gone_message, global_mount_name_empty_message,
    settings_env_empty_key_error_message, settings_env_empty_key_text_plan,
    settings_env_key_input_state, settings_env_new_key_text_plan,
    settings_env_plain_value_text_plan, settings_env_source_picker_state,
    settings_no_registered_roles_error_message,
};

use crate::tui::state::{
    GlobalMountConfirm, GlobalMountTextTarget, SettingsEnvOpPickerTarget, SettingsEnvScope,
    SettingsEnvTextTarget, SettingsModal,
};

pub(crate) fn commit_settings_confirm(
    settings: &mut crate::tui::state::SettingsState<'_>,
    action: GlobalMountConfirm,
) -> SettingsModalOutcome {
    let plan = settings_update::settings_confirm_commit_plan(
        action,
        settings.mounts.selected,
        settings.mounts.pending.len(),
    );
    match plan {
        settings_update::SettingsConfirmCommitPlan::Remove {
            remove_index,
            selected,
        } => {
            settings
                .mounts
                .remove_row_and_select(remove_index, selected);
            SettingsModalOutcome::Continue
        }
        settings_update::SettingsConfirmCommitPlan::Save => request_settings_save(settings),
        settings_update::SettingsConfirmCommitPlan::OpenSavePreview => {
            open_settings_save_preview(settings);
            SettingsModalOutcome::Continue
        }
        settings_update::SettingsConfirmCommitPlan::DiscardAll => {
            settings.discard_all();
            settings.mounts.request_exit();
            SettingsModalOutcome::Continue
        }
        settings_update::SettingsConfirmCommitPlan::Noop => SettingsModalOutcome::Continue,
    }
}

pub(crate) fn request_settings_save(
    _settings: &mut crate::tui::state::SettingsState<'_>,
) -> SettingsModalOutcome {
    SettingsModalOutcome::SaveSettings
}

pub(crate) fn open_settings_save_preview(settings: &mut crate::tui::state::SettingsState<'_>) {
    let lines = super::super::save::build_settings_save_lines(settings);
    settings
        .mounts
        .modals
        .open(SettingsModal::MountPreviewSave {
            state: crate::tui::components::confirm_save::ConfirmSaveState::new(lines),
        });
}

pub(crate) fn commit_text(
    global: &mut crate::tui::state::GlobalMountsState<'_>,
    target: &GlobalMountTextTarget,
    value: &str,
) -> SettingsModalOutcome {
    match settings_update::global_mount_text_commit_plan(target, value) {
        plan @ (GlobalMountTextCommitPlan::AddScope(_)
        | GlobalMountTextCommitPlan::AddName(_)
        | GlobalMountTextCommitPlan::AddSource(_)
        | GlobalMountTextCommitPlan::AddDestination(_)) => {
            return apply_global_mount_add_text(global, plan);
        }
        plan => match settings_update::global_mount_edit_text_apply_plan(
            &mut global.pending,
            global.selected,
            plan,
        ) {
            GlobalMountEditTextApplyPlan::MissingRow => {
                global.set_error(global_mount_gone_message());
                return SettingsModalOutcome::Continue;
            }
            GlobalMountEditTextApplyPlan::EmptyName => {
                global.set_error(global_mount_name_empty_message());
                return SettingsModalOutcome::Continue;
            }
            GlobalMountEditTextApplyPlan::Applied => {
                global.clear_modal_chain();
            }
            GlobalMountEditTextApplyPlan::Noop => {}
        },
    }
    SettingsModalOutcome::Continue
}

pub(crate) fn commit_env_text(
    env: &mut crate::tui::state::SettingsEnvState<'_>,
    target: &SettingsEnvTextTarget,
    pending_value: Option<jackin_core::EnvValue>,
    value: &str,
) {
    match settings_update::settings_env_text_commit_plan(target, value, pending_value.is_some()) {
        SettingsEnvTextCommitPlan::EmptyKey { scope } => {
            env.set_error(settings_env_empty_key_error_message());
            let plan = settings_env_empty_key_text_plan(scope);
            let state = settings_env_key_input_state(&env.pending, &plan.scope, plan.label, "");
            env.modals.open(SettingsModal::EnvText {
                target: plan.target,
                pending_value,
                state: Box::new(state),
            });
        }
        SettingsEnvTextCommitPlan::SetCarriedPickerValue { scope, key } => {
            if let Some(stashed) = pending_value {
                set_settings_env_value_typed(env, &scope, &key, stashed);
                env.clear_modal_chain();
            }
        }
        SettingsEnvTextCommitPlan::OpenSourcePicker { scope, key } => {
            env.open_sub_modal(SettingsModal::EnvSourcePicker {
                key: (scope, key.clone()),
                state: settings_env_source_picker_state(key),
            });
        }
        SettingsEnvTextCommitPlan::SetPlainValue { scope, key, value } => {
            set_settings_env_value_typed(env, &scope, &key, jackin_core::EnvValue::Plain(value));
            env.clear_modal_chain();
        }
    }
}

pub(crate) fn commit_settings_env_source_picker(
    env: &mut crate::tui::state::SettingsEnvState<'_>,
    selection: SettingsEnvSourcePickerSelection,
    key: (SettingsEnvScope, String),
    source: crate::tui::components::source_picker::SourcePickerState,
    op_cache: std::rc::Rc<std::cell::RefCell<jackin_env::OpCache>>,
) {
    match settings_update::settings_env_source_picker_commit_plan(selection, &key) {
        SettingsEnvSourcePickerCommitPlan::OpenPlainText { scope, key } => {
            env.modals.open(SettingsModal::EnvSourcePicker {
                key: (scope.clone(), key.clone()),
                state: source,
            });
            let plan = settings_env_plain_value_text_plan(scope, key);
            env.open_sub_modal(env_text_modal(plan.target, plan.label, plan.current));
        }
        SettingsEnvSourcePickerCommitPlan::OpenOpPicker { scope, key } => {
            env.modals.open(SettingsModal::EnvSourcePicker {
                key: (scope.clone(), key.clone()),
                state: source,
            });
            env.open_sub_modal(SettingsModal::EnvOpPicker {
                target: SettingsEnvOpPickerTarget::Existing { scope, key },
                state: Box::new(crate::tui::op_picker::OpPickerState::new_with_cache(
                    op_cache,
                )),
            });
        }
    }
}

pub(crate) fn commit_settings_env_scope_picker(
    env: &mut crate::tui::state::SettingsEnvState<'_>,
    selection: SettingsEnvScopePickerSelection,
) {
    match settings_update::settings_env_scope_picker_commit_plan(selection) {
        SettingsEnvScopePickerCommitPlan::OpenGlobalKeyInput { scope } => {
            let plan = settings_env_new_key_text_plan(scope);
            let input_state =
                settings_env_key_input_state(&env.pending, &plan.scope, plan.label, "");
            // Don't stash the just-committed ScopePicker as
            // the Text modal's parent — Esc on Text would
            // pop back into a consumed picker. Start the
            // child modal with an empty parent chain.
            env.open_sub_modal(SettingsModal::EnvText {
                target: plan.target,
                pending_value: None,
                state: Box::new(input_state),
            });
        }
        SettingsEnvScopePickerCommitPlan::OpenRolePicker => {
            open_settings_env_role_picker(env);
        }
    }
}

pub(crate) fn open_settings_env_role_picker(env: &mut crate::tui::state::SettingsEnvState<'_>) {
    use crate::tui::state::RolePickerState;

    match settings_update::settings_env_role_picker_open_plan(&env.pending) {
        RolePickerOpenPlan::NoRoles => {
            env.set_error(settings_no_registered_roles_error_message());
        }
        RolePickerOpenPlan::Open(roles) => {
            env.open_sub_modal(SettingsModal::EnvRolePicker {
                state: RolePickerState::new(roles),
            });
        }
    }
}
