// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Editor-stage dispatch: tab navigation, field focus, per-tab key
//! handling, and the editor-level modal dispatcher.

pub(super) mod agents;
pub(super) mod general;
pub(super) mod modal;
pub(super) mod secrets;
#[cfg(test)]
mod tests;
pub use modal::{
    apply_text_input_to_pending, env_key_input_state, handle_token_generate_pick,
    open_create_op_picker_for_generate, open_secrets_picker_modal, set_pending_env_op_ref,
    start_plain_token_generate,
};

use crossterm::event::KeyEvent;

use super::InputOutcome;
use crate::tui::components::error_popup::no_github_url_error_popup_state;
use crate::tui::components::file_browser::page_rows_for_modal;
use crate::tui::components::save_discard::editor_exit_save_discard_state;
use crate::tui::op_picker::OpPickerState;
use crate::tui::screens::editor::model::{
    AuthEnterPlan, EditorAuthActionKeyPlan, EditorEnterKeyPlan, EditorEscapeKeyPlan,
    EditorFieldSelectionKeyPlan, EditorHorizontalScrollKeyPlan, EditorImmediateActionKeyPlan,
    EditorMountActionKeyPlan, EditorMountGithubOpenPlan, EditorNavigationKeyPlan,
    EditorRoleActionKeyPlan, EditorRoleHeaderExpansionKeyPlan, EditorSaveKeyPlan,
    EditorSecretsActionKeyPlan, EditorTabActionKeyPlan, EditorTopLevelKeyPlan,
    RoleHeaderExpansionPlan,
};
use crate::tui::screens::editor::view::{
    mount_destination_input_state, mount_dst_choice_state, secret_new_key_after_picker_label,
    secret_new_key_label, secret_new_value_input_state,
};
use crate::tui::state::ManagerEffect;
use crate::tui::state::update::{ManagerMessage, update_manager};
use crate::tui::state::{
    ConfirmTarget, EditorSaveFlow, EditorState, ExitIntent, FileBrowserTarget, ManagerStage,
    ManagerState, Modal, SecretsPickerTarget, SecretsScopeTag, TextInputTarget,
    open_editor_action_error, open_role_input_error,
};
use crate::tui::update::{
    BoolConfirmModalPlan, ConfirmSaveModalPlan, DismissibleModalPlan, FileBrowserModalPlan,
    InlinePickerPlan, MountDstChoicePlan, SaveDiscardModalPlan, ScopePickerPlan, SourcePickerPlan,
    bool_confirm_modal_plan, confirm_save_modal_plan, dismissible_modal_plan,
    file_browser_modal_plan, inline_picker_plan, mount_dst_choice_plan, save_discard_modal_plan,
    scope_picker_plan, source_picker_plan,
};
use jackin_config::AppConfig;
use jackin_core::JackinPaths;
use jackin_tui::components::KeyChord;

use crate::tui::keymap::{
    EDITOR_CONTENT_KEYMAP, EDITOR_GLOBAL_KEYMAP, EDITOR_TAB_BAR_KEYMAP, EditorContentAction,
    EditorGlobalAction, EditorTabBarAction,
};

fn dispatch_editor_top_level(key: KeyEvent, tab_bar_focused: bool) -> EditorTopLevelKeyPlan {
    use crossterm::event::KeyCode;

    let chord = KeyChord::from(key);

    if let Some(action) = EDITOR_GLOBAL_KEYMAP.dispatch(chord) {
        return match action {
            EditorGlobalAction::Save => EditorTopLevelKeyPlan::Save,
            EditorGlobalAction::Escape => EditorTopLevelKeyPlan::Escape,
        };
    }

    // Tab-bar navigation keys (Left/BackTab, Right, Tab/Down/j/J) are intercepted when
    // the tab bar has focus. Other keys (Enter, h/H/l/L, Up/k/K, etc.) fall through to
    // the content keymap even when the tab bar is focused — matching the original
    // `editor_top_level_key_plan` behavior where these guards were not exhaustive.
    if tab_bar_focused && let Some(action) = EDITOR_TAB_BAR_KEYMAP.dispatch(chord) {
        return match action {
            EditorTabBarAction::PrevTab => {
                EditorTopLevelKeyPlan::Navigation(EditorNavigationKeyPlan::MoveTab {
                    delta: -1,
                    focus_tab_bar: true,
                })
            }
            EditorTabBarAction::NextTab => {
                EditorTopLevelKeyPlan::Navigation(EditorNavigationKeyPlan::MoveTab {
                    delta: 1,
                    focus_tab_bar: true,
                })
            }
            EditorTabBarAction::FocusContent => {
                EditorTopLevelKeyPlan::Navigation(EditorNavigationKeyPlan::FocusContent)
            }
        };
    }

    // Content-mode (and tab-bar fall-through): Char(_) wildcard falls through.
    match EDITOR_CONTENT_KEYMAP.dispatch(chord) {
        Some(EditorContentAction::MoveUp) => EditorTopLevelKeyPlan::MoveField { delta: -1 },
        Some(EditorContentAction::MoveDown) => EditorTopLevelKeyPlan::MoveField { delta: 1 },
        Some(EditorContentAction::ScrollLeft) => {
            EditorTopLevelKeyPlan::ScrollHorizontal { delta: -8 }
        }
        Some(EditorContentAction::ScrollRight) => {
            EditorTopLevelKeyPlan::ScrollHorizontal { delta: 8 }
        }
        Some(EditorContentAction::CollapseHeader) => {
            EditorTopLevelKeyPlan::SetRoleHeaderExpanded { expanded: false }
        }
        Some(EditorContentAction::ExpandHeader) => {
            EditorTopLevelKeyPlan::SetRoleHeaderExpanded { expanded: true }
        }
        Some(EditorContentAction::NextTab) => {
            EditorTopLevelKeyPlan::Navigation(EditorNavigationKeyPlan::MoveTab {
                delta: 1,
                focus_tab_bar: true,
            })
        }
        Some(EditorContentAction::FocusTabBar) => {
            EditorTopLevelKeyPlan::Navigation(EditorNavigationKeyPlan::FocusTabBar)
        }
        Some(EditorContentAction::CheckImmediate) => EditorTopLevelKeyPlan::CheckImmediateAction,
        None => {
            // Char(_) wildcard: any printable character triggers immediate-action check.
            if matches!(key.code, KeyCode::Char(_)) {
                EditorTopLevelKeyPlan::CheckImmediateAction
            } else {
                EditorTopLevelKeyPlan::ContinueToTabActions
            }
        }
    }
}

// Central keymap dispatch — table-like layout makes the keymap
// readable at a glance; extracting per-key helpers just scatters it.
pub fn handle_editor_key(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    paths: &JackinPaths,
    cwd: &std::path::Path,
    key: KeyEvent,
) -> anyhow::Result<InputOutcome> {
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

fn dispatch_editor_save(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    plan: EditorSaveKeyPlan,
) -> anyhow::Result<()> {
    match plan {
        EditorSaveKeyPlan::BeginSave => super::save::begin_editor_save(state, config, true),
        EditorSaveKeyPlan::Noop => Ok(()),
    }
}

fn dispatch_editor_escape(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
    plan: EditorEscapeKeyPlan,
) {
    match plan {
        EditorEscapeKeyPlan::FocusTabBar => {
            dispatch_manager(state, ManagerMessage::FocusEditorTabBar);
        }
        EditorEscapeKeyPlan::FocusTabBarAndClearAuthKind => {
            dispatch_manager(state, ManagerMessage::ClearEditorAuthKind);
            dispatch_manager(state, ManagerMessage::FocusEditorTabBar);
        }
        EditorEscapeKeyPlan::ClearAuthKind => {
            dispatch_manager(state, ManagerMessage::ClearEditorAuthKind);
        }
        EditorEscapeKeyPlan::OpenSaveDiscard => {
            if let ManagerStage::Editor(editor) = &mut state.stage {
                editor.open_save_discard_cancel(editor_exit_save_discard_state());
            }
        }
        EditorEscapeKeyPlan::ReloadFromConfig => {
            let _unused = update_manager(
                state,
                ManagerMessage::ReloadFromConfig {
                    config: Box::new(config.clone()),
                    cwd: cwd.to_path_buf(),
                },
            );
        }
    }
}

fn dispatch_editor_enter_key(
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
        EditorEnterKeyPlan::Auth(AuthEnterPlan::AddRoleOverride) => {
            super::auth::open_auth_role_picker(editor, config);
            None
        }
        EditorEnterKeyPlan::Auth(AuthEnterPlan::ToggleRole(role)) => {
            super::auth::toggle_role_expand(editor, role);
            None
        }
        EditorEnterKeyPlan::Auth(AuthEnterPlan::OpenForm) => {
            super::auth::open_auth_form_modal(editor, config);
            None
        }
        EditorEnterKeyPlan::Auth(AuthEnterPlan::Noop) | EditorEnterKeyPlan::Noop => None,
    }
}

fn dispatch_editor_auth_action(
    editor: &mut EditorState<'_>,
    config: &AppConfig,
    plan: EditorAuthActionKeyPlan,
) {
    match plan {
        EditorAuthActionKeyPlan::OpenRolePicker => {
            super::auth::open_auth_role_picker(editor, config);
        }
        EditorAuthActionKeyPlan::ClearFocusedRow => {
            super::auth::handle_d_on_auth_row(editor, config);
        }
        EditorAuthActionKeyPlan::NotAuthAction => {}
    }
}

fn dispatch_editor_secrets_action(
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

fn dispatch_editor_mount_action(
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

fn dispatch_editor_role_action(
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

fn dispatch_manager(state: &mut ManagerState<'_>, message: ManagerMessage) {
    let _dirty = update_manager(state, message);
}

fn dispatch_editor_horizontal_scroll(
    state: &mut ManagerState<'_>,
    plan: EditorHorizontalScrollKeyPlan,
    term_width: u16,
) {
    match plan {
        EditorHorizontalScrollKeyPlan::WorkspaceMounts {
            delta,
            content_width,
        } => dispatch_manager(
            state,
            ManagerMessage::ScrollEditorWorkspaceMountsHorizontal {
                delta,
                term_width,
                content_width,
            },
        ),
        EditorHorizontalScrollKeyPlan::TabContent {
            delta,
            content_width,
        } => dispatch_manager(
            state,
            ManagerMessage::ScrollEditorTabHorizontal {
                delta,
                term_width,
                content_width,
            },
        ),
    }
}

fn dispatch_editor_field_selection(
    state: &mut ManagerState<'_>,
    plan: EditorFieldSelectionKeyPlan,
) {
    dispatch_manager(
        state,
        ManagerMessage::MoveEditorFieldSelection {
            delta: plan.delta,
            max_row: plan.max_row,
            skipped_rows: plan.skipped_rows,
            term: plan.term,
            footer_h: plan.footer_h,
        },
    );
}

fn dispatch_editor_navigation(state: &mut ManagerState<'_>, plan: EditorNavigationKeyPlan) -> bool {
    match plan {
        EditorNavigationKeyPlan::MoveTab {
            delta,
            focus_tab_bar,
        } => {
            dispatch_manager(
                state,
                ManagerMessage::MoveEditorTab {
                    delta,
                    focus_tab_bar,
                },
            );
            true
        }
        EditorNavigationKeyPlan::FocusContent => {
            dispatch_manager(state, ManagerMessage::FocusEditorContent);
            true
        }
        EditorNavigationKeyPlan::FocusTabBar => {
            dispatch_manager(state, ManagerMessage::FocusEditorTabBar);
            true
        }
        EditorNavigationKeyPlan::NotNavigation => false,
    }
}

fn dispatch_editor_immediate_action(
    state: &mut ManagerState<'_>,
    plan: EditorImmediateActionKeyPlan,
) -> bool {
    match plan {
        EditorImmediateActionKeyPlan::EnterAuthKind(kind) => {
            dispatch_manager(state, ManagerMessage::EnterEditorAuthKind { kind });
            true
        }
        EditorImmediateActionKeyPlan::ToggleGeneralSelected => {
            dispatch_manager(state, ManagerMessage::ToggleEditorGeneralSelected);
            true
        }
        EditorImmediateActionKeyPlan::ToggleMountReadonlySelected => {
            dispatch_manager(state, ManagerMessage::ToggleEditorMountReadonlySelected);
            true
        }
        EditorImmediateActionKeyPlan::ToggleSecretMask { scope, key } => {
            dispatch_manager(state, ManagerMessage::ToggleEditorSecretMask { scope, key });
            true
        }
        EditorImmediateActionKeyPlan::NotImmediateAction => false,
    }
}

fn dispatch_editor_role_header_expansion(
    state: &mut ManagerState<'_>,
    plan: EditorRoleHeaderExpansionKeyPlan,
) {
    match plan {
        EditorRoleHeaderExpansionKeyPlan::Secrets(RoleHeaderExpansionPlan::Set {
            role,
            expanded,
        }) => {
            dispatch_manager(
                state,
                ManagerMessage::SetEditorSecretsRoleExpanded { role, expanded },
            );
        }
        EditorRoleHeaderExpansionKeyPlan::Auth(RoleHeaderExpansionPlan::Set { role, expanded }) => {
            dispatch_manager(
                state,
                ManagerMessage::SetEditorAuthRoleExpanded { role, expanded },
            );
        }
        EditorRoleHeaderExpansionKeyPlan::Secrets(RoleHeaderExpansionPlan::HeaderNoop)
        | EditorRoleHeaderExpansionKeyPlan::Auth(RoleHeaderExpansionPlan::HeaderNoop) => {}
        EditorRoleHeaderExpansionKeyPlan::Secrets(RoleHeaderExpansionPlan::NotHeader)
        | EditorRoleHeaderExpansionKeyPlan::Auth(RoleHeaderExpansionPlan::NotHeader)
        | EditorRoleHeaderExpansionKeyPlan::NotRoleHeaderTab => {}
    }
}

pub type EditorModalOutcome = crate::tui::message::ConsoleEditorModalOutcome<
    jackin_core::RoleSelector,
    jackin_config::RoleSource,
    jackin_core::OpRef,
>;

#[allow(
    clippy::too_many_lines,
    reason = "Editor-modal input dispatcher handling every per-modal-state key \
              binding inline. Each key-event arm carries its own focused state \
              transition; extracting arms into sub-dispatchers would require \
              re-borrowing the editor state across fn boundaries and obscure \
              the per-binding readability."
)]
pub fn handle_editor_modal(
    editor: &mut EditorState<'_>,
    key: KeyEvent,
    op_available: bool,
    op_cache: std::rc::Rc<std::cell::RefCell<jackin_env::OpCache>>,
    config: &mut AppConfig,
    _paths: &JackinPaths,
    term_size: ratatui::layout::Rect,
) -> EditorModalOutcome {
    let Some(modal) = editor.modal.as_mut() else {
        return EditorModalOutcome::Continue;
    };
    match modal {
        Modal::TextInput { target, state } => {
            match inline_picker_plan(state.handle_key(key)) {
                InlinePickerPlan::Commit(value) => {
                    let target = target.clone();
                    if target == TextInputTarget::Role {
                        editor.clear_modal_chain();
                        return apply_role_input(editor, config, &value);
                    }
                    apply_text_input_to_pending(&target, editor, &value, op_available);
                }
                InlinePickerPlan::Dismiss => {
                    let target = target.clone();
                    if matches!(target, TextInputTarget::AuthCredential) {
                        // Plain-text leg of the source-picker round trip
                        // recovers identically to the OpPicker leg.
                        editor.dismiss_active_modal();
                        super::auth::restore_auth_form_after_op_picker_cancel(editor);
                        return EditorModalOutcome::Continue;
                    }
                    editor.pop_modal_chain();
                }
                InlinePickerPlan::Continue => {}
            }
        }
        Modal::FileBrowser { state, .. } => {
            let page_rows = page_rows_for_modal(term_size, state);
            let outcome = state.handle_key_with_page_rows(key, Some(page_rows));
            match file_browser_modal_plan(outcome) {
                FileBrowserModalPlan::Dismiss => {
                    editor.pop_modal_chain();
                }
                FileBrowserModalPlan::ResolveGitUrl(path) => {
                    return EditorModalOutcome::ResolveFileBrowserGitUrl(path);
                }
                FileBrowserModalPlan::OpenUrl(url) => return EditorModalOutcome::OpenUrl(url),
                FileBrowserModalPlan::Continue => {}
                FileBrowserModalPlan::ApplyFileBrowserOutcome(outcome) => {
                    return EditorModalOutcome::ApplyFileBrowserOutcome(outcome);
                }
            }
        }
        Modal::WorkdirPick { state } => match inline_picker_plan(state.handle_key(key)) {
            InlinePickerPlan::Commit(workdir) => {
                editor.commit_workdir_input(workdir);
            }
            InlinePickerPlan::Dismiss => {
                editor.pop_modal_chain();
            }
            InlinePickerPlan::Continue => {}
        },
        Modal::Confirm { target, state } => match bool_confirm_modal_plan(state.handle_key(key)) {
            BoolConfirmModalPlan::Confirm => {
                let target = target.clone();
                editor.clear_modal_chain();
                // Source-drift acknowledgement consumes `plan` and
                // re-stashes it as a `PendingCommit` for the outer
                // dispatcher (which owns `paths` / `cwd` / `runner`)
                // to drain via `commit_editor_save`.
                if let ConfirmTarget::DeleteIsolatedAndSave {
                    mut plan,
                    exit_on_success,
                    ..
                } = target
                {
                    plan.delete_isolated_acknowledged = true;
                    plan.isolated_cleanup_complete = false;
                    editor.save_flow = EditorSaveFlow::PendingCommit {
                        plan,
                        exit_on_success,
                    };
                } else {
                    match apply_editor_confirm(editor, &target) {
                        Ok(EditorModalOutcome::Continue) => {}
                        Ok(outcome) => return outcome,
                        Err(e) => open_editor_action_error(editor, &e),
                    }
                }
            }
            BoolConfirmModalPlan::Dismiss => {
                let was_drift = matches!(target, ConfirmTarget::DeleteIsolatedAndSave { .. });
                editor.clear_modal_chain();
                if was_drift {
                    editor.save_flow = EditorSaveFlow::Idle;
                }
            }
            BoolConfirmModalPlan::Continue => {}
        },
        Modal::MountDstChoice {
            target,
            state: modal_state,
        } => {
            let target = target.clone();
            let src = modal_state.src.clone();
            let outcome = modal_state.handle_key(key);
            dispatch_editor_mount_dst_choice(editor, target, &src, &outcome);
        }
        Modal::SaveDiscardCancel { state: modal_state } => {
            match save_discard_modal_plan(modal_state.handle_key(key)) {
                SaveDiscardModalPlan::Save => {
                    editor.clear_modal_chain();
                    editor.exit_after_save = Some(ExitIntent::Save);
                }
                SaveDiscardModalPlan::Discard => {
                    editor.clear_modal_chain();
                    editor.exit_after_save = Some(ExitIntent::Discard);
                }
                SaveDiscardModalPlan::Dismiss => {
                    editor.clear_modal_chain();
                }
                SaveDiscardModalPlan::Continue => {}
            }
        }
        // List-view modals; defensive cancel if one lands here.
        Modal::GithubPicker { .. } | Modal::RolePicker { .. } => {
            editor.clear_modal_chain();
        }
        Modal::RoleOverridePicker { state: picker } => {
            match inline_picker_plan(picker.handle_key(key)) {
                InlinePickerPlan::Commit(role) => {
                    // The override section materializes organically on
                    // the first value commit; we don't touch
                    // `pending.roles` here, so a cancel mid-flow leaves
                    // no empty placeholder.
                    let role_name = role.key();
                    let scope = SecretsScopeTag::Role(role_name);
                    let label = secret_new_key_label(&scope);
                    let state = env_key_input_state(editor, &scope, label, "");
                    editor.open_sub_modal(Modal::TextInput {
                        target: TextInputTarget::EnvKey { scope },
                        state,
                    });
                }
                InlinePickerPlan::Dismiss => {
                    editor.pop_modal_chain();
                }
                InlinePickerPlan::Continue => {}
            }
        }
        Modal::ConfirmSave { state: modal_state } => {
            match confirm_save_modal_plan(modal_state.handle_key(key)) {
                ConfirmSaveModalPlan::Commit => {
                    // Confirming → PendingCommit atomically so plan +
                    // exit_on_success travel together to the outer
                    // handler that holds paths/cwd.
                    let plan = crate::tui::state::PendingSaveCommit {
                        effective_removals: modal_state.effective_removals.clone(),
                        final_mounts: modal_state.final_mounts.clone(),
                        // First commit pass — the drift check in
                        // `commit_editor_save` runs unconditionally. The
                        // `DeleteIsolatedAndSave` confirm modal is what
                        // re-stashes the plan with the flag flipped to
                        // `true` so the second pass skips the check.
                        delete_isolated_acknowledged: false,
                        isolated_cleanup_complete: false,
                    };
                    let exit_on_success = matches!(
                        editor.save_flow,
                        EditorSaveFlow::Confirming {
                            exit_on_success: true
                        }
                    );
                    editor.clear_modal_chain();
                    editor.save_flow = EditorSaveFlow::PendingCommit {
                        plan,
                        exit_on_success,
                    };
                }
                ConfirmSaveModalPlan::Dismiss => {
                    editor.clear_modal_chain();
                    editor.save_flow = EditorSaveFlow::Idle;
                }
                ConfirmSaveModalPlan::Continue => {}
            }
        }
        Modal::ErrorPopup { state: popup_state } => {
            match dismissible_modal_plan(popup_state.handle_key(key)) {
                DismissibleModalPlan::Dismiss => {
                    // A source-folder validation rejection stacks this popup
                    // directly over the auth source-folder picker. Dismissing it
                    // returns to that picker so the operator can pick another
                    // folder, rather than tearing down the whole auth flow.
                    if matches!(
                        editor.modal_parents.last(),
                        Some(Modal::FileBrowser {
                            target: FileBrowserTarget::AuthFormSourceFolder,
                            ..
                        })
                    ) {
                        editor.pop_modal_chain();
                        return EditorModalOutcome::Continue;
                    }
                    editor.clear_modal_chain();
                    editor.save_flow = EditorSaveFlow::Idle;
                    // If the popup was raised by a failed OpPicker commit
                    // for the auth form, the form's state was re-stashed
                    // into the modal parent stack instead of being
                    // re-mounted directly — restore it now so the operator
                    // lands back on the form with the prior credential
                    // unchanged, ready to retry through the source picker.
                    if editor.has_modal_parent() {
                        super::auth::restore_auth_form_after_op_picker_cancel(editor);
                    }
                }
                DismissibleModalPlan::Continue => {}
            }
        }
        Modal::StatusPopup { .. } | Modal::ContainerInfo { .. } => {}
        Modal::ScopePicker { state: scope_state } => {
            match scope_picker_plan(scope_state.handle_key(key)) {
                ScopePickerPlan::AllAgents => {
                    let scope = SecretsScopeTag::Workspace;
                    let state =
                        env_key_input_state(editor, &scope, secret_new_key_label(&scope), "");
                    editor.open_sub_modal(Modal::TextInput {
                        target: TextInputTarget::EnvKey { scope },
                        state,
                    });
                }
                ScopePickerPlan::SpecificAgent => {
                    // Empty eligible set → `open_agent_override_picker`
                    // is a no-op; we close the modal then.
                    agents::open_agent_override_picker(editor, config);
                    if !editor.has_active_role_override_picker() {
                        editor.clear_modal_chain();
                    }
                }
                ScopePickerPlan::Dismiss => {
                    editor.pop_modal_chain();
                }
                ScopePickerPlan::Continue => {}
            }
        }
        Modal::SourcePicker {
            state: source,
            env_key,
        } => {
            match source_picker_plan(source.handle_key(key)) {
                SourcePickerPlan::Plain => {
                    let Some((scope, key)) = env_key.take() else {
                        editor.clear_modal_chain();
                        return EditorModalOutcome::Continue;
                    };
                    editor.open_sub_modal(Modal::TextInput {
                        target: TextInputTarget::EnvValue {
                            scope,
                            key: key.clone(),
                        },
                        state: secret_new_value_input_state(&key),
                    });
                }
                SourcePickerPlan::Op => {
                    let Some((scope, key)) = env_key.take() else {
                        editor.clear_modal_chain();
                        return EditorModalOutcome::Continue;
                    };
                    editor.open_sub_modal(Modal::OpPicker {
                        secrets_target: Some(SecretsPickerTarget::Existing { scope, key }),
                        state: Box::new(OpPickerState::new_with_cache(op_cache)),
                    });
                }
                SourcePickerPlan::Dismiss => {
                    // Cancel: drop the in-flight key name and close
                    // the modal. Operator returns to the Secrets tab
                    // with no env entry added.
                    editor.pop_modal_chain();
                }
                SourcePickerPlan::Continue => {}
            }
        }
        Modal::AuthSourcePicker { state: source } => {
            let outcome = source.handle_key(key);
            // Generate wins over the provide dispatch: the `g`/`G` trigger
            // sets `generating_token_target` (and stashes the form into
            // the modal parent stack for the post-mint re-mount), so
            // the generate branch is reachable only on that path and the
            // provide arms below stay untouched.
            if editor.generating_token_target.is_some() {
                match source_picker_plan(outcome) {
                    SourcePickerPlan::Plain => {
                        start_plain_token_generate(editor);
                    }
                    SourcePickerPlan::Op => {
                        open_create_op_picker_for_generate(editor, op_cache);
                    }
                    // Cancel before minting: restore the stashed form so
                    // the operator lands back on the Edit-auth dialog
                    // unchanged (matches the provide-path source-picker
                    // cancel below).
                    SourcePickerPlan::Dismiss => {
                        editor.generating_token_target = None;
                        super::auth::restore_auth_form_after_op_picker_cancel(editor);
                    }
                    SourcePickerPlan::Continue => {}
                }
                return EditorModalOutcome::Continue;
            }
            match source_picker_plan(outcome) {
                SourcePickerPlan::Plain => {
                    super::auth::apply_plain_source_picker_to_auth_form(editor);
                }
                SourcePickerPlan::Op => {
                    super::auth::open_op_picker_from_auth_source(editor, op_cache);
                }
                SourcePickerPlan::Dismiss => {
                    super::auth::restore_auth_form_after_op_picker_cancel(editor);
                }
                SourcePickerPlan::Continue => {}
            }
        }
        Modal::AuthForm { .. } => {
            if matches!(
                super::auth::handle_auth_form_key(editor, key, op_available),
                super::auth::AuthFormKeyOutcome::OpenSourceFolderBrowser
            ) {
                return EditorModalOutcome::OpenAuthSourceFolderBrowser;
            }
        }
        Modal::AuthRolePicker { state: picker } => match inline_picker_plan(picker.handle_key(key))
        {
            InlinePickerPlan::Commit(role) => {
                if let Some(kind) = editor.auth_selected_kind {
                    let target = crate::tui::state::AuthFormTarget::WorkspaceRole {
                        role: role.key(),
                        kind,
                    };
                    let form = crate::tui::state::AuthForm::new(kind);
                    editor.open_sub_modal(Modal::AuthForm {
                        target,
                        state: Box::new(form),
                        focus: crate::tui::state::AuthFormFocus::Mode,
                        literal_buffer: String::new(),
                    });
                } else {
                    editor.pop_modal_chain();
                }
            }
            InlinePickerPlan::Dismiss => {
                editor.pop_modal_chain();
            }
            InlinePickerPlan::Continue => {}
        },
        Modal::OpPicker {
            secrets_target,
            state: picker,
        } => {
            let outcome = picker.handle_key(key);
            let secrets_target = secrets_target.clone();
            // Token-generate wins over both browse and provide dispatch:
            // `generating_token_target` is set exactly when the picker was
            // opened by the auth-form `g`/`G` trigger (Create mode), so the
            // create variants are reachable only on this path.
            if let Some(target) = editor.generating_token_target.take() {
                handle_token_generate_pick(editor, target, outcome);
                return EditorModalOutcome::Continue;
            }
            match inline_picker_plan(outcome) {
                // Browse-mode caller: only `Existing` is reachable.
                InlinePickerPlan::Commit(
                    crate::tui::op_picker::OpPickerSelection::NewItem { .. }
                    | crate::tui::op_picker::OpPickerSelection::EditItemField { .. },
                ) => unreachable!("Secrets-tab OpPicker runs in Browse mode"),
                InlinePickerPlan::Commit(crate::tui::op_picker::OpPickerSelection::Existing(
                    op_ref,
                )) => {
                    // Auth-form round trip wins over the Secrets-tab
                    // dispatch: the auth form sets
                    // the modal parent stack exactly when it's the
                    // caller, so the two paths can never collide.
                    if editor.has_modal_parent() {
                        // Close the OpPicker — the auth form stays stashed on
                        // modal_parents so the _committed / _failed helpers find it.
                        editor.dismiss_active_modal();
                        return EditorModalOutcome::ValidateOpRef(op_ref);
                    }
                    // Operator picked a Vault → Item → Field path. The
                    // dispatch depends on whether `P` was pressed on a
                    // key row (write directly) or on an `+ Add` sentinel
                    // (stash the OpRef, ask for the key name first).
                    match secrets_target {
                        Some(SecretsPickerTarget::Existing { scope, key }) => {
                            set_pending_env_op_ref(editor, &scope, &key, op_ref);
                            editor.clear_modal_chain();
                        }
                        Some(SecretsPickerTarget::NewKey { scope }) => {
                            let label = secret_new_key_after_picker_label(&scope);
                            let state = env_key_input_state(editor, &scope, label, "");
                            editor.open_sub_modal(Modal::TextInput {
                                target: TextInputTarget::EnvKeyWithValue {
                                    scope: scope.clone(),
                                    value: jackin_core::EnvValue::OpRef(op_ref),
                                },
                                state,
                            });
                        }
                        None => {
                            editor.clear_modal_chain();
                        }
                    }
                }
                InlinePickerPlan::Dismiss => {
                    // Auth-form round trip: re-mount the form
                    // unchanged. Mirrors the Commit branch — the two
                    // callers (Secrets-tab `P`, auth-form Enter) are
                    // disambiguated by the modal parent stack.
                    if editor.has_modal_parent() {
                        super::auth::restore_auth_form_after_op_picker_cancel(editor);
                        return EditorModalOutcome::Continue;
                    }
                    editor.pop_modal_chain();
                }
                InlinePickerPlan::Continue => {}
            }
        }
    }
    EditorModalOutcome::Continue
}

fn apply_role_input(
    editor: &mut EditorState<'_>,
    config: &AppConfig,
    value: &str,
) -> EditorModalOutcome {
    match crate::services::role_source::resolve_role_input_source(config, value) {
        Ok(resolved) => EditorModalOutcome::StartRoleRegistration {
            raw: resolved.raw,
            key: resolved.key,
            selector: resolved.selector,
            source: resolved.source,
        },
        Err(e) => {
            let err_text = e.error.to_string();
            if let Some(panic_message) = err_text.strip_prefix("role loader panicked: ") {
                let message = crate::tui::components::error_popup::internal_role_load_error_message(
                    &e.raw,
                    panic_message,
                );
                open_role_input_error(editor, &message);
                return EditorModalOutcome::Continue;
            }
            open_role_resolution_error(editor, &e.raw, e.source_url.as_ref(), &e.error);
            EditorModalOutcome::Continue
        }
    }
}

fn apply_editor_confirm(
    editor: &mut EditorState<'_>,
    target: &ConfirmTarget,
) -> anyhow::Result<EditorModalOutcome> {
    match target {
        ConfirmTarget::DeleteEnvVar { scope, key } => {
            editor.delete_env_var(scope, key)?;
        }
        ConfirmTarget::TrustRoleSource { key, source } => {
            return Ok(EditorModalOutcome::PersistTrustedRoleSource {
                key: key.clone(),
                source: source.clone(),
            });
        }
        // `DeleteIsolatedAndSave` is handled inline at the dispatch
        // site because it consumes `plan` and routes through
        // `EditorSaveFlow::PendingCommit`. No-op here.
        ConfirmTarget::DeleteIsolatedAndSave { .. } => {}
    }
    Ok(EditorModalOutcome::Continue)
}

/// Only `EditAddMountSrc` is meaningful here; the prelude's
/// `CreateFirstMountSrc` target routes through `handle_prelude_modal`.
fn dispatch_editor_mount_dst_choice(
    editor: &mut EditorState<'_>,
    target: FileBrowserTarget,
    src: &str,
    outcome: &jackin_tui::ModalOutcome<crate::tui::components::mount_dst_choice::MountDstChoice>,
) {
    match mount_dst_choice_plan(outcome.clone()) {
        MountDstChoicePlan::CommitSamePath => {
            if target == FileBrowserTarget::EditAddMountSrc {
                editor.add_shared_mount(src, src);
            }
            editor.clear_modal_chain();
        }
        MountDstChoicePlan::OpenEditInput => {
            if target == FileBrowserTarget::EditAddMountSrc {
                editor.add_shared_mount(src, src);
                editor.open_sub_modal(Modal::TextInput {
                    target: TextInputTarget::MountDst,
                    state: mount_destination_input_state(src),
                });
            } else {
                editor.clear_modal_chain();
            }
        }
        MountDstChoicePlan::Dismiss => {
            editor.pop_modal_chain();
        }
        MountDstChoicePlan::Continue => {}
    }
}

pub fn apply_file_browser_to_editor(
    target: FileBrowserTarget,
    editor: &mut EditorState<'_>,
    path: std::path::PathBuf,
) {
    match target {
        FileBrowserTarget::EditAddMountSrc => {
            // Defer the mount push to the choice modal: in the common case
            // the operator will take "Mount at same path" (dst = src) and we skip the
            // TextInput entirely. Only the `Edit destination` branch pushes
            // a provisional mount and opens the TextInput.
            editor.open_sub_modal(Modal::MountDstChoice {
                target,
                state: mount_dst_choice_state(path.display().to_string()),
            });
        }
        FileBrowserTarget::CreateFirstMountSrc => {
            // Only meaningful in prelude path — handled by
            // `handle_prelude_modal`.
            drop((editor, path));
        }
        FileBrowserTarget::AuthFormSourceFolder => {
            super::auth::apply_source_folder_to_auth_form(editor, path);
        }
    }
}

fn open_role_resolution_error(
    editor: &mut EditorState<'_>,
    raw: &str,
    source_url: Option<&String>,
    err: &anyhow::Error,
) {
    use crate::tui::components::error_popup::{
        configured_role_load_error_message, generic_role_repository_error_message,
        repository_role_load_error_message,
    };
    jackin_diagnostics::debug_log!(
        "role",
        "showing role-load error popup for raw={raw:?}: {err:?}"
    );
    let message = source_url.map_or_else(
        || configured_role_load_error_message(raw),
        |source_url| {
            repository_role_load_error_message(
                raw,
                source_url,
                generic_role_repository_error_message(),
            )
        },
    );
    editor.open_error_popup(
        crate::tui::components::error_popup::role_load_error_popup_state(message),
    );
}
