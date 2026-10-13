// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Settings env modal openers.

use crate::tui::screens::settings::update as settings_update;

use crate::tui::screens::settings::view::{
    settings_env_delete_confirm_state, settings_env_key_input_state,
    settings_env_new_key_text_plan, settings_env_scope_picker_state, settings_env_text_input_state,
    settings_env_value_edit_text_plan,
};

use crate::tui::state::{
    SettingsEnvConfirm, SettingsEnvEnterPlan, SettingsEnvOpPickerTarget, SettingsEnvScope,
    SettingsModal,
};

pub(crate) fn open_settings_env_enter_modal(settings: &mut crate::tui::state::SettingsState<'_>) {
    let plan = settings_update::settings_env_selected_enter_plan(
        &settings.env.pending,
        &settings.env.expanded,
        settings.env.selected,
    );
    match plan {
        SettingsEnvEnterPlan::EditValue { scope, key } => {
            let plan = settings_env_value_edit_text_plan(&settings.env.pending, scope, key);
            let state = settings_env_text_input_state(&plan.target, plan.label, plan.current);
            settings.env.modals.open(SettingsModal::EnvText {
                target: plan.target,
                pending_value: None,
                state: Box::new(state),
            });
        }
        SettingsEnvEnterPlan::OpenScopePicker => {
            settings.env.modals.open(SettingsModal::EnvScopePicker {
                state: settings_env_scope_picker_state(),
            });
        }
        SettingsEnvEnterPlan::ExpandRole(role) => {
            settings.env.expand_role(role);
        }
        SettingsEnvEnterPlan::AddRoleKey { scope } => {
            let plan = settings_env_new_key_text_plan(scope);
            let state =
                settings_env_key_input_state(&settings.env.pending, &plan.scope, plan.label, "");
            settings.env.modals.open(SettingsModal::EnvText {
                target: plan.target,
                pending_value: None,
                state: Box::new(state),
            });
        }
        SettingsEnvEnterPlan::Noop => {}
    }
}

pub(crate) fn open_settings_env_add_modal(settings: &mut crate::tui::state::SettingsState<'_>) {
    let Some(scope) = settings_update::settings_env_selected_add_target(
        &settings.env.pending,
        &settings.env.expanded,
        settings.env.selected,
    ) else {
        return;
    };
    let plan = settings_env_new_key_text_plan(scope);
    let state = settings_env_key_input_state(&settings.env.pending, &plan.scope, plan.label, "");
    settings.env.modals.open(SettingsModal::EnvText {
        target: plan.target,
        pending_value: None,
        state: Box::new(state),
    });
}

pub(crate) fn open_settings_env_delete_confirm(
    settings: &mut crate::tui::state::SettingsState<'_>,
) {
    let Some(key) = settings_update::settings_env_selected_delete_key(
        &settings.env.pending,
        &settings.env.expanded,
        settings.env.selected,
    ) else {
        return;
    };
    settings.env.modals.open(SettingsModal::EnvConfirm {
        action: SettingsEnvConfirm::Delete,
        state: settings_env_delete_confirm_state(&key),
    });
}

pub(crate) fn toggle_settings_env_mask(settings: &mut crate::tui::state::SettingsState<'_>) {
    settings_update::toggle_selected_settings_env_maskable_value(
        &mut settings.env.unmasked_rows,
        &settings.env.pending,
        &settings.env.expanded,
        settings.env.selected,
    );
}

pub(crate) fn open_settings_env_picker_modal(
    settings: &mut crate::tui::state::SettingsState<'_>,
    op_cache: std::rc::Rc<std::cell::RefCell<jackin_env::OpCache>>,
) {
    let Some(target) = settings_update::settings_env_selected_picker_target(
        &settings.env.pending,
        &settings.env.expanded,
        settings.env.selected,
    ) else {
        return;
    };
    let target = match target {
        (scope, Some(key)) => SettingsEnvOpPickerTarget::Existing { scope, key },
        (scope, None) => SettingsEnvOpPickerTarget::NewKey { scope },
    };
    settings.env.modals.open(SettingsModal::EnvOpPicker {
        target,
        state: Box::new(crate::tui::op_picker::OpPickerState::new_with_cache(
            op_cache,
        )),
    });
}

pub(crate) fn delete_selected_settings_env(env: &mut crate::tui::state::SettingsEnvState<'_>) {
    env.remove_selected_row();
}

pub(crate) fn set_settings_env_value_typed(
    env: &mut crate::tui::state::SettingsEnvState<'_>,
    scope: &SettingsEnvScope,
    key: &str,
    value: jackin_core::EnvValue,
) {
    env.set_value(scope, key, value);
}
