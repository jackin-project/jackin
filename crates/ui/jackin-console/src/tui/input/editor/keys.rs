// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Editor per-tab key dispatchers.

use super::super::InputOutcome;
use super::{
    agents, dispatch_editor_field_selection, dispatch_editor_horizontal_scroll,
    dispatch_editor_immediate_action, dispatch_editor_navigation,
    dispatch_editor_role_header_expansion, dispatch_editor_top_level, dispatch_manager, general,
    open_secrets_picker_modal, secrets,
};
use crate::tui::components::error_popup::no_github_url_error_popup_state;
use crossterm::event::{KeyCode, KeyEvent};

use crate::tui::components::save_discard::editor_exit_save_discard_state;

use crate::tui::screens::editor::model::{
    AuthEnterPlan, EditorAuthActionKeyPlan, EditorEnterKeyPlan, EditorEscapeKeyPlan,
    EditorImmediateActionKeyPlan, EditorMountActionKeyPlan, EditorMountGithubOpenPlan,
    EditorRoleActionKeyPlan, EditorSaveKeyPlan, EditorSecretsActionKeyPlan, EditorTabActionKeyPlan,
    EditorTopLevelKeyPlan,
};

use crate::tui::state::ManagerEffect;
use crate::tui::state::update::{ManagerMessage, update_manager};
use crate::tui::state::{EditorState, ManagerStage, ManagerState};

use jackin_config::AppConfig;
use jackin_core::JackinPaths;

// Central keymap dispatch — table-like layout makes the keymap
// readable at a glance; extracting per-key helpers just scatters it.
pub fn handle_editor_key(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    paths: &JackinPaths,
    cwd: &std::path::Path,
    key: KeyEvent,
) -> anyhow::Result<InputOutcome> {
    if let ManagerStage::Editor(editor) = &mut state.stage
        && editor.active_tab == crate::tui::state::EditorTab::Auth
        && !editor.tab_bar_focused()
        && editor.focused_account_row(config)
        && matches!(key.code, KeyCode::Enter | KeyCode::Char(' ' | 'd' | 'D'))
    {
        editor.edit_account_row(config, matches!(key.code, KeyCode::Char('d' | 'D')));
        return Ok(InputOutcome::Continue);
    }
    // Capture before the editor borrow (separate fields, but explicit is cleaner).
    let op_cache = std::rc::Rc::clone(&state.op_cache);
    let op_available = state.op_available;
    let term_width = state.cached_term_size.width;
    let term_size = state.cached_term_size;

    let top_level_plan = match &state.stage {
        ManagerStage::Editor(editor) => dispatch_editor_top_level(key, editor.tab_bar_focused()),
        _ => EditorTopLevelKeyPlan::ContinueToTabActions,
    };
    match top_level_plan {
        EditorTopLevelKeyPlan::Save => {
            if let Some(plan) = match &state.stage {
                ManagerStage::Editor(editor) => Some(editor.save_key_plan()),
                _ => None,
            } {
                dispatch_editor_save(state, config, plan)?;
            }
            // `paths` is consumed by the commit path in
            // handle_editor_modal, not here.
            let _unused = paths;
            return Ok(InputOutcome::Continue);
        }
        EditorTopLevelKeyPlan::Escape => {
            if let Some(plan) = match &state.stage {
                ManagerStage::Editor(editor) => Some(editor.escape_key_plan()),
                _ => None,
            } {
                dispatch_editor_escape(state, config, cwd, plan);
            }
            return Ok(InputOutcome::Continue);
        }
        EditorTopLevelKeyPlan::Navigation(plan) => {
            dispatch_editor_navigation(state, plan);
            return Ok(InputOutcome::Continue);
        }
        EditorTopLevelKeyPlan::ScrollHorizontal { delta } => {
            if let Some(plan) = match &state.stage {
                ManagerStage::Editor(editor) => Some(editor.horizontal_scroll_key_plan(delta)),
                _ => None,
            } {
                dispatch_editor_horizontal_scroll(state, plan, term_width);
            }
            return Ok(InputOutcome::Continue);
        }
        EditorTopLevelKeyPlan::MoveField { delta } => {
            if let Some(plan) = match &state.stage {
                ManagerStage::Editor(editor) => {
                    Some(editor.field_selection_key_plan(config, delta, term_size))
                }
                _ => None,
            } {
                dispatch_editor_field_selection(state, plan);
            }
            return Ok(InputOutcome::Continue);
        }
        EditorTopLevelKeyPlan::SetRoleHeaderExpanded { expanded } => {
            if let Some(plan) = match &state.stage {
                ManagerStage::Editor(editor) => {
                    Some(editor.focused_role_header_expansion_key_plan(config, expanded))
                }
                _ => None,
            } {
                dispatch_editor_role_header_expansion(state, plan);
            }
            return Ok(InputOutcome::Continue);
        }
        EditorTopLevelKeyPlan::CheckImmediateAction => {
            let plan = match &state.stage {
                ManagerStage::Editor(editor) => {
                    editor.immediate_action_key_plan(config, key.code, key.modifiers)
                }
                _ => EditorImmediateActionKeyPlan::NotImmediateAction,
            };
            if dispatch_editor_immediate_action(state, plan) {
                return Ok(InputOutcome::Continue);
            }
        }
        EditorTopLevelKeyPlan::ContinueToTabActions => {}
    }

    let ManagerStage::Editor(editor) = &mut state.stage else {
        return Ok(InputOutcome::Continue);
    };

    match editor.tab_action_key_plan(config, key.code, key.modifiers, op_available) {
        EditorTabActionKeyPlan::Role(role_action_plan) => {
            dispatch_editor_role_action(editor, config, role_action_plan);
        }
        EditorTabActionKeyPlan::Mount(mount_action_plan) => {
            if let Some(effect) = dispatch_editor_mount_action(editor, mount_action_plan) {
                state.request_effect(effect);
            }
        }
        EditorTabActionKeyPlan::Secrets(secrets_action_plan) => {
            dispatch_editor_secrets_action(editor, op_cache, secrets_action_plan);
        }
        EditorTabActionKeyPlan::Auth(auth_action_plan) => {
            dispatch_editor_auth_action(editor, config, auth_action_plan);
        }
        EditorTabActionKeyPlan::Enter(enter_plan) => {
            if let Some(effect) = dispatch_editor_enter_key(editor, config, op_cache, enter_plan) {
                state.request_effect(effect);
            }
        }
        EditorTabActionKeyPlan::Noop => {}
    }
    Ok(InputOutcome::Continue)
}

pub(crate) fn dispatch_editor_save(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    plan: EditorSaveKeyPlan,
) -> anyhow::Result<()> {
    match plan {
        EditorSaveKeyPlan::BeginSave => super::super::save::begin_editor_save(state, config, true),
        EditorSaveKeyPlan::Noop => Ok(()),
    }
}

pub(crate) fn dispatch_editor_escape(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
    plan: EditorEscapeKeyPlan,
) {
    match plan {
        EditorEscapeKeyPlan::FocusTabBar => {
            dispatch_manager(state, ManagerMessage::FocusEditorTabBar);
        }
        EditorEscapeKeyPlan::OpenSaveDiscard => {
            if let ManagerStage::Editor(editor) = &mut state.stage {
                editor.open_save_discard_cancel(editor_exit_save_discard_state());
            }
        }
        EditorEscapeKeyPlan::ReloadFromConfig => {
            update_manager(
                state,
                ManagerMessage::ReloadFromConfig {
                    config: Box::new(config.clone()),
                    cwd: cwd.to_path_buf(),
                },
            );
        }
    }
}

pub(crate) fn dispatch_editor_enter_key(
    editor: &mut EditorState<'_>,
    config: &AppConfig,
    op_cache: std::rc::Rc<std::cell::RefCell<jackin_env::OpCache>>,
    plan: EditorEnterKeyPlan,
) -> Option<ManagerEffect> {
    match plan {
        EditorEnterKeyPlan::OpenGeneralField => {
            general::open_editor_field_modal(editor);
            None
        }
        EditorEnterKeyPlan::OpenMountFileBrowser => {
            Some(ManagerEffect::OpenEditorAddMountFileBrowser)
        }
        EditorEnterKeyPlan::OpenSecretsPicker => {
            open_secrets_picker_modal(editor, op_cache);
            None
        }
        EditorEnterKeyPlan::OpenSecretsEnterModal => {
            secrets::open_secrets_enter_modal(editor);
            None
        }
        EditorEnterKeyPlan::OpenRoleInput => {
            agents::open_role_input(editor, config);
            None
        }
        EditorEnterKeyPlan::Auth(AuthEnterPlan::OpenForm) => {
            super::super::auth::open_auth_form_modal(editor, config);
            None
        }
        EditorEnterKeyPlan::Auth(AuthEnterPlan::Noop) | EditorEnterKeyPlan::Noop => None,
    }
}

pub(crate) fn dispatch_editor_auth_action(
    editor: &mut EditorState<'_>,
    config: &AppConfig,
    plan: EditorAuthActionKeyPlan,
) {
    match plan {
        EditorAuthActionKeyPlan::ClearFocusedRow => {
            super::super::auth::handle_d_on_auth_row(editor, config);
        }
        EditorAuthActionKeyPlan::NotAuthAction => {}
    }
}

pub(crate) fn dispatch_editor_secrets_action(
    editor: &mut EditorState<'_>,
    op_cache: std::rc::Rc<std::cell::RefCell<jackin_env::OpCache>>,
    plan: EditorSecretsActionKeyPlan,
) {
    match plan {
        EditorSecretsActionKeyPlan::OpenPicker => {
            open_secrets_picker_modal(editor, op_cache);
        }
        EditorSecretsActionKeyPlan::OpenDeleteConfirm => {
            secrets::open_secrets_delete_confirm(editor);
        }
        EditorSecretsActionKeyPlan::OpenAddModal => {
            secrets::open_secrets_add_modal(editor);
        }
        EditorSecretsActionKeyPlan::NotSecretsAction => {}
    }
}

pub(crate) fn dispatch_editor_mount_action(
    editor: &mut EditorState<'_>,
    plan: EditorMountActionKeyPlan,
) -> Option<ManagerEffect> {
    match plan {
        EditorMountActionKeyPlan::AddMount => Some(ManagerEffect::OpenEditorAddMountFileBrowser),
        EditorMountActionKeyPlan::RemoveSelectedMount => {
            editor.remove_selected_mount();
            None
        }
        EditorMountActionKeyPlan::CycleIsolation => {
            editor.cycle_isolation_for_selected_mount();
            None
        }
        EditorMountActionKeyPlan::OpenGithub => match editor.focused_mount_github_open_plan() {
            EditorMountGithubOpenPlan::Open(web_url) => Some(ManagerEffect::OpenUrl(web_url)),
            EditorMountGithubOpenPlan::NoGithubUrl => {
                editor.open_error_popup(no_github_url_error_popup_state());
                None
            }
            EditorMountGithubOpenPlan::NoSelection => None,
        },
        EditorMountActionKeyPlan::NotMountAction => None,
    }
}

pub(crate) fn dispatch_editor_role_action(
    editor: &mut EditorState<'_>,
    config: &AppConfig,
    plan: EditorRoleActionKeyPlan,
) {
    match plan {
        EditorRoleActionKeyPlan::OpenRoleInput => {
            agents::open_role_input(editor, config);
        }
        EditorRoleActionKeyPlan::ToggleAllowed => {
            agents::toggle_agent_allowed_at_cursor(editor, config);
        }
        EditorRoleActionKeyPlan::ToggleDefault => {
            agents::toggle_default_agent_at_cursor(editor, config);
        }
        EditorRoleActionKeyPlan::NotRoleAction => {}
    }
}
