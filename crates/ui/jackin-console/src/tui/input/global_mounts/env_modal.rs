// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Settings env modal handling.

use super::{
    commit_env_text, commit_settings_env_scope_picker, commit_settings_env_source_picker,
    delete_selected_settings_env, set_settings_env_value_typed,
};
use crossterm::event::KeyEvent;

use crate::tui::screens::settings::update as settings_update;
use crate::tui::screens::settings::update::{
    SettingsEnvScopePickerSelection, SettingsEnvSourcePickerSelection,
};
use crate::tui::screens::settings::view::{
    settings_env_key_input_state, settings_env_new_key_after_picker_text_plan,
    settings_env_new_key_text_plan,
};

use crate::tui::state::{SettingsEnvConfirm, SettingsEnvOpPickerTarget, SettingsModal};
use crate::tui::update::{
    BoolConfirmModalPlan, InlinePickerPlan, ScopePickerPlan, SourcePickerPlan,
    bool_confirm_modal_plan, inline_picker_plan, scope_picker_plan, source_picker_plan,
};

pub fn handle_settings_env_modal(
    env: &mut crate::tui::state::SettingsEnvState<'_>,
    key: KeyEvent,
    op_cache: std::rc::Rc<std::cell::RefCell<jackin_env::OpCache>>,
) {
    let Some(modal) = env.modals.take_current() else {
        return;
    };
    match modal {
        SettingsModal::EnvText {
            target,
            pending_value,
            mut state,
        } => match inline_picker_plan(state.handle_key(key.into())) {
            InlinePickerPlan::Commit(value) => {
                let committed_target = target.clone();
                env.modals.open(SettingsModal::EnvText {
                    target,
                    pending_value: pending_value.clone(),
                    state,
                });
                commit_env_text(env, &committed_target, pending_value, &value);
            }
            InlinePickerPlan::Dismiss => {
                env.pop_modal_chain();
            }
            InlinePickerPlan::Continue => {
                env.modals.open(SettingsModal::EnvText {
                    target,
                    pending_value,
                    state,
                });
            }
        },
        SettingsModal::EnvSourcePicker {
            key: env_key,
            state: mut source,
        } => match source_picker_plan(source.handle_key(key)) {
            SourcePickerPlan::Plain => {
                commit_settings_env_source_picker(
                    env,
                    SettingsEnvSourcePickerSelection::Plain,
                    env_key,
                    source,
                    op_cache,
                );
            }
            SourcePickerPlan::Op => {
                commit_settings_env_source_picker(
                    env,
                    SettingsEnvSourcePickerSelection::Op,
                    env_key,
                    source,
                    op_cache,
                );
            }
            SourcePickerPlan::Dismiss => {
                env.pop_modal_chain();
            }
            SourcePickerPlan::Continue => {
                env.modals.open(SettingsModal::EnvSourcePicker {
                    key: env_key,
                    state: source,
                });
            }
        },
        SettingsModal::EnvOpPicker {
            target,
            state: mut picker,
        } => {
            match crate::tui::update::op_picker_inline_plan(picker.handle_key(key)) {
                // Browse-mode caller: only `Existing` is reachable.
                InlinePickerPlan::Commit(
                    crate::tui::op_picker::OpPickerSelection::NewItem { .. }
                    | crate::tui::op_picker::OpPickerSelection::EditItemField { .. },
                ) => unreachable!("settings-env OpPicker runs in Browse mode"),
                InlinePickerPlan::Commit(crate::tui::op_picker::OpPickerSelection::Existing(
                    op_ref,
                )) => match target {
                    SettingsEnvOpPickerTarget::Existing { scope, key } => {
                        set_settings_env_value_typed(
                            env,
                            &scope,
                            &key,
                            jackin_core::EnvValue::OpRef(op_ref),
                        );
                        env.clear_modal_chain();
                    }
                    SettingsEnvOpPickerTarget::NewKey { scope } => {
                        let plan = settings_env_new_key_after_picker_text_plan(scope);
                        let state =
                            settings_env_key_input_state(&env.pending, &plan.scope, plan.label, "");
                        env.modals.open(SettingsModal::EnvOpPicker {
                            target: SettingsEnvOpPickerTarget::NewKey {
                                scope: plan.scope.clone(),
                            },
                            state: picker,
                        });
                        env.open_sub_modal(SettingsModal::EnvText {
                            target: plan.target,
                            pending_value: Some(jackin_core::EnvValue::OpRef(op_ref)),
                            state: Box::new(state),
                        });
                    }
                },
                InlinePickerPlan::Dismiss => {
                    env.pop_modal_chain();
                }
                InlinePickerPlan::Continue => {
                    env.modals.open(SettingsModal::EnvOpPicker {
                        target,
                        state: picker,
                    });
                }
            }
        }
        SettingsModal::EnvRolePicker { state: mut picker } => {
            match inline_picker_plan(picker.handle_key(key)) {
                InlinePickerPlan::Commit(role) => {
                    let plan = settings_update::settings_env_role_picker_commit_plan(&role);
                    let text_plan = settings_env_new_key_text_plan(plan.scope);
                    let state = settings_env_key_input_state(
                        &env.pending,
                        &text_plan.scope,
                        text_plan.label,
                        "",
                    );
                    env.modals
                        .open(SettingsModal::EnvRolePicker { state: picker });
                    env.open_sub_modal(SettingsModal::EnvText {
                        target: text_plan.target,
                        pending_value: None,
                        state: Box::new(state),
                    });
                }
                InlinePickerPlan::Dismiss => {
                    env.pop_modal_chain();
                }
                InlinePickerPlan::Continue => {
                    env.modals
                        .open(SettingsModal::EnvRolePicker { state: picker });
                }
            }
        }
        SettingsModal::EnvScopePicker { mut state } => {
            match scope_picker_plan(state.handle_key(key)) {
                ScopePickerPlan::AllAgents => {
                    commit_settings_env_scope_picker(
                        env,
                        SettingsEnvScopePickerSelection::AllAgents,
                    );
                }
                ScopePickerPlan::SpecificAgent => {
                    commit_settings_env_scope_picker(
                        env,
                        SettingsEnvScopePickerSelection::SpecificAgent,
                    );
                }
                ScopePickerPlan::Dismiss => {
                    env.pop_modal_chain();
                }
                ScopePickerPlan::Continue => {
                    env.modals.open(SettingsModal::EnvScopePicker { state });
                }
            }
        }
        SettingsModal::EnvConfirm { action, mut state } => {
            match bool_confirm_modal_plan(state.handle_key(key.into())) {
                BoolConfirmModalPlan::Confirm => match action {
                    SettingsEnvConfirm::Delete => {
                        delete_selected_settings_env(env);
                        env.clear_modal_chain();
                    }
                },
                BoolConfirmModalPlan::Dismiss => env.clear_modal_chain(),
                BoolConfirmModalPlan::Continue => {
                    env.modals.open(SettingsModal::EnvConfirm { action, state });
                }
            }
        }
        _ => unreachable!("env input handler received a non-env settings modal"),
    }
}
