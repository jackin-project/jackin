//! Editor-stage dispatch: tab navigation, field focus, per-tab key
//! handling, and the editor-level modal dispatcher.

pub(super) mod agents;
pub(super) mod general;
pub(super) mod modal;
pub(super) mod mounts;
pub(super) mod secrets;
pub(super) use modal::{
    apply_text_input_to_pending, env_key_input_state, handle_token_generate_pick,
    open_create_op_picker_for_generate, open_secrets_picker_modal, set_pending_env_op_ref,
    start_plain_token_generate,
};

#[cfg(test)]
pub(super) use jackin_console::tui::screens::editor::view::role_load_input_state;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::InputOutcome;
use crate::console::tui::effect::ManagerEffect;
use crate::console::tui::message::{ManagerMessage, update_manager};
use crate::console::tui::op_picker::OpPickerState;
#[cfg(test)]
use crate::console::tui::state::PendingRoleLoad;
use crate::console::tui::state::{
    AuthRow, ConfirmTarget, EditorSaveFlow, EditorState, EditorTab, ExitIntent, FieldFocus,
    FileBrowserTarget, ManagerStage, ManagerState, Modal, SecretsRow, SecretsScopeTag,
    TextInputTarget, open_editor_action_error, open_role_input_error, open_role_resolution_error,
};
use crate::paths::JackinPaths;
use jackin_config::AppConfig;
use jackin_console::tui::components::error_popup::no_github_url_error_popup_state;
use jackin_console::tui::components::file_browser::page_rows_for_modal;
use jackin_console::tui::components::save_discard::editor_exit_save_discard_state;
use jackin_console::tui::mount_display::workspace_config_mounts_content_width_with_cache;
use jackin_console::tui::screens::editor::update::{
    auth_skipped_rows, editor_max_row_for_tab, editor_mount_add_row_selected,
    editor_role_add_row_selected, editor_secrets_selection_bounds,
};
use jackin_console::tui::screens::editor::view::{
    mount_destination_input_state, mount_dst_choice_state, secret_new_key_after_picker_label,
    secret_new_key_label, secret_new_value_input_state,
};
use jackin_console::tui::update::{
    FileBrowserModalPlan, MountDstChoicePlan, file_browser_modal_plan, mount_dst_choice_plan,
};
use jackin_tui::ModalOutcome;
#[cfg(test)]
use jackin_tui::runtime::{Subscription, SubscriptionPoll};

// Central keymap dispatch — table-like layout makes the keymap
// readable at a glance; extracting per-key helpers just scatters it.
#[expect(
    clippy::too_many_lines,
    reason = "pending extraction — tracked in codebase-readability roadmap"
)]
pub(super) fn handle_editor_key(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    paths: &JackinPaths,
    cwd: &std::path::Path,
    key: KeyEvent,
) -> anyhow::Result<InputOutcome> {
    // s and Esc handled outside the editor borrow — both need to
    // call back into state/config.
    match key.code {
        KeyCode::Char('s' | 'S') => {
            if let ManagerStage::Editor(editor) = &state.stage
                && editor.change_count() == 0
            {
                return Ok(InputOutcome::Continue);
            }
            if matches!(&state.stage, ManagerStage::Editor(_)) {
                super::save::begin_editor_save(state, config, true)?;
            }
            // `paths` is consumed by the commit path in
            // handle_editor_modal, not here.
            let _unused = paths;
            return Ok(InputOutcome::Continue);
        }
        KeyCode::Esc => {
            if let ManagerStage::Editor(editor) = &state.stage {
                if !editor.tab_bar_focused() {
                    if editor.active_tab == EditorTab::Auth && editor.auth_selected_kind.is_some() {
                        dispatch_manager(state, ManagerMessage::ClearEditorAuthKind);
                    }
                    dispatch_manager(state, ManagerMessage::FocusEditorTabBar);
                    return Ok(InputOutcome::Continue);
                }
                // Auth-tab in-tab pop: clears the focused-kind
                // selection without dirty check (see EditorState
                // field doc). A subsequent Esc on the picker view
                // falls through to the dirty branch below.
                if editor.active_tab == EditorTab::Auth && editor.auth_selected_kind.is_some() {
                    dispatch_manager(state, ManagerMessage::ClearEditorAuthKind);
                    return Ok(InputOutcome::Continue);
                }
                let dirty = editor.is_dirty();
                if dirty {
                    if let ManagerStage::Editor(editor) = &mut state.stage {
                        editor.modal = Some(Modal::SaveDiscardCancel {
                            state: editor_exit_save_discard_state(),
                        });
                    }
                } else {
                    let _unused = update_manager(
                        state,
                        ManagerMessage::ReloadFromConfig {
                            config: Box::new(config.clone()),
                            cwd: cwd.to_path_buf(),
                        },
                    );
                }
            }
            return Ok(InputOutcome::Continue);
        }
        _ => {}
    }

    // Capture before the editor borrow (separate fields, but explicit is cleaner).
    let op_cache = std::rc::Rc::clone(&state.op_cache);
    let op_available = state.op_available;
    let term_width = state.cached_term_size.width;
    let term_size = state.cached_term_size;

    if let ManagerStage::Editor(editor) = &state.stage {
        match key.code {
            KeyCode::Left | KeyCode::BackTab if editor.tab_bar_focused() => {
                dispatch_manager(
                    state,
                    ManagerMessage::MoveEditorTab {
                        delta: -1,
                        focus_tab_bar: true,
                    },
                );
                return Ok(InputOutcome::Continue);
            }
            KeyCode::Right if editor.tab_bar_focused() => {
                dispatch_manager(
                    state,
                    ManagerMessage::MoveEditorTab {
                        delta: 1,
                        focus_tab_bar: true,
                    },
                );
                return Ok(InputOutcome::Continue);
            }
            KeyCode::Tab | KeyCode::Down | KeyCode::Char('j' | 'J') if editor.tab_bar_focused() => {
                dispatch_manager(state, ManagerMessage::FocusEditorContent);
                return Ok(InputOutcome::Continue);
            }
            KeyCode::Tab => {
                dispatch_manager(
                    state,
                    ManagerMessage::MoveEditorTab {
                        delta: 1,
                        focus_tab_bar: true,
                    },
                );
                return Ok(InputOutcome::Continue);
            }
            KeyCode::BackTab => {
                dispatch_manager(state, ManagerMessage::FocusEditorTabBar);
                return Ok(InputOutcome::Continue);
            }
            KeyCode::Char('h' | 'H') if editor.active_tab == EditorTab::Mounts => {
                dispatch_manager(
                    state,
                    ManagerMessage::ScrollEditorWorkspaceMountsHorizontal {
                        delta: -8,
                        term_width,
                        content_width: workspace_config_mounts_content_width_with_cache(
                            &editor.pending.mounts,
                            &editor.mount_info_cache,
                        ),
                    },
                );
                return Ok(InputOutcome::Continue);
            }
            KeyCode::Char('l' | 'L') if editor.active_tab == EditorTab::Mounts => {
                dispatch_manager(
                    state,
                    ManagerMessage::ScrollEditorWorkspaceMountsHorizontal {
                        delta: 8,
                        term_width,
                        content_width: workspace_config_mounts_content_width_with_cache(
                            &editor.pending.mounts,
                            &editor.mount_info_cache,
                        ),
                    },
                );
                return Ok(InputOutcome::Continue);
            }
            KeyCode::Char('h' | 'H') => {
                dispatch_manager(
                    state,
                    ManagerMessage::ScrollEditorTabHorizontal {
                        delta: -8,
                        term_width,
                        content_width: editor.tab_content_width,
                    },
                );
                return Ok(InputOutcome::Continue);
            }
            KeyCode::Char('l' | 'L') => {
                dispatch_manager(
                    state,
                    ManagerMessage::ScrollEditorTabHorizontal {
                        delta: 8,
                        term_width,
                        content_width: editor.tab_content_width,
                    },
                );
                return Ok(InputOutcome::Continue);
            }
            KeyCode::Up | KeyCode::Char('k' | 'K') => {
                let (max_row, skipped_rows) = editor_selection_bounds(editor, config);
                dispatch_manager(
                    state,
                    ManagerMessage::MoveEditorFieldSelection {
                        delta: -1,
                        max_row,
                        skipped_rows,
                        term: term_size,
                        footer_h: editor.cached_footer_h,
                    },
                );
                return Ok(InputOutcome::Continue);
            }
            KeyCode::Down | KeyCode::Char('j' | 'J') => {
                let (max_row, skipped_rows) = editor_selection_bounds(editor, config);
                dispatch_manager(
                    state,
                    ManagerMessage::MoveEditorFieldSelection {
                        delta: 1,
                        max_row,
                        skipped_rows,
                        term: term_size,
                        footer_h: editor.cached_footer_h,
                    },
                );
                return Ok(InputOutcome::Continue);
            }
            KeyCode::Right if editor.active_tab == EditorTab::Secrets => {
                let FieldFocus::Row(n) = editor.active_field;
                let rows = editor.secrets_flat_rows();
                if let Some(SecretsRow::RoleHeader { role, expanded }) = rows.get(n).cloned() {
                    if !expanded {
                        dispatch_manager(
                            state,
                            ManagerMessage::SetEditorSecretsRoleExpanded {
                                role,
                                expanded: true,
                            },
                        );
                    }
                    return Ok(InputOutcome::Continue);
                }
            }
            KeyCode::Left if editor.active_tab == EditorTab::Secrets => {
                let FieldFocus::Row(n) = editor.active_field;
                let rows = editor.secrets_flat_rows();
                if let Some(SecretsRow::RoleHeader { role, expanded }) = rows.get(n).cloned() {
                    if expanded {
                        dispatch_manager(
                            state,
                            ManagerMessage::SetEditorSecretsRoleExpanded {
                                role,
                                expanded: false,
                            },
                        );
                    }
                    return Ok(InputOutcome::Continue);
                }
            }
            KeyCode::Right if editor.active_tab == EditorTab::Auth => {
                let FieldFocus::Row(n) = editor.active_field;
                let rows = editor.auth_flat_rows(config);
                if let Some(AuthRow::RoleHeader { role, expanded }) = rows.get(n).cloned() {
                    if !expanded {
                        dispatch_manager(
                            state,
                            ManagerMessage::SetEditorAuthRoleExpanded {
                                role,
                                expanded: true,
                            },
                        );
                    }
                    return Ok(InputOutcome::Continue);
                }
            }
            KeyCode::Left if editor.active_tab == EditorTab::Auth => {
                let FieldFocus::Row(n) = editor.active_field;
                let rows = editor.auth_flat_rows(config);
                if let Some(AuthRow::RoleHeader { role, expanded }) = rows.get(n).cloned() {
                    if expanded {
                        dispatch_manager(
                            state,
                            ManagerMessage::SetEditorAuthRoleExpanded {
                                role,
                                expanded: false,
                            },
                        );
                    }
                    return Ok(InputOutcome::Continue);
                }
            }
            KeyCode::Enter if editor.active_tab == EditorTab::Auth => {
                let FieldFocus::Row(n) = editor.active_field;
                let rows = editor.auth_flat_rows(config);
                if let Some(AuthRow::AuthKindRow { kind }) = rows.get(n) {
                    dispatch_manager(state, ManagerMessage::EnterEditorAuthKind { kind: *kind });
                    return Ok(InputOutcome::Continue);
                }
            }
            KeyCode::Char(' ') if editor.active_tab == EditorTab::General => {
                dispatch_manager(state, ManagerMessage::ToggleEditorGeneralSelected);
                return Ok(InputOutcome::Continue);
            }
            KeyCode::Char('r' | 'R') if editor.active_tab == EditorTab::Mounts => {
                dispatch_manager(state, ManagerMessage::ToggleEditorMountReadonlySelected);
                return Ok(InputOutcome::Continue);
            }
            KeyCode::Char('m' | 'M')
                if editor.active_tab == EditorTab::Secrets
                    && (key.modifiers - KeyModifiers::SHIFT).is_empty() =>
            {
                if let Some((scope, key)) = secrets::focused_unmask_key(editor) {
                    dispatch_manager(state, ManagerMessage::ToggleEditorSecretMask { scope, key });
                }
                return Ok(InputOutcome::Continue);
            }
            _ => {}
        }
    }

    let ManagerStage::Editor(editor) = &mut state.stage else {
        return Ok(InputOutcome::Continue);
    };

    match key.code {
        KeyCode::Enter => match editor.active_tab {
            EditorTab::General => general::open_editor_field_modal(editor),
            EditorTab::Mounts => {
                let FieldFocus::Row(n) = editor.active_field;
                if editor_mount_add_row_selected(n, editor.pending.mounts.len()) {
                    state.request_effect(ManagerEffect::OpenEditorAddMountFileBrowser);
                    return Ok(InputOutcome::Continue);
                }
            }
            EditorTab::Secrets => {
                // For op-ref rows Enter re-opens the 1Password picker (same as P).
                let FieldFocus::Row(n) = editor.active_field;
                let rows = editor.secrets_flat_rows();
                let is_op_ref = match rows.get(n) {
                    Some(SecretsRow::WorkspaceKeyRow(key)) => editor
                        .pending
                        .env
                        .get(key)
                        .is_some_and(|v| matches!(v, jackin_core::EnvValue::OpRef(_))),
                    Some(SecretsRow::RoleKeyRow { role, key }) => editor
                        .pending
                        .roles
                        .get(role)
                        .and_then(|o| o.env.get(key))
                        .is_some_and(|v| matches!(v, jackin_core::EnvValue::OpRef(_))),
                    _ => false,
                };
                if is_op_ref && op_available {
                    open_secrets_picker_modal(editor, op_cache);
                } else {
                    secrets::open_secrets_enter_modal(editor);
                }
            }
            EditorTab::Roles => {
                let FieldFocus::Row(n) = editor.active_field;
                if editor_role_add_row_selected(n, config.roles.len()) {
                    agents::open_role_input(editor, config);
                }
            }
            EditorTab::Auth => {
                let FieldFocus::Row(n) = editor.active_field;
                let rows = editor.auth_flat_rows(config);
                match rows.get(n) {
                    Some(AuthRow::AddSentinel { .. }) => {
                        super::auth::open_auth_role_picker(editor, config);
                    }
                    Some(AuthRow::RoleHeader { role, .. }) => {
                        super::auth::toggle_role_expand(editor, role.clone());
                    }
                    Some(AuthRow::WorkspaceMode { .. } | AuthRow::RoleMode { .. }) => {
                        super::auth::open_auth_form_modal(editor, config);
                    }
                    _ => {}
                }
            }
        },
        KeyCode::Char('a' | 'A') if editor.active_tab == EditorTab::Roles => {
            agents::open_role_input(editor, config);
        }
        KeyCode::Char('a' | 'A')
            if editor.active_tab == EditorTab::Auth && editor.auth_selected_kind.is_some() =>
        {
            super::auth::open_auth_role_picker(editor, config);
        }
        KeyCode::Char(' ') if editor.active_tab == EditorTab::Roles => {
            agents::toggle_agent_allowed_at_cursor(editor, config);
        }
        KeyCode::Char('*') if editor.active_tab == EditorTab::Roles => {
            agents::toggle_default_agent_at_cursor(editor, config);
        }
        KeyCode::Char('a' | 'A') if editor.active_tab == EditorTab::Mounts => {
            state.request_effect(ManagerEffect::OpenEditorAddMountFileBrowser);
            return Ok(InputOutcome::Continue);
        }
        KeyCode::Char('d' | 'D') if editor.active_tab == EditorTab::Mounts => {
            mounts::remove_mount_at_cursor(editor);
        }
        KeyCode::Char('d' | 'D') if editor.active_tab == EditorTab::Auth => {
            super::auth::handle_d_on_auth_row(editor, config);
        }
        // M toggles per-row masking on the focused Secrets-tab key row.
        // Operator feedback (commit 32): the global mask flag was too
        // blunt — it revealed every value at once when an operator just
        // wanted to peek at one. Now M flips membership of `(scope, key)`
        // in `editor.unmasked_rows`. Header / sentinel / op:// rows are
        // no-ops (op:// rows render as breadcrumbs, not masked values).
        //
        // SHIFT modifier tolerated for Caps-Lock parity (see prior
        // commits); Ctrl/Alt/Cmd still bypass the arm.
        // P sits at row level (not inside the EnvValue modal) so it
        // doesn't collide with text input. SHIFT tolerated per the
        // `m|M` arm above.
        KeyCode::Char('p' | 'P')
            if editor.active_tab == EditorTab::Secrets
                && (key.modifiers - KeyModifiers::SHIFT).is_empty()
                && op_available =>
        {
            open_secrets_picker_modal(editor, op_cache);
        }
        KeyCode::Char('d' | 'D')
            if editor.active_tab == EditorTab::Secrets
                && (key.modifiers - KeyModifiers::SHIFT).is_empty() =>
        {
            secrets::open_secrets_delete_confirm(editor);
        }
        KeyCode::Char('a' | 'A')
            if editor.active_tab == EditorTab::Secrets
                && (key.modifiers - KeyModifiers::SHIFT).is_empty() =>
        {
            secrets::open_secrets_add_modal(editor);
        }
        KeyCode::Char('i' | 'I') if editor.active_tab == EditorTab::Mounts => {
            // Cycle the per-mount isolation strategy on the highlighted row.
            // Mirrors the R (readonly) toggle but threads through the
            // dedicated state helper so the cycling rule lives in one place.
            // Silent no-op on the `+ Add mount` sentinel.
            editor.cycle_isolation_for_selected_mount();
        }
        KeyCode::Char('o' | 'O') if editor.active_tab == EditorTab::Mounts => {
            let FieldFocus::Row(n) = editor.active_field;
            if let Some(m) = editor.pending.mounts.get(n) {
                if let Some(web_url) = editor.mount_info_cache.github_web_url(&m.src) {
                    state.request_effect(ManagerEffect::OpenUrl(web_url));
                    return Ok(InputOutcome::Continue);
                }
                editor.modal = Some(Modal::ErrorPopup {
                    state: no_github_url_error_popup_state(),
                });
            }
        }
        _ => {}
    }
    Ok(InputOutcome::Continue)
}

fn editor_selection_bounds(editor: &EditorState<'_>, config: &AppConfig) -> (usize, Vec<usize>) {
    match editor.active_tab {
        EditorTab::Secrets => {
            let rows = editor.secrets_flat_rows();
            editor_secrets_selection_bounds(&rows)
        }
        EditorTab::Auth => {
            let rows = editor.auth_flat_rows(config);
            (
                editor_max_row_for_tab(
                    editor.active_tab,
                    editor.pending.mounts.len(),
                    config.roles.len(),
                    0,
                    rows.len(),
                ),
                auth_skipped_rows(&rows),
            )
        }
        EditorTab::General | EditorTab::Mounts | EditorTab::Roles => (
            editor_max_row_for_tab(
                editor.active_tab,
                editor.pending.mounts.len(),
                config.roles.len(),
                0,
                0,
            ),
            Vec::new(),
        ),
    }
}

fn dispatch_manager(state: &mut ManagerState<'_>, message: ManagerMessage) {
    let _dirty = update_manager(state, message);
}

pub(super) type EditorModalOutcome = jackin_console::tui::message::ConsoleEditorModalOutcome<
    jackin_core::RoleSelector,
    jackin_config::RoleSource,
    jackin_core::OpRef,
>;

#[expect(
    clippy::too_many_lines,
    clippy::needless_pass_by_ref_mut,
    reason = "pending per-modal split — tracked in codebase-readability roadmap"
)]
pub(super) fn handle_editor_modal(
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
            match state.handle_key(key) {
                ModalOutcome::Commit(value) => {
                    let target = target.clone();
                    if target == TextInputTarget::Role {
                        editor.clear_modal_chain();
                        return apply_role_input(editor, config, &value);
                    }
                    apply_text_input_to_pending(&target, editor, &value, op_available);
                }
                ModalOutcome::Cancel => {
                    let target = target.clone();
                    let was_env_textinput = matches!(
                        &target,
                        TextInputTarget::EnvKey { .. } | TextInputTarget::EnvValue { .. }
                    );
                    if matches!(target, TextInputTarget::AuthCredential) {
                        // Plain-text leg of the source-picker round trip
                        // recovers identically to the OpPicker leg.
                        editor.modal = None;
                        super::auth::restore_auth_form_after_op_picker_cancel(editor);
                        return EditorModalOutcome::Continue;
                    }
                    editor.pop_modal_chain();
                    // Scratch slots only get dropped when the pop
                    // unwinds the whole chain — a parent modal (e.g.
                    // SourcePicker) still reading `pending_env_key`
                    // must see it intact.
                    if was_env_textinput && editor.modal.is_none() {
                        // env_key context now in Modal::SourcePicker
                        editor.pending_picker_value = None;
                    }
                }
                ModalOutcome::Continue => {}
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
        Modal::WorkdirPick { state } => match state.handle_key(key) {
            ModalOutcome::Commit(workdir) => {
                editor.pending.workdir = workdir;
                editor.clear_modal_chain();
            }
            ModalOutcome::Cancel => {
                editor.pop_modal_chain();
            }
            ModalOutcome::Continue => {}
        },
        Modal::Confirm { target, state } => match state.handle_key(key) {
            ModalOutcome::Commit(yes) => {
                let target = target.clone();
                editor.clear_modal_chain();
                if yes {
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
                } else if matches!(target, ConfirmTarget::DeleteIsolatedAndSave { .. }) {
                    editor.save_flow = EditorSaveFlow::Idle;
                }
            }
            ModalOutcome::Cancel => {
                let was_drift = matches!(target, ConfirmTarget::DeleteIsolatedAndSave { .. });
                editor.clear_modal_chain();
                if was_drift {
                    editor.save_flow = EditorSaveFlow::Idle;
                }
            }
            ModalOutcome::Continue => {}
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
            use jackin_tui::components::SaveDiscardChoice;
            match modal_state.handle_key(key) {
                ModalOutcome::Commit(SaveDiscardChoice::Save) => {
                    editor.clear_modal_chain();
                    editor.exit_after_save = Some(ExitIntent::Save);
                }
                ModalOutcome::Commit(SaveDiscardChoice::Discard) => {
                    editor.clear_modal_chain();
                    editor.exit_after_save = Some(ExitIntent::Discard);
                }
                ModalOutcome::Cancel => {
                    editor.clear_modal_chain();
                }
                ModalOutcome::Continue => {}
            }
        }
        // List-view modals; defensive cancel if one lands here.
        Modal::GithubPicker { .. } | Modal::RolePicker { .. } => {
            editor.clear_modal_chain();
        }
        Modal::RoleOverridePicker { state: picker } => {
            match picker.handle_key(key) {
                ModalOutcome::Commit(role) => {
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
                ModalOutcome::Cancel => {
                    editor.pop_modal_chain();
                }
                ModalOutcome::Continue => {}
            }
        }
        Modal::ConfirmSave { state: modal_state } => {
            use jackin_console::tui::components::confirm_save::SaveChoice;
            match modal_state.handle_key(key) {
                ModalOutcome::Commit(SaveChoice::Save) => {
                    // Confirming → PendingCommit atomically so plan +
                    // exit_on_success travel together to the outer
                    // handler that holds paths/cwd.
                    let plan = crate::console::tui::state::PendingSaveCommit {
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
                ModalOutcome::Cancel => {
                    editor.clear_modal_chain();
                    editor.save_flow = EditorSaveFlow::Idle;
                }
                ModalOutcome::Continue => {}
            }
        }
        Modal::ErrorPopup { state: popup_state } => match popup_state.handle_key(key) {
            ModalOutcome::Cancel | ModalOutcome::Commit(()) => {
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
                // into `pending_auth_form_return` instead of being
                // re-mounted directly — restore it now so the operator
                // lands back on the form with the prior credential
                // unchanged, ready to retry through the source picker.
                if !editor.modal_parents.is_empty() {
                    super::auth::restore_auth_form_after_op_picker_cancel(editor);
                }
            }
            ModalOutcome::Continue => {}
        },
        Modal::StatusPopup { .. } | Modal::ContainerInfo { .. } => {}
        Modal::ScopePicker { state: scope_state } => {
            use jackin_console::tui::components::scope_picker::ScopeChoice;
            match scope_state.handle_key(key) {
                ModalOutcome::Commit(ScopeChoice::AllAgents) => {
                    let scope = SecretsScopeTag::Workspace;
                    let state =
                        env_key_input_state(editor, &scope, secret_new_key_label(&scope), "");
                    editor.open_sub_modal(Modal::TextInput {
                        target: TextInputTarget::EnvKey { scope },
                        state,
                    });
                }
                ModalOutcome::Commit(ScopeChoice::SpecificAgent) => {
                    // Empty eligible set → `open_agent_override_picker`
                    // is a no-op; we close the modal then.
                    agents::open_agent_override_picker(editor, config);
                    if !matches!(editor.modal, Some(Modal::RoleOverridePicker { .. })) {
                        editor.clear_modal_chain();
                    }
                }
                ModalOutcome::Cancel => {
                    editor.pop_modal_chain();
                }
                ModalOutcome::Continue => {}
            }
        }
        Modal::SourcePicker {
            state: source,
            env_key,
        } => {
            use jackin_console::tui::components::source_picker::SourceChoice;
            match source.handle_key(key) {
                ModalOutcome::Commit(SourceChoice::Plain) => {
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
                ModalOutcome::Commit(SourceChoice::Op) => {
                    let Some((scope, key)) = env_key.take() else {
                        editor.clear_modal_chain();
                        return EditorModalOutcome::Continue;
                    };
                    editor.pending_picker_target = Some((scope, Some(key)));
                    // The env_key context now lives in the modal; no separate
                    // pending_env_key field to clear.
                    // env_key context now in Modal::SourcePicker
                    editor.open_sub_modal(Modal::OpPicker {
                        state: Box::new(OpPickerState::new_with_cache(op_cache)),
                    });
                }
                ModalOutcome::Cancel => {
                    // Cancel: drop the in-flight key name and close
                    // the modal. Operator returns to the Secrets tab
                    // with no env entry added.
                    editor.pop_modal_chain();
                    // env_key context now in Modal::SourcePicker
                    editor.pending_picker_value = None;
                }
                ModalOutcome::Continue => {}
            }
        }
        Modal::AuthSourcePicker { state: source } => {
            use jackin_console::tui::components::source_picker::SourceChoice;
            let outcome = source.handle_key(key);
            // Generate wins over the provide dispatch: the `g`/`G` trigger
            // sets `generating_token_target` (and stashes the form into
            // `pending_auth_form_return` for the post-mint re-mount), so
            // the generate branch is reachable only on that path and the
            // provide arms below stay untouched.
            if editor.generating_token_target.is_some() {
                match outcome {
                    ModalOutcome::Commit(SourceChoice::Plain) => {
                        start_plain_token_generate(editor);
                    }
                    ModalOutcome::Commit(SourceChoice::Op) => {
                        open_create_op_picker_for_generate(editor, op_cache);
                    }
                    // Cancel before minting: restore the stashed form so
                    // the operator lands back on the Edit-auth dialog
                    // unchanged (matches the provide-path source-picker
                    // cancel below).
                    ModalOutcome::Cancel => {
                        editor.generating_token_target = None;
                        super::auth::restore_auth_form_after_op_picker_cancel(editor);
                    }
                    ModalOutcome::Continue => {}
                }
                return EditorModalOutcome::Continue;
            }
            match outcome {
                ModalOutcome::Commit(SourceChoice::Plain) => {
                    super::auth::apply_plain_source_picker_to_auth_form(editor);
                }
                ModalOutcome::Commit(SourceChoice::Op) => {
                    super::auth::open_op_picker_from_auth_source(editor, op_cache);
                }
                ModalOutcome::Cancel => {
                    super::auth::restore_auth_form_after_op_picker_cancel(editor);
                }
                ModalOutcome::Continue => {}
            }
        }
        Modal::AuthForm { .. } => {
            super::auth::handle_auth_form_key(editor, key, op_available);
        }
        Modal::AuthRolePicker { state: picker } => match picker.handle_key(key) {
            ModalOutcome::Commit(role) => {
                if let Some(kind) = editor.auth_selected_kind {
                    let target = crate::console::tui::state::AuthFormTarget::WorkspaceRole {
                        role: role.key(),
                        kind,
                    };
                    let form = crate::console::tui::state::AuthForm::new(kind);
                    editor.open_sub_modal(Modal::AuthForm {
                        target,
                        state: Box::new(form),
                        focus: crate::console::tui::state::AuthFormFocus::Mode,
                        literal_buffer: String::new(),
                    });
                } else {
                    editor.pop_modal_chain();
                }
            }
            ModalOutcome::Cancel => {
                editor.pop_modal_chain();
            }
            ModalOutcome::Continue => {}
        },
        Modal::OpPicker { state: picker } => {
            let outcome = picker.handle_key(key);
            // Token-generate wins over both browse and provide dispatch:
            // `generating_token_target` is set exactly when the picker was
            // opened by the auth-form `g`/`G` trigger (Create mode), so the
            // create variants are reachable only on this path.
            if let Some(target) = editor.generating_token_target.take() {
                handle_token_generate_pick(editor, target, outcome);
                return EditorModalOutcome::Continue;
            }
            match outcome {
                // Browse-mode caller: only `Existing` is reachable.
                ModalOutcome::Commit(
                    crate::console::tui::op_picker::OpPickerSelection::NewItem { .. }
                    | crate::console::tui::op_picker::OpPickerSelection::EditItemField { .. },
                ) => unreachable!("Secrets-tab OpPicker runs in Browse mode"),
                ModalOutcome::Commit(
                    crate::console::tui::op_picker::OpPickerSelection::Existing(op_ref),
                ) => {
                    // Auth-form round trip wins over the Secrets-tab
                    // dispatch: the auth form sets
                    // `pending_auth_form_return` exactly when it's the
                    // caller, so the two paths can never collide.
                    if !editor.modal_parents.is_empty() {
                        // Close the OpPicker — the auth form stays stashed on
                        // modal_parents so the _committed / _failed helpers find it.
                        editor.modal = None;
                        return EditorModalOutcome::ValidateOpRef(op_ref);
                    }
                    // Operator picked a Vault → Item → Field path. The
                    // dispatch depends on whether `P` was pressed on a
                    // key row (write directly) or on an `+ Add` sentinel
                    // (stash the OpRef, ask for the key name first).
                    let target = editor.pending_picker_target.take();
                    match target {
                        Some((scope, Some(key))) => {
                            set_pending_env_op_ref(editor, &scope, &key, op_ref);
                            editor.clear_modal_chain();
                        }
                        Some((scope, None)) => {
                            editor.pending_picker_value =
                                Some(jackin_core::EnvValue::OpRef(op_ref));
                            let label = secret_new_key_after_picker_label(&scope);
                            let state = env_key_input_state(editor, &scope, label, "");
                            editor.open_sub_modal(Modal::TextInput {
                                target: TextInputTarget::EnvKey { scope },
                                state,
                            });
                        }
                        None => {
                            editor.clear_modal_chain();
                        }
                    }
                }
                ModalOutcome::Cancel => {
                    // Auth-form round trip: re-mount the form
                    // unchanged. Mirrors the Commit branch — the two
                    // callers (Secrets-tab `P`, auth-form Enter) are
                    // disambiguated by `pending_auth_form_return`.
                    if !editor.modal_parents.is_empty() {
                        super::auth::restore_auth_form_after_op_picker_cancel(editor);
                        return EditorModalOutcome::Continue;
                    }
                    // Clear both scratch fields so a stale path/target
                    // can't carry into a later interaction.
                    editor.pop_modal_chain();
                    editor.pending_picker_target = None;
                    editor.pending_picker_value = None;
                }
                ModalOutcome::Continue => {}
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
    match crate::console::domain::resolve_role_input_source(config, value) {
        Ok(resolved) => EditorModalOutcome::StartRoleRegistration {
            raw: resolved.raw,
            key: resolved.key,
            selector: resolved.selector,
            source: resolved.source,
        },
        Err(e) => {
            let err_text = e.error.to_string();
            if let Some(panic_message) = err_text.strip_prefix("role loader panicked: ") {
                let message =
                    jackin_console::tui::components::error_popup::internal_role_load_error_message(
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

#[cfg(test)]
fn poll_role_load(
    editor: &mut EditorState<'_>,
    config: &mut AppConfig,
    paths: &JackinPaths,
) -> bool {
    let Some((load, result)) = poll_role_load_completion(editor) else {
        return false;
    };
    crate::console::effects::apply_role_load_completion(editor, config, paths, load, result);
    true
}

#[cfg(test)]
fn poll_role_load_completion(
    editor: &mut EditorState<'_>,
) -> Option<(PendingRoleLoad, anyhow::Result<()>)> {
    let load = editor.pending_role_load.as_mut()?;
    let result = match load.rx.poll_next() {
        SubscriptionPoll::Ready(result) => result,
        SubscriptionPoll::Pending => return None,
        SubscriptionPoll::Closed => Err(anyhow::anyhow!(
            jackin_console::tui::subscriptions::role_loader_worker_disconnected_message()
        )),
    };
    let load = editor
        .pending_role_load
        .take()
        .expect("pending role load checked above");
    Some((load, result))
}

fn apply_editor_confirm(
    editor: &mut EditorState<'_>,
    target: &ConfirmTarget,
) -> anyhow::Result<EditorModalOutcome> {
    match target {
        ConfirmTarget::DeleteEnvVar { scope, key } => {
            // CLAUDE_CODE_OAUTH_TOKEN under oauth_token mode is owned by the
            // claude-token orchestrator; an unset here would silently break
            // auth at the next launch.
            let protected = key == jackin_env::CLAUDE_OAUTH_TOKEN_ENV
                && matches!(scope, SecretsScopeTag::Workspace)
                && editor.pending.claude.as_ref().map(|c| c.auth_forward)
                    == Some(jackin_config::AuthForwardMode::OAuthToken);
            if protected {
                anyhow::bail!(
                    "CLAUDE_CODE_OAUTH_TOKEN is managed by `jackin workspace claude-token` \
                     — use `jackin workspace claude-token revoke <workspace>` to clear it"
                );
            }
            match scope {
                SecretsScopeTag::Workspace => {
                    editor.pending.env.remove(key);
                }
                SecretsScopeTag::Role(role) => {
                    let mut drop_agent = false;
                    if let Some(ov) = editor.pending.roles.get_mut(role) {
                        ov.env.remove(key);
                        // Drop empty override so change_count reports
                        // clean when the role's overrides are later
                        // re-added.
                        if ov.env.is_empty() {
                            drop_agent = true;
                        }
                    }
                    if drop_agent {
                        editor.pending.roles.remove(role);
                    }
                }
            }
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
    outcome: &ModalOutcome<jackin_console::tui::components::mount_dst_choice::MountDstChoice>,
) {
    match mount_dst_choice_plan(outcome.clone()) {
        MountDstChoicePlan::CommitSamePath => {
            if target == FileBrowserTarget::EditAddMountSrc {
                editor.pending.mounts.push(
                    jackin_console::services::workspace::shared_mount_config(src, src, false),
                );
            }
            editor.clear_modal_chain();
        }
        MountDstChoicePlan::OpenEditInput => {
            if target == FileBrowserTarget::EditAddMountSrc {
                editor.pending.mounts.push(
                    jackin_console::services::workspace::shared_mount_config(src, src, false),
                );
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

pub(in crate::console) fn apply_file_browser_to_editor(
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

#[cfg(test)]
mod tests;
