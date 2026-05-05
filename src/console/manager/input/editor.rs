//! Editor-stage dispatch: tab navigation, field focus, per-tab key
//! handling, and the editor-level modal dispatcher.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::super::super::widgets::{
    ModalOutcome, file_browser::FileBrowserState, op_picker::OpPickerState,
    workdir_pick::WorkdirPickState,
};
use super::super::render::editor::{SecretsRow, secrets_flat_rows};
use super::super::state::{
    ConfirmTarget, EditorMode, EditorSaveFlow, EditorState, EditorTab, ExitIntent, FieldFocus,
    FileBrowserTarget, ManagerStage, ManagerState, Modal, SecretsScopeTag, TextInputTarget, Toast,
    ToastKind,
};
use super::InputOutcome;
use crate::config::AppConfig;
use crate::paths::JackinPaths;

// Central keymap dispatch — table-like layout makes the keymap
// readable at a glance; extracting per-key helpers just scatters it.
#[allow(clippy::too_many_lines)]
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
            let _ = paths;
            return Ok(InputOutcome::Continue);
        }
        KeyCode::Esc => {
            if let ManagerStage::Editor(editor) = &state.stage {
                let dirty = editor.is_dirty();
                if dirty {
                    if let ManagerStage::Editor(editor) = &mut state.stage {
                        editor.modal = Some(Modal::SaveDiscardCancel {
                            state: crate::console::widgets::save_discard::SaveDiscardState::new(
                                "Save changes before leaving?",
                            ),
                        });
                    }
                } else {
                    let cache = state.op_cache.clone();
                    let op_available = state.op_available;
                    *state = ManagerState::from_config_with_cache_and_op(
                        config,
                        cwd,
                        cache,
                        op_available,
                    );
                }
            }
            return Ok(InputOutcome::Continue);
        }
        _ => {}
    }

    // Capture before the editor borrow (separate fields, but explicit is cleaner).
    let op_cache = state.op_cache.clone();
    let op_available = state.op_available;

    let ManagerStage::Editor(editor) = &mut state.stage else {
        return Ok(InputOutcome::Continue);
    };

    match key.code {
        KeyCode::Tab | KeyCode::Right => {
            // Secrets tab `AgentHeader` absorbs `→` in both states
            // (expand or no-op) — falling through to tab-cycle on an
            // expanded header would surprise the operator. See
            // RULES.md "TUI Keybindings → Contextual key absorption".
            // `Tab` never absorbs.
            if key.code == KeyCode::Right && editor.active_tab == EditorTab::Secrets {
                let FieldFocus::Row(n) = editor.active_field;
                let rows = secrets_flat_rows(editor);
                if let Some(SecretsRow::RoleHeader { role, expanded }) = rows.get(n).cloned() {
                    if !expanded {
                        editor.secrets_expanded.insert(role);
                    }
                    return Ok(InputOutcome::Continue);
                }
            }
            let was_secrets = editor.active_tab == EditorTab::Secrets;
            editor.active_tab = match editor.active_tab {
                EditorTab::General => EditorTab::Mounts,
                EditorTab::Mounts => EditorTab::Roles,
                EditorTab::Roles => EditorTab::Secrets,
                EditorTab::Secrets => EditorTab::General,
            };
            editor.active_field = FieldFocus::Row(0);
            if was_secrets {
                reset_secrets_view(editor);
            }
        }
        KeyCode::Left => {
            // Mirror of Tab/Right above — `AgentHeader` absorbs `←`
            // in both states (collapse or no-op).
            if editor.active_tab == EditorTab::Secrets {
                let FieldFocus::Row(n) = editor.active_field;
                let rows = secrets_flat_rows(editor);
                if let Some(SecretsRow::RoleHeader { role, expanded }) = rows.get(n).cloned() {
                    if expanded {
                        editor.secrets_expanded.remove(&role);
                    }
                    return Ok(InputOutcome::Continue);
                }
            }
            let was_secrets = editor.active_tab == EditorTab::Secrets;
            editor.active_tab = match editor.active_tab {
                EditorTab::General => EditorTab::Secrets,
                EditorTab::Mounts => EditorTab::General,
                EditorTab::Roles => EditorTab::Mounts,
                EditorTab::Secrets => EditorTab::Roles,
            };
            editor.active_field = FieldFocus::Row(0);
            if was_secrets {
                reset_secrets_view(editor);
            }
        }
        KeyCode::Up | KeyCode::Char('k' | 'K') => {
            let FieldFocus::Row(n) = editor.active_field;
            let candidate = n.saturating_sub(1);
            // Skip Secrets-tab spacer rows so the cursor never lands
            // on a blank line.
            let next = if editor.active_tab == EditorTab::Secrets {
                let rows = secrets_flat_rows(editor);
                step_secrets_cursor_up(&rows, candidate)
            } else {
                candidate
            };
            editor.active_field = FieldFocus::Row(next);
        }
        KeyCode::Down | KeyCode::Char('j' | 'J') => {
            let FieldFocus::Row(n) = editor.active_field;
            if editor.active_tab == EditorTab::Secrets {
                let rows = secrets_flat_rows(editor);
                let max = rows.len().saturating_sub(1);
                let candidate = (n + 1).min(max);
                editor.active_field =
                    FieldFocus::Row(step_secrets_cursor_down(&rows, candidate, max));
            } else {
                let max = max_row_for_tab(editor, config);
                editor.active_field = FieldFocus::Row((n + 1).min(max));
            }
        }
        KeyCode::Enter => match editor.active_tab {
            EditorTab::General => open_editor_field_modal(editor),
            EditorTab::Mounts => {
                let FieldFocus::Row(n) = editor.active_field;
                if n == editor.pending.mounts.len() {
                    editor.modal = Some(Modal::FileBrowser {
                        target: FileBrowserTarget::EditAddMountSrc,
                        state: FileBrowserState::new_from_home()?,
                    });
                }
            }
            EditorTab::Secrets => {
                open_secrets_enter_modal(editor);
            }
            EditorTab::Roles => {
                let FieldFocus::Row(n) = editor.active_field;
                if n == config.roles.len() {
                    open_role_input(editor, config);
                }
            }
        },
        KeyCode::Char('a' | 'A') if editor.active_tab == EditorTab::Roles => {
            open_role_input(editor, config);
        }
        KeyCode::Char(' ') if editor.active_tab == EditorTab::Roles => {
            toggle_agent_allowed_at_cursor(editor, config);
        }
        KeyCode::Char(' ') if editor.active_tab == EditorTab::General => {
            // Row 2 is the keep_awake toggle. Other General rows
            // ignore Space — Enter is the modal-opening key for them.
            let FieldFocus::Row(n) = editor.active_field;
            if n == 2 {
                editor.pending.keep_awake.enabled = !editor.pending.keep_awake.enabled;
            }
        }
        KeyCode::Char('*') if editor.active_tab == EditorTab::Roles => {
            toggle_default_agent_at_cursor(editor, config);
        }
        KeyCode::Char('a' | 'A') if editor.active_tab == EditorTab::Mounts => {
            editor.modal = Some(Modal::FileBrowser {
                target: FileBrowserTarget::EditAddMountSrc,
                state: FileBrowserState::new_from_home()?,
            });
        }
        KeyCode::Char('d' | 'D') if editor.active_tab == EditorTab::Mounts => {
            remove_mount_at_cursor(editor);
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
        KeyCode::Char('m' | 'M')
            if editor.active_tab == EditorTab::Secrets
                && (key.modifiers - KeyModifiers::SHIFT).is_empty() =>
        {
            toggle_focused_row_mask(editor);
        }
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
            open_secrets_delete_confirm(editor);
        }
        KeyCode::Char('a' | 'A')
            if editor.active_tab == EditorTab::Secrets
                && (key.modifiers - KeyModifiers::SHIFT).is_empty() =>
        {
            open_secrets_add_modal(editor);
        }
        KeyCode::Char('r' | 'R') if editor.active_tab == super::super::state::EditorTab::Mounts => {
            let FieldFocus::Row(n) = editor.active_field;
            if let Some(m) = editor.pending.mounts.get_mut(n) {
                m.readonly = !m.readonly;
            }
        }
        KeyCode::Char('i' | 'I') if editor.active_tab == super::super::state::EditorTab::Mounts => {
            // Cycle the per-mount isolation strategy on the highlighted row.
            // Mirrors the R (readonly) toggle but threads through the
            // dedicated state helper so the cycling rule lives in one place.
            // Silent no-op on the `+ Add mount` sentinel.
            editor.cycle_isolation_for_selected_mount();
        }
        KeyCode::Char('o' | 'O') if editor.active_tab == super::super::state::EditorTab::Mounts => {
            // Open in browser; toast for non-GitHub mounts so the
            // binding stays discoverable.
            let FieldFocus::Row(n) = editor.active_field;
            if let Some(m) = editor.pending.mounts.get(n) {
                let kind = super::super::mount_info::inspect(&m.src);
                match kind {
                    super::super::mount_info::MountKind::Git {
                        origin: Some(super::super::mount_info::GitOrigin::Github { web_url, .. }),
                        ..
                    } => {
                        if let Err(e) = open::that_detached(&web_url) {
                            state.toast = Some(Toast {
                                message: format!("failed to open URL: {e}"),
                                kind: ToastKind::Error,
                                shown_at: std::time::Instant::now(),
                            });
                        }
                    }
                    super::super::mount_info::MountKind::Git { .. }
                    | super::super::mount_info::MountKind::Folder
                    | super::super::mount_info::MountKind::Missing => {
                        state.toast = Some(Toast {
                            message: "no GitHub URL for this mount".into(),
                            kind: ToastKind::Error,
                            shown_at: std::time::Instant::now(),
                        });
                    }
                }
            }
        }
        _ => {}
    }
    Ok(InputOutcome::Continue)
}

fn max_row_for_tab(editor: &EditorState<'_>, config: &AppConfig) -> usize {
    match editor.active_tab {
        // 0=Name, 1=Working dir, 2=Keep awake
        EditorTab::General => 2,
        EditorTab::Mounts => editor.pending.mounts.len(),
        // One extra sentinel row: + Add role.
        EditorTab::Roles => config.roles.len(),
        // Secrets tab is handled inline in the Down key arm; never reached here.
        EditorTab::Secrets => 0,
    }
}

/// Walks forward past spacer rows. Defensive fallback to `candidate`
/// if every row through `max` is a spacer (currently impossible).
fn step_secrets_cursor_down(
    rows: &[super::super::render::editor::SecretsRow],
    candidate: usize,
    max: usize,
) -> usize {
    use super::super::render::editor::SecretsRow;
    let mut idx = candidate;
    while idx <= max {
        match rows.get(idx) {
            Some(SecretsRow::SectionSpacer) => idx += 1,
            _ => return idx,
        }
    }
    candidate
}

/// Walks backward past spacers; index 0 is always focusable.
fn step_secrets_cursor_up(
    rows: &[super::super::render::editor::SecretsRow],
    candidate: usize,
) -> usize {
    use super::super::render::editor::SecretsRow;
    let mut idx = candidate;
    loop {
        match rows.get(idx) {
            Some(SecretsRow::SectionSpacer) => {
                if idx == 0 {
                    return 0;
                }
                idx -= 1;
            }
            _ => return idx,
        }
    }
}

fn reset_secrets_view(editor: &mut EditorState<'_>) {
    editor.unmasked_rows.clear();
    editor.secrets_expanded.clear();
}

/// No-op on header/sentinel/op:// rows.
fn toggle_focused_row_mask(editor: &mut EditorState<'_>) {
    let FieldFocus::Row(n) = editor.active_field;
    let rows = secrets_flat_rows(editor);
    let Some(row) = rows.get(n).cloned() else {
        return;
    };
    let key = match row {
        SecretsRow::WorkspaceKeyRow(key) => {
            // OpRef rows render as breadcrumbs and ignore mask state.
            if editor
                .pending
                .env
                .get(&key)
                .is_some_and(|v| matches!(v, crate::operator_env::EnvValue::OpRef(_)))
            {
                return;
            }
            (SecretsScopeTag::Workspace, key)
        }
        SecretsRow::RoleKeyRow { role, key } => {
            if editor
                .pending
                .roles
                .get(&role)
                .and_then(|o| o.env.get(&key))
                .is_some_and(|v| matches!(v, crate::operator_env::EnvValue::OpRef(_)))
            {
                return;
            }
            (SecretsScopeTag::Role(role), key)
        }
        _ => return,
    };
    if !editor.unmasked_rows.remove(&key) {
        editor.unmasked_rows.insert(key);
    }
}

fn open_editor_field_modal(editor: &mut EditorState<'_>) {
    use super::super::super::widgets::text_input::TextInputState;
    if editor.active_tab == EditorTab::General {
        let FieldFocus::Row(n) = editor.active_field;
        match n {
            0 => {
                let current = match &editor.mode {
                    EditorMode::Edit { name } => {
                        editor.pending_name.clone().unwrap_or_else(|| name.clone())
                    }
                    EditorMode::Create => editor.pending_name.clone().unwrap_or_default(),
                };
                editor.modal = Some(Modal::TextInput {
                    target: TextInputTarget::Name,
                    state: TextInputState::new("Rename workspace", current),
                });
            }
            1 if !editor.pending.mounts.is_empty() => {
                editor.modal = Some(Modal::WorkdirPick {
                    state: WorkdirPickState::from_mounts(&editor.pending.mounts),
                });
            }
            _ => {}
        }
    }
}

fn open_secrets_enter_modal(editor: &mut EditorState<'_>) {
    use super::super::super::widgets::text_input::TextInputState;
    let FieldFocus::Row(n) = editor.active_field;
    let rows = secrets_flat_rows(editor);
    let Some(row) = rows.get(n).cloned() else {
        return;
    };
    match row {
        SecretsRow::WorkspaceKeyRow(key) => {
            // OpRef rows are not text-editable — operator deletes via
            // D and re-adds via the source picker.
            if editor
                .pending
                .env
                .get(&key)
                .is_some_and(|v| matches!(v, crate::operator_env::EnvValue::OpRef(_)))
            {
                return;
            }
            let current = editor
                .pending
                .env
                .get(&key)
                .map(|v| v.as_persisted_str().to_string())
                .unwrap_or_default();
            editor.modal = Some(Modal::TextInput {
                target: TextInputTarget::EnvValue {
                    scope: SecretsScopeTag::Workspace,
                    key: key.clone(),
                },
                state: TextInputState::new_allow_empty(format!("Edit {key}"), current),
            });
        }
        SecretsRow::WorkspaceAddSentinel => {
            // Workspace sentinel asks the scope question first; the
            // per-role sentinel fast-path stays direct.
            use crate::console::widgets::scope_picker::ScopePickerState;
            editor.modal = Some(Modal::ScopePicker {
                state: ScopePickerState::new(),
            });
        }
        SecretsRow::RoleHeader { role, expanded } => {
            if !expanded {
                editor.secrets_expanded.insert(role);
            }
        }
        SecretsRow::RoleKeyRow { role, key } => {
            if editor
                .pending
                .roles
                .get(&role)
                .and_then(|o| o.env.get(&key))
                .is_some_and(|v| matches!(v, crate::operator_env::EnvValue::OpRef(_)))
            {
                return;
            }
            let current = editor
                .pending
                .roles
                .get(&role)
                .and_then(|o| o.env.get(&key))
                .map(|v| v.as_persisted_str().to_string())
                .unwrap_or_default();
            let label = format!("Edit {key}");
            editor.modal = Some(Modal::TextInput {
                target: TextInputTarget::EnvValue {
                    scope: SecretsScopeTag::Role(role),
                    key,
                },
                state: TextInputState::new_allow_empty(label, current),
            });
        }
        SecretsRow::RoleAddSentinel(role) => {
            // In-section fast-path — already viewing the role, don't
            // re-ask the scope question.
            let label = format!("New {role} environment key");
            let scope = SecretsScopeTag::Role(role);
            let state = env_key_input_state(editor, &scope, label, String::new());
            editor.modal = Some(Modal::TextInput {
                target: TextInputTarget::EnvKey { scope },
                state,
            });
        }
        // Spacer rows are skipped on `↑`/`↓`; defensive no-op.
        SecretsRow::SectionSpacer => {}
    }
}

/// Listing rules: workspace-allowed list when non-empty, otherwise
/// every role in `config.roles`. Roles already carrying an
/// override are NOT filtered out — operator may want to add more
/// keys.
fn open_agent_override_picker(editor: &mut EditorState<'_>, config: &AppConfig) {
    use super::super::super::widgets::role_picker::RolePickerState;
    use crate::selector::RoleSelector;
    let eligible: Vec<RoleSelector> =
        super::super::render::editor::eligible_agents_for_override(editor, config)
            .into_iter()
            .filter_map(|name| RoleSelector::parse(&name).ok())
            .collect();
    if eligible.is_empty() {
        return;
    }
    editor.modal = Some(Modal::RoleOverridePicker {
        state: RolePickerState::with_confirm_label(eligible, "select"),
    });
}

fn open_role_input(editor: &mut EditorState<'_>, config: &AppConfig) {
    use super::super::super::widgets::text_input::TextInputState;

    let trusted_roles = config
        .roles
        .iter()
        .filter(|(_, source)| source.trusted)
        .map(|(key, _)| key.clone())
        .collect();
    let mut state = TextInputState::new_with_forbidden("Add role", "", trusted_roles);
    state.forbidden_label = "trusted role registry".into();
    editor.modal = Some(Modal::TextInput {
        target: TextInputTarget::Role,
        state,
    });
}

fn open_secrets_delete_confirm(editor: &mut EditorState<'_>) {
    use crate::console::widgets::confirm::ConfirmState;
    let FieldFocus::Row(n) = editor.active_field;
    let rows = secrets_flat_rows(editor);
    let Some(row) = rows.get(n).cloned() else {
        return;
    };
    let (scope, key) = match row {
        SecretsRow::WorkspaceKeyRow(key) => (SecretsScopeTag::Workspace, key),
        SecretsRow::RoleKeyRow { role, key } => (SecretsScopeTag::Role(role), key),
        _ => return,
    };
    let prompt = format!("Delete environment variable {key}?");
    editor.modal = Some(Modal::Confirm {
        target: ConfirmTarget::DeleteEnvVar { scope, key },
        state: ConfirmState::new(prompt),
    });
}

/// `A` commits to the row's contextual scope without asking — unlike
/// the workspace-sentinel `Enter` path, which routes through
/// `ScopePicker`. Operator already chose a row with unambiguous
/// scope; an extra prompt would be a regression.
fn open_secrets_add_modal(editor: &mut EditorState<'_>) {
    let FieldFocus::Row(n) = editor.active_field;
    let rows = secrets_flat_rows(editor);
    let Some(row) = rows.get(n).cloned() else {
        return;
    };
    let (scope, label) = match row {
        SecretsRow::WorkspaceKeyRow(_) | SecretsRow::WorkspaceAddSentinel => (
            SecretsScopeTag::Workspace,
            "New workspace environment key".to_string(),
        ),
        SecretsRow::RoleHeader { role, .. }
        | SecretsRow::RoleKeyRow { role, .. }
        | SecretsRow::RoleAddSentinel(role) => (
            SecretsScopeTag::Role(role.clone()),
            format!("New {role} environment key"),
        ),
        // Cursor never lands on `SectionSpacer` (skipped on `↑`/`↓`),
        // but keep the match exhaustive — silently no-op on the
        // pathological case.
        SecretsRow::SectionSpacer => return,
    };
    let state = env_key_input_state(editor, &scope, label, String::new());
    editor.modal = Some(Modal::TextInput {
        target: TextInputTarget::EnvKey { scope },
        state,
    });
}

/// Space on an role row toggles its **effective** allow-state.
///
/// The underlying data model uses an "empty list = all allowed" shorthand,
/// so the checkbox on each row must reflect
/// `list.is_empty() || list.contains(name)`. The toggle preserves that
/// invariant in both directions:
///
/// - **Effective-allowed + empty list** (in "all" mode): populate the list
///   with every role *except* this one. Status flips to
///   `custom (total-1 of total)`; the row flips to `[ ]`.
/// - **Effective-allowed + non-empty list** (row is in the list): remove it.
///   An empty remainder is left empty (semantically = "all"); otherwise
///   stays `custom`. The row flips to `[ ]`.
/// - **Effective-blocked** (row not in list): add the name. If the list now
///   contains every role in `config.roles`, clear it back to empty
///   (= "all"). Otherwise stays `custom`. The row flips to `[x]`.
fn toggle_agent_allowed_at_cursor(editor: &mut EditorState<'_>, config: &AppConfig) {
    let FieldFocus::Row(n) = editor.active_field;
    // n is 0-based into config.roles (no header offset).
    let agent_names: Vec<String> = config.roles.keys().cloned().collect();
    let Some(role) = agent_names.get(n) else {
        return;
    };

    // Read "all" state before the mutable borrow on `allowed_roles`.
    let is_all_mode = super::super::agent_allow::allows_all_agents(&editor.pending);
    let list = &mut editor.pending.allowed_roles;
    let in_list = list.iter().position(|a| a == role);

    if is_all_mode {
        // Demote "all" to "custom" without this row by enumerating
        // the full roster minus the current role.
        *list = agent_names
            .iter()
            .filter(|a| a.as_str() != role.as_str())
            .cloned()
            .collect();
        if editor.pending.default_role.as_deref() == Some(role.as_str()) {
            editor.pending.default_role = None;
        }
    } else if let Some(pos) = in_list {
        list.remove(pos);
        if editor.pending.default_role.as_deref() == Some(role.as_str()) {
            editor.pending.default_role = None;
        }
    } else {
        // Filling in the full roster collapses back to the "all"
        // shorthand so the badge reads `all` rather than
        // `custom (N of N)`.
        list.push(role.clone());
        if list.len() == agent_names.len() && agent_names.iter().all(|a| list.contains(a)) {
            list.clear();
        }
    }
}

/// On the current default → clear; on allowed → set; on disallowed
/// → no-op (operator must `Space` to allow first).
fn toggle_default_agent_at_cursor(editor: &mut EditorState<'_>, config: &AppConfig) {
    let FieldFocus::Row(n) = editor.active_field;
    let agent_names: Vec<String> = config.roles.keys().cloned().collect();
    let Some(role) = agent_names.get(n) else {
        return;
    };

    if editor.pending.default_role.as_deref() == Some(role.as_str()) {
        editor.pending.default_role = None;
        return;
    }

    if !super::super::agent_allow::agent_is_effectively_allowed(&editor.pending, role) {
        return;
    }

    editor.pending.default_role = Some(role.clone());
}

fn remove_mount_at_cursor(editor: &mut EditorState<'_>) {
    let FieldFocus::Row(n) = editor.active_field;
    if n < editor.pending.mounts.len() {
        editor.pending.mounts.remove(n);
    }
}

#[allow(clippy::too_many_lines, clippy::needless_pass_by_value)]
pub(super) fn handle_editor_modal(
    editor: &mut EditorState<'_>,
    key: KeyEvent,
    op_available: bool,
    op_cache: std::rc::Rc<std::cell::RefCell<crate::console::op_cache::OpCache>>,
    config: &mut AppConfig,
    paths: &JackinPaths,
) {
    let Some(modal) = editor.modal.as_mut() else {
        return;
    };
    match modal {
        Modal::TextInput { target, state } => {
            match state.handle_key(key) {
                ModalOutcome::Commit(value) => {
                    let target = target.clone();
                    editor.modal = None;
                    if target == TextInputTarget::Role {
                        apply_role_input(editor, config, paths, &value);
                    } else {
                        apply_text_input_to_pending(&target, editor, &value, op_available);
                    }
                }
                ModalOutcome::Cancel => {
                    // Cancel of EnvKey/EnvValue must drop both the
                    // stashed key and any picker value — otherwise a
                    // later sentinel-picker commit silently applies
                    // the path to an unrelated key.
                    if let TextInputTarget::EnvKey { .. } | TextInputTarget::EnvValue { .. } =
                        target
                    {
                        editor.pending_env_key = None;
                        editor.pending_picker_value = None;
                    }
                    editor.modal = None;
                }
                ModalOutcome::Continue => {}
            }
        }
        Modal::FileBrowser { target, state } => match state.handle_key(key) {
            ModalOutcome::Commit(path) => {
                let target = *target;
                editor.modal = None;
                apply_file_browser_to_editor(target, editor, path);
            }
            ModalOutcome::Cancel => {
                editor.modal = None;
            }
            ModalOutcome::Continue => {}
        },
        Modal::WorkdirPick { state } => match state.handle_key(key) {
            ModalOutcome::Commit(workdir) => {
                editor.pending.workdir = workdir;
                editor.modal = None;
            }
            ModalOutcome::Cancel => {
                editor.modal = None;
            }
            ModalOutcome::Continue => {}
        },
        Modal::Confirm { target, state } => match state.handle_key(key) {
            ModalOutcome::Commit(yes) => {
                let target = target.clone();
                editor.modal = None;
                if yes {
                    // Source-drift acknowledgement consumes `plan` and
                    // re-stashes it as a `PendingCommit` for the outer
                    // dispatcher (which owns `paths` / `cwd` / `runner`)
                    // to drain via `commit_editor_save`.
                    if let super::super::state::ConfirmTarget::DeleteIsolatedAndSave {
                        mut plan,
                        exit_on_success,
                        ..
                    } = target
                    {
                        plan.delete_isolated_acknowledged = true;
                        editor.save_flow = EditorSaveFlow::PendingCommit {
                            plan,
                            exit_on_success,
                        };
                    } else if let Err(e) = apply_editor_confirm(editor, &target, config, paths) {
                        open_editor_action_error(editor, &e);
                    }
                } else if matches!(
                    target,
                    super::super::state::ConfirmTarget::DeleteIsolatedAndSave { .. }
                ) {
                    editor.save_flow = EditorSaveFlow::Idle;
                }
            }
            ModalOutcome::Cancel => {
                let was_drift = matches!(
                    target,
                    super::super::state::ConfirmTarget::DeleteIsolatedAndSave { .. }
                );
                editor.modal = None;
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
            let target = *target;
            let src = modal_state.src.clone();
            let outcome = modal_state.handle_key(key);
            dispatch_editor_mount_dst_choice(editor, target, &src, &outcome);
        }
        Modal::SaveDiscardCancel { state: modal_state } => {
            use crate::console::widgets::save_discard::SaveDiscardChoice;
            match modal_state.handle_key(key) {
                ModalOutcome::Commit(SaveDiscardChoice::Save) => {
                    editor.modal = None;
                    editor.exit_after_save = Some(ExitIntent::Save);
                }
                ModalOutcome::Commit(SaveDiscardChoice::Discard) => {
                    editor.modal = None;
                    editor.exit_after_save = Some(ExitIntent::Discard);
                }
                ModalOutcome::Cancel => {
                    editor.modal = None;
                }
                ModalOutcome::Continue => {}
            }
        }
        // List-view modals; defensive cancel if one lands here.
        Modal::GithubPicker { .. } | Modal::RolePicker { .. } => {
            editor.modal = None;
        }
        Modal::RoleOverridePicker { state: picker } => {
            match picker.handle_key(key) {
                ModalOutcome::Commit(role) => {
                    // The override section materializes organically on
                    // the first value commit; we don't touch
                    // `pending.roles` here, so a cancel mid-flow leaves
                    // no empty placeholder.
                    let role_name = role.key();
                    let scope = SecretsScopeTag::Role(role_name.clone());
                    let label = format!("New {role_name} environment key");
                    let state = env_key_input_state(editor, &scope, label, "");
                    editor.modal = Some(Modal::TextInput {
                        target: TextInputTarget::EnvKey { scope },
                        state,
                    });
                }
                ModalOutcome::Cancel => {
                    editor.modal = None;
                }
                ModalOutcome::Continue => {}
            }
        }
        Modal::ConfirmSave { state: modal_state } => {
            use crate::console::widgets::confirm_save::SaveChoice;
            match modal_state.handle_key(key) {
                ModalOutcome::Commit(SaveChoice::Save) => {
                    // Confirming → PendingCommit atomically so plan +
                    // exit_on_success travel together to the outer
                    // handler that holds paths/cwd.
                    let plan = super::super::state::PendingSaveCommit {
                        effective_removals: modal_state.effective_removals.clone(),
                        final_mounts: modal_state.final_mounts.clone(),
                        // First commit pass — the drift check in
                        // `commit_editor_save` runs unconditionally. The
                        // `DeleteIsolatedAndSave` confirm modal is what
                        // re-stashes the plan with the flag flipped to
                        // `true` so the second pass skips the check.
                        delete_isolated_acknowledged: false,
                    };
                    let exit_on_success = matches!(
                        editor.save_flow,
                        EditorSaveFlow::Confirming {
                            exit_on_success: true
                        }
                    );
                    editor.modal = None;
                    editor.save_flow = EditorSaveFlow::PendingCommit {
                        plan,
                        exit_on_success,
                    };
                }
                ModalOutcome::Cancel => {
                    editor.modal = None;
                    editor.save_flow = EditorSaveFlow::Idle;
                }
                ModalOutcome::Continue => {}
            }
        }
        Modal::ErrorPopup { state: popup_state } => match popup_state.handle_key(key) {
            ModalOutcome::Cancel | ModalOutcome::Commit(()) => {
                editor.modal = None;
                editor.save_flow = EditorSaveFlow::Idle;
            }
            ModalOutcome::Continue => {}
        },
        Modal::ScopePicker { state: scope_state } => {
            use crate::console::widgets::scope_picker::ScopeChoice;
            match scope_state.handle_key(key) {
                ModalOutcome::Commit(ScopeChoice::AllAgents) => {
                    let scope = SecretsScopeTag::Workspace;
                    let state = env_key_input_state(
                        editor,
                        &scope,
                        "New workspace environment key",
                        String::new(),
                    );
                    editor.modal = Some(Modal::TextInput {
                        target: TextInputTarget::EnvKey { scope },
                        state,
                    });
                }
                ModalOutcome::Commit(ScopeChoice::SpecificAgent) => {
                    // Empty eligible set → `open_agent_override_picker`
                    // is a no-op; we close the modal then.
                    open_agent_override_picker(editor, config);
                    if !matches!(editor.modal, Some(Modal::RoleOverridePicker { .. })) {
                        editor.modal = None;
                    }
                }
                ModalOutcome::Cancel => {
                    editor.modal = None;
                }
                ModalOutcome::Continue => {}
            }
        }
        Modal::SourcePicker { state: source } => {
            use crate::console::widgets::source_picker::SourceChoice;
            use crate::console::widgets::text_input::TextInputState;
            match source.handle_key(key) {
                ModalOutcome::Commit(SourceChoice::Plain) => {
                    let Some((scope, key)) = editor.pending_env_key.clone() else {
                        editor.modal = None;
                        return;
                    };
                    editor.modal = Some(Modal::TextInput {
                        target: TextInputTarget::EnvValue {
                            scope,
                            key: key.clone(),
                        },
                        state: TextInputState::new_allow_empty(
                            format!("Value for {key}"),
                            String::new(),
                        ),
                    });
                }
                ModalOutcome::Commit(SourceChoice::Op) => {
                    let Some((scope, key)) = editor.pending_env_key.clone() else {
                        editor.modal = None;
                        return;
                    };
                    editor.pending_picker_target = Some((scope, Some(key)));
                    // Clear pending_env_key — pending_picker_target
                    // owns the (scope, key) pair now, and a stale
                    // pending_env_key would confuse a later
                    // sentinel-add commit.
                    editor.pending_env_key = None;
                    editor.modal = Some(Modal::OpPicker {
                        state: Box::new(OpPickerState::new_with_cache(op_cache)),
                    });
                }
                ModalOutcome::Cancel => {
                    // Cancel: drop the in-flight key name and close
                    // the modal. Operator returns to the Secrets tab
                    // with no env entry added.
                    editor.modal = None;
                    editor.pending_env_key = None;
                    editor.pending_picker_value = None;
                }
                ModalOutcome::Continue => {}
            }
        }
        Modal::OpPicker { state: picker } => {
            match picker.handle_key(key) {
                ModalOutcome::Commit(op_ref) => {
                    // Operator picked a Vault → Item → Field path. The
                    // dispatch depends on whether `P` was pressed on a
                    // key row (write directly) or on an `+ Add` sentinel
                    // (stash the OpRef, ask for the key name first).
                    let target = editor.pending_picker_target.take();
                    match target {
                        Some((scope, Some(key))) => {
                            set_pending_env_op_ref(editor, &scope, &key, op_ref);
                            editor.modal = None;
                        }
                        Some((scope, None)) => {
                            editor.pending_picker_value =
                                Some(crate::operator_env::EnvValue::OpRef(op_ref));
                            let label = format!("New environment key for {}", scope_label(&scope));
                            let state = env_key_input_state(editor, &scope, label, "");
                            editor.modal = Some(Modal::TextInput {
                                target: TextInputTarget::EnvKey { scope },
                                state,
                            });
                        }
                        None => {
                            editor.modal = None;
                        }
                    }
                }
                ModalOutcome::Cancel => {
                    // Clear both scratch fields so a stale path/target
                    // can't carry into a later interaction.
                    editor.modal = None;
                    editor.pending_picker_target = None;
                    editor.pending_picker_value = None;
                }
                ModalOutcome::Continue => {}
            }
        }
    }
}

/// `pending_picker_target` records `(scope, Some(key))` for key rows
/// (commit replaces value) or `(scope, None)` for sentinels (commit
/// stashes path, opens `EnvKey` modal). Headers / spacers are no-ops.
fn open_secrets_picker_modal(
    editor: &mut EditorState<'_>,
    op_cache: std::rc::Rc<std::cell::RefCell<crate::console::op_cache::OpCache>>,
) {
    let FieldFocus::Row(n) = editor.active_field;
    let rows = secrets_flat_rows(editor);
    let Some(row) = rows.get(n).cloned() else {
        return;
    };
    let target = match row {
        SecretsRow::WorkspaceKeyRow(key) => Some((SecretsScopeTag::Workspace, Some(key))),
        SecretsRow::RoleKeyRow { role, key } => Some((SecretsScopeTag::Role(role), Some(key))),
        SecretsRow::WorkspaceAddSentinel => Some((SecretsScopeTag::Workspace, None)),
        SecretsRow::RoleAddSentinel(role) => Some((SecretsScopeTag::Role(role), None)),
        SecretsRow::RoleHeader { .. } | SecretsRow::SectionSpacer => None,
    };
    let Some(target) = target else {
        return;
    };
    editor.pending_picker_target = Some(target);
    editor.modal = Some(Modal::OpPicker {
        state: Box::new(OpPickerState::new_with_cache(op_cache)),
    });
}

const fn scope_label(scope: &SecretsScopeTag) -> &str {
    match scope {
        SecretsScopeTag::Workspace => "workspace",
        SecretsScopeTag::Role(role) => role.as_str(),
    }
}

/// From `editor.pending` (not on-disk config) so a same-session
/// add blocks a follow-up duplicate.
fn forbidden_keys_for_scope(editor: &EditorState<'_>, scope: &SecretsScopeTag) -> Vec<String> {
    match scope {
        SecretsScopeTag::Workspace => editor.pending.env.keys().cloned().collect(),
        SecretsScopeTag::Role(role) => editor
            .pending
            .roles
            .get(role)
            .map(|o| o.env.keys().cloned().collect())
            .unwrap_or_default(),
    }
}

fn forbidden_label_for_scope(scope: &SecretsScopeTag) -> String {
    match scope {
        SecretsScopeTag::Workspace => "workspace env".to_string(),
        SecretsScopeTag::Role(role) => format!("role {role}"),
    }
}

/// Centralises `EnvKey` construction so every opener (Enter on
/// sentinel, A on row, P-on-sentinel fast-path, empty-key re-open)
/// stays consistent.
fn env_key_input_state<'a>(
    editor: &EditorState<'_>,
    scope: &SecretsScopeTag,
    label: impl Into<String>,
    initial: impl Into<String>,
) -> super::super::super::widgets::text_input::TextInputState<'a> {
    use super::super::super::widgets::text_input::TextInputState;
    let mut state =
        TextInputState::new_with_forbidden(label, initial, forbidden_keys_for_scope(editor, scope));
    state.forbidden_label = forbidden_label_for_scope(scope);
    state
}

/// Single source of truth for setting one env entry on the pending
/// draft. Role scope auto-creates the override entry and
/// auto-expands the section so the operator sees the new value —
/// same semantics as `ConfigEditor::set_env_var` on save.
fn set_pending_env_value(
    editor: &mut EditorState<'_>,
    scope: &SecretsScopeTag,
    key: &str,
    value: &str,
) {
    match scope {
        SecretsScopeTag::Workspace => {
            editor.pending.env.insert(
                key.to_string(),
                crate::operator_env::EnvValue::Plain(value.to_string()),
            );
        }
        SecretsScopeTag::Role(role) => {
            let entry = editor.pending.roles.entry(role.clone()).or_default();
            entry.env.insert(
                key.to_string(),
                crate::operator_env::EnvValue::Plain(value.to_string()),
            );
            editor.secrets_expanded.insert(role.clone());
        }
    }
}

/// Write an `OpRef` (picker commit result) into the pending env map.
fn set_pending_env_op_ref(
    editor: &mut EditorState<'_>,
    scope: &SecretsScopeTag,
    key: &str,
    op_ref: crate::operator_env::OpRef,
) {
    match scope {
        SecretsScopeTag::Workspace => {
            editor.pending.env.insert(
                key.to_string(),
                crate::operator_env::EnvValue::OpRef(op_ref),
            );
        }
        SecretsScopeTag::Role(role) => {
            let entry = editor.pending.roles.entry(role.clone()).or_default();
            entry.env.insert(
                key.to_string(),
                crate::operator_env::EnvValue::OpRef(op_ref),
            );
            editor.secrets_expanded.insert(role.clone());
        }
    }
}

/// Write an already-typed `EnvValue` into the pending env map.
/// Used by the sentinel-add flow where the picker stashed an `OpRef`
/// before the key name was known.
fn set_pending_env_value_typed(
    editor: &mut EditorState<'_>,
    scope: &SecretsScopeTag,
    key: &str,
    value: crate::operator_env::EnvValue,
) {
    match scope {
        SecretsScopeTag::Workspace => {
            editor.pending.env.insert(key.to_string(), value);
        }
        SecretsScopeTag::Role(role) => {
            let entry = editor.pending.roles.entry(role.clone()).or_default();
            entry.env.insert(key.to_string(), value);
            editor.secrets_expanded.insert(role.clone());
        }
    }
}

pub(super) fn apply_text_input_to_pending(
    target: &TextInputTarget,
    editor: &mut EditorState<'_>,
    value: &str,
    op_available: bool,
) {
    match target {
        TextInputTarget::Name => {
            editor.pending_name = Some(value.to_string());
        }
        TextInputTarget::Workdir => editor.pending.workdir = value.to_string(),
        TextInputTarget::MountDst => {
            // Provisional mount with src==dst was inserted at FileBrowser
            // commit; update its dst now.
            if let Some(last) = editor.pending.mounts.last_mut() {
                last.dst = value.to_string();
            }
        }
        TextInputTarget::Role => {
            // Role text-input is dispatched via apply_role_input before
            // reaching this match — landing here means a future caller
            // wired Role through the wrong path. Panic so the regression
            // is loud at the point of misuse rather than silently
            // discarding the user's input.
            unreachable!("TextInputTarget::Role is dispatched via apply_role_input");
        }
        TextInputTarget::EnvKey { scope } => {
            // Empty key re-opens the EnvKey modal with the inline
            // "cannot be empty" label instead of committing.
            let trimmed = value.trim();
            if trimmed.is_empty() {
                editor.pending_env_key = None;
                let state =
                    env_key_input_state(editor, scope, "Key cannot be empty", String::new());
                editor.modal = Some(Modal::TextInput {
                    target: TextInputTarget::EnvKey {
                        scope: scope.clone(),
                    },
                    state,
                });
                return;
            }
            let key = trimmed.to_string();
            // Sentinel-picker fast path: P committed an OpRef before the
            // key existed; both fields land here.
            if let Some(stashed) = editor.pending_picker_value.take() {
                set_pending_env_value_typed(editor, scope, &key, stashed);
                editor.pending_env_key = None;
                return;
            }
            editor.pending_env_key = Some((scope.clone(), key.clone()));
            editor.modal = Some(Modal::SourcePicker {
                state: crate::console::widgets::source_picker::SourcePickerState::new(
                    key,
                    op_available,
                ),
            });
        }
        TextInputTarget::EnvValue { scope, key } => {
            set_pending_env_value(editor, scope, key, value);
            editor.pending_env_key = None;
        }
    }
}

fn apply_role_input(
    editor: &mut EditorState<'_>,
    config: &mut AppConfig,
    paths: &JackinPaths,
    value: &str,
) {
    let mut runner = crate::docker::ShellRunner {
        debug: crate::tui::is_debug_mode(),
    };
    apply_role_input_with_runner(editor, config, paths, value, &mut runner);
}

fn apply_role_input_with_runner(
    editor: &mut EditorState<'_>,
    config: &mut AppConfig,
    paths: &JackinPaths,
    value: &str,
    runner: &mut impl crate::docker::CommandRunner,
) {
    let raw = value.trim();
    let selector = match crate::selector::RoleSelector::parse(raw) {
        Ok(selector) => selector,
        Err(e) => {
            let err = anyhow::Error::new(e);
            open_role_resolution_error(editor, raw, None, &err);
            return;
        }
    };

    let key = selector.key();
    let result = (|| -> anyhow::Result<crate::config::RoleSource> {
        let source = candidate_role_source(config, &selector)?;
        let source_to_register = source.clone();
        crate::runtime::register_agent_repo(
            paths,
            &selector,
            &source.git,
            runner,
            crate::tui::is_debug_mode(),
            || persist_role_source_registration(config, paths, &key, &source_to_register),
        )?;
        Ok(source)
    })();

    match result {
        Ok(source) if source.trusted => {
            add_role_to_workspace_editor(editor, config, &key);
        }
        Ok(source) => open_role_trust_confirm(editor, key, source),
        Err(e) => {
            let source = candidate_role_source(config, &selector).ok();
            open_role_resolution_error(editor, raw, source.as_ref().map(|source| &source.git), &e);
        }
    }
}

fn candidate_role_source(
    config: &AppConfig,
    selector: &crate::selector::RoleSelector,
) -> anyhow::Result<crate::config::RoleSource> {
    let mut candidate = config.clone();
    match candidate.resolve_role_source(selector) {
        Ok((source, _)) => Ok(source),
        Err(_) if selector.namespace.is_none() => Ok(crate::config::RoleSource {
            git: format!(
                "https://github.com/jackin-project/jackin-{}.git",
                selector.name
            ),
            trusted: false,
            env: std::collections::BTreeMap::new(),
        }),
        Err(err) => Err(err),
    }
}

fn open_role_resolution_error(
    editor: &mut EditorState<'_>,
    raw: &str,
    source_url: Option<&String>,
    err: &anyhow::Error,
) {
    crate::debug_log!("role", "failed to resolve role {raw:?}: {err:?}");
    let message = source_url.map_or_else(
        || {
            format!(
                "Could not understand role {raw:?}.\n\nUse a configured role such as \
             \"agent-smith\" or a GitHub selector like \"owner/agent-name\"."
            )
        },
        |source_url| {
            format!(
                "Could not resolve role {raw:?}.\n\nLooked for repository:\n{source_url}\n\n{}",
                friendly_role_resolution_error(err)
            )
        },
    );
    editor.modal = Some(Modal::ErrorPopup {
        state: crate::console::widgets::error_popup::ErrorPopupState::new(
            "Role not found",
            message,
        ),
    });
}

fn open_editor_action_error(editor: &mut EditorState<'_>, err: &dyn std::fmt::Display) {
    crate::debug_log!("editor", "failed to apply confirmed editor action: {err}");
    editor.modal = Some(Modal::ErrorPopup {
        state: crate::console::widgets::error_popup::ErrorPopupState::new(
            "Could not apply change",
            format!("The change could not be saved.\n\n{err}"),
        ),
    });
}

/// Translate a runtime role-resolution error into the operator-facing
/// blurb shown beneath the role-input dialog.
///
/// When adding a `RepoError` variant, add the corresponding match arm
/// here. Errors that were never wrapped as `RepoError` (e.g. fs/IO
/// errors raised before the clone) hit the fallback branch — generic
/// rather than mis-classified.
fn friendly_role_resolution_error(err: &anyhow::Error) -> String {
    if let Some(repo_err) = err
        .chain()
        .find_map(|cause| cause.downcast_ref::<crate::runtime::RepoError>())
    {
        return match repo_err {
            crate::runtime::RepoError::CloneFailed(_) => {
                "Repository is not available, or you do not have access.".into()
            }
            crate::runtime::RepoError::RemoteMismatch => {
                "A cached copy already exists for this role, but it points at a different \
                 repository."
                    .into()
            }
            crate::runtime::RepoError::InvalidRoleRepo(detail) => format!(
                "Repository is not a valid Jackin role: {}.",
                humanize_invalid_role_repo(detail)
            ),
        };
    }
    "Repository could not be used as a Jackin role.".into()
}

/// Render a `RoleRepoValidationError` for the role-input popup.
///
/// `Missing(path)` is shown as the basename only — the full repo path
/// is operator-noise here since the popup already says which role they
/// asked for. Other variants fall back to the typed `Display` impl with
/// any trailing period trimmed (the surrounding sentence adds its own).
fn humanize_invalid_role_repo(err: &crate::repo::RoleRepoValidationError) -> String {
    use crate::repo::RoleRepoValidationError as V;
    match err {
        V::Missing(path) => {
            let file = path
                .file_name()
                .and_then(|name| name.to_str())
                .map_or_else(|| path.display().to_string(), str::to_string);
            format!("missing {file}")
        }
        _ => err.to_string().trim_end_matches('.').to_string(),
    }
}

fn open_role_trust_confirm(
    editor: &mut EditorState<'_>,
    key: String,
    source: crate::config::RoleSource,
) {
    let state =
        crate::console::widgets::confirm::ConfirmState::role_trust(key.clone(), source.git.clone());
    editor.modal = Some(Modal::Confirm {
        target: ConfirmTarget::TrustRoleSource { key, source },
        state,
    });
}

fn persist_role_source_registration(
    config: &mut AppConfig,
    paths: &JackinPaths,
    key: &str,
    source: &crate::config::RoleSource,
) -> anyhow::Result<()> {
    let mut editor_doc = crate::config::ConfigEditor::open(paths)?;
    editor_doc.upsert_agent_source(key, source);
    *config = editor_doc.save()?;
    Ok(())
}

fn add_role_to_workspace_editor(editor: &mut EditorState<'_>, config: &AppConfig, key: &str) {
    if !editor.pending.allowed_roles.is_empty()
        && !editor.pending.allowed_roles.iter().any(|role| role == key)
    {
        editor.pending.allowed_roles.push(key.to_string());
    }

    if let Some(idx) = config.roles.keys().position(|role| role == key) {
        editor.active_field = FieldFocus::Row(idx);
    }
}

fn persist_trusted_role_add(
    editor: &mut EditorState<'_>,
    config: &mut AppConfig,
    paths: &JackinPaths,
    key: &str,
    mut source: crate::config::RoleSource,
) -> anyhow::Result<()> {
    source.trusted = true;
    persist_role_source_registration(config, paths, key, &source)?;
    add_role_to_workspace_editor(editor, config, key);
    Ok(())
}

fn apply_editor_confirm(
    editor: &mut EditorState<'_>,
    target: &ConfirmTarget,
    config: &mut AppConfig,
    paths: &JackinPaths,
) -> anyhow::Result<()> {
    match target {
        ConfirmTarget::DeleteEnvVar { scope, key } => match scope {
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
        },
        ConfirmTarget::TrustRoleSource { key, source } => {
            persist_trusted_role_add(editor, config, paths, key, source.clone())?;
        }
        // `DeleteIsolatedAndSave` is handled inline at the dispatch
        // site because it consumes `plan` and routes through
        // `EditorSaveFlow::PendingCommit`. No-op here.
        ConfirmTarget::DeleteIsolatedAndSave { .. } => {}
    }
    Ok(())
}

/// Only `EditAddMountSrc` is meaningful here; the prelude's
/// `CreateFirstMountSrc` target routes through `handle_prelude_modal`.
fn dispatch_editor_mount_dst_choice(
    editor: &mut EditorState<'_>,
    target: FileBrowserTarget,
    src: &str,
    outcome: &ModalOutcome<crate::console::widgets::mount_dst_choice::MountDstChoice>,
) {
    use crate::console::widgets::mount_dst_choice::MountDstChoice;
    match outcome {
        ModalOutcome::Commit(MountDstChoice::SamePath) => {
            if target == FileBrowserTarget::EditAddMountSrc {
                editor.pending.mounts.push(crate::workspace::MountConfig {
                    src: src.to_string(),
                    dst: src.to_string(),
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                });
            }
            editor.modal = None;
        }
        ModalOutcome::Commit(MountDstChoice::Edit) => {
            if target == FileBrowserTarget::EditAddMountSrc {
                editor.pending.mounts.push(crate::workspace::MountConfig {
                    src: src.to_string(),
                    dst: src.to_string(),
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                });
                editor.modal = Some(Modal::TextInput {
                    target: super::super::state::TextInputTarget::MountDst,
                    state: crate::console::widgets::text_input::TextInputState::new(
                        "Destination",
                        src,
                    ),
                });
            } else {
                editor.modal = None;
            }
        }
        ModalOutcome::Cancel => {
            editor.modal = None;
        }
        ModalOutcome::Continue => {}
    }
}

pub(super) fn apply_file_browser_to_editor(
    target: FileBrowserTarget,
    editor: &mut EditorState<'_>,
    path: std::path::PathBuf,
) {
    use crate::console::widgets::mount_dst_choice::MountDstChoiceState;
    match target {
        FileBrowserTarget::EditAddMountSrc => {
            // Defer the mount push to the choice modal: in the common case
            // the operator will take "Mount at same path" (dst = src) and we skip the
            // TextInput entirely. Only the `Edit destination` branch pushes
            // a provisional mount and opens the TextInput.
            editor.modal = Some(Modal::MountDstChoice {
                target,
                state: MountDstChoiceState::new(path.display().to_string()),
            });
        }
        FileBrowserTarget::CreateFirstMountSrc => {
            // Only meaningful in prelude path — handled by
            // `handle_prelude_modal`.
            let _ = (editor, path);
        }
    }
}

#[cfg(test)]
#[allow(clippy::too_many_lines)]
mod tests {
    //! Editor-stage tests: tab cycling, modal dispatch, role allow/default
    //! bindings, and mount-row readonly toggle.
    use super::super::super::state::{
        ConfirmTarget, EditorState, EditorTab, FieldFocus, FileBrowserTarget, ManagerStage,
        ManagerState, Modal, SecretsScopeTag, TextInputTarget,
    };
    use super::super::test_support::{key, mount};
    use super::{
        apply_file_browser_to_editor, apply_role_input_with_runner, apply_text_input_to_pending,
        handle_editor_modal,
    };
    use crate::config::AppConfig;
    use crate::console::manager::input::handle_key;
    use crate::console::op_cache::OpCache;
    use crate::paths::JackinPaths;
    use crate::workspace::{MountConfig, WorkspaceConfig};
    use crossterm::event::KeyCode;
    use tempfile::TempDir;

    /// Test helper: invoke `handle_editor_modal` with default plumbing
    /// for the new `op_available` / `op_cache` parameters. Existing
    /// editor-modal tests don't exercise the `SourcePicker` /
    /// `OpPicker` branches that need real wiring; defaults are fine.
    fn handle_modal(editor: &mut EditorState<'_>, k: crossterm::event::KeyEvent) {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let mut config = AppConfig::default();
        handle_modal_with(editor, k, &mut config, &paths);
    }

    fn handle_modal_with(
        editor: &mut EditorState<'_>,
        k: crossterm::event::KeyEvent,
        config: &mut AppConfig,
        paths: &JackinPaths,
    ) {
        handle_editor_modal(
            editor,
            k,
            false,
            std::rc::Rc::new(std::cell::RefCell::new(OpCache::default())),
            config,
            paths,
        );
    }

    /// Test helper: invoke `apply_text_input_to_pending` with
    /// `op_available = false`. Tests that don't open the `SourcePicker`
    /// don't care about the flag.
    fn apply_text_input(target: &TextInputTarget, editor: &mut EditorState<'_>, value: &str) {
        apply_text_input_to_pending(target, editor, value, false);
    }

    fn empty_ws() -> WorkspaceConfig {
        WorkspaceConfig::default()
    }

    fn config_with_agents(names: &[&str]) -> AppConfig {
        let mut config = AppConfig::default();
        for name in names {
            config.roles.insert(
                (*name).into(),
                crate::config::RoleSource {
                    git: format!("https://example.test/{name}.git"),
                    ..Default::default()
                },
            );
        }
        config.workspaces.insert("ws".into(), empty_ws());
        config
    }

    fn seed_valid_role_repo(repo_dir: &std::path::Path) {
        std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();
    }

    fn first_temp_role_repo(data_dir: &std::path::Path) -> std::path::PathBuf {
        std::fs::read_dir(data_dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| {
                path.is_dir()
                    && path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.starts_with("role-resolve-"))
            })
            .expect("role registration temp dir should exist before git clone side-effect")
            .join("repo")
    }

    fn seed_first_temp_valid_role_repo(data_dir: &std::path::Path) {
        seed_valid_role_repo(&first_temp_role_repo(data_dir));
    }

    fn editor_on_agents_tab<'a>(ws: WorkspaceConfig, row: usize) -> ManagerState<'a> {
        let mut state = ManagerState::from_config(&AppConfig::default(), std::path::Path::new("/"));
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Roles;
        editor.active_field = FieldFocus::Row(row);
        state.stage = ManagerStage::Editor(editor);
        state
    }

    fn editor_on_mounts_tab<'a>(ws: WorkspaceConfig, row: usize) -> ManagerState<'a> {
        let mut state = ManagerState::from_config(&AppConfig::default(), std::path::Path::new("/"));
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Mounts;
        editor.active_field = FieldFocus::Row(row);
        state.stage = ManagerStage::Editor(editor);
        state
    }

    fn ws_with_one_mount(readonly: bool) -> WorkspaceConfig {
        WorkspaceConfig {
            mounts: vec![MountConfig {
                src: "/host/a".into(),
                dst: "/host/a".into(),
                readonly,
                isolation: crate::isolation::MountIsolation::Shared,
            }],
            ..WorkspaceConfig::default()
        }
    }

    fn press(
        state: &mut ManagerState<'_>,
        config: &mut AppConfig,
        code: KeyCode,
    ) -> anyhow::Result<()> {
        let tmp = tempfile::tempdir()?;
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs()?;
        handle_key(state, config, &paths, tmp.path(), key(code))?;
        Ok(())
    }

    fn pending_allowed(state: &ManagerState<'_>) -> Vec<String> {
        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        e.pending.allowed_roles.clone()
    }

    /// Build an editor sitting on the Mounts tab with an empty mount list,
    /// and simulate the commit of a `FileBrowser` at `/host/path`. The bridge
    /// function is `apply_file_browser_to_editor`, which opens the new
    /// `MountDstChoice` modal instead of the old "push + `TextInput`" chain.
    fn editor_with_browser_committed(src: &str) -> EditorState<'static> {
        let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
        editor.active_tab = EditorTab::Mounts;
        editor.active_field = FieldFocus::Row(0);
        apply_file_browser_to_editor(
            FileBrowserTarget::EditAddMountSrc,
            &mut editor,
            std::path::PathBuf::from(src),
        );
        editor
    }

    /// Build a minimal `(ManagerState, AppConfig, JackinPaths, TempDir)` with
    /// the state stage parked in an Editor on the given `start_tab`. Used
    /// to drive `handle_key` through `handle_editor_key`'s tab-cycle branch.
    fn editor_state_on_tab(
        start_tab: EditorTab,
    ) -> (ManagerState<'static>, AppConfig, JackinPaths, TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let config = AppConfig::default();
        let mut state = ManagerState::from_config(&config, tmp.path());
        let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
        editor.active_tab = start_tab;
        state.stage = ManagerStage::Editor(editor);
        (state, config, paths, tmp)
    }

    // ── Editor: rename modal entry on the name row ────────────────────

    #[test]
    fn create_mode_enter_on_name_row_opens_rename_modal() {
        // In Create mode, pressing Enter on row 0 (Name) must open the
        // rename TextInput modal pre-filled with the current pending_name
        // — the same flow Edit mode uses. This is the operator's escape
        // hatch from a prelude-captured name they mistyped.
        let (tmp, paths, mut config) = {
            let tmp = tempfile::tempdir().unwrap();
            let paths = JackinPaths::for_tests(tmp.path());
            paths.ensure_base_dirs().unwrap();
            let config = AppConfig::default();
            let toml = toml::to_string(&config).unwrap();
            std::fs::write(&paths.config_file, toml).unwrap();
            let loaded = AppConfig::load_or_init(&paths).unwrap();
            (tmp, paths, loaded)
        };
        let cwd = tmp.path();
        let mut state = ManagerState::from_config(&config, cwd);
        let mut editor = EditorState::new_create();
        editor.pending_name = Some("typo-name".into());
        editor.active_field = FieldFocus::Row(0);
        state.stage = ManagerStage::Editor(editor);

        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!("still in editor after Enter on name row");
        };
        match &e.modal {
            Some(Modal::TextInput { target, state }) => {
                assert_eq!(target, &TextInputTarget::Name);
                assert_eq!(
                    state.value(),
                    "typo-name",
                    "TextInput must be pre-filled with current pending_name"
                );
            }
            other => panic!("expected TextInput(Name); got {other:?}"),
        }
    }

    #[test]
    fn create_mode_rename_commit_updates_pending_name() {
        // After the TextInput commits a new value, pending_name should
        // reflect the operator's edit. Same code path as Edit mode —
        // apply_text_input_to_pending doesn't distinguish modes.
        let mut editor = EditorState::new_create();
        editor.pending_name = Some("old-name".into());

        apply_text_input(&TextInputTarget::Name, &mut editor, "new-name");

        assert_eq!(editor.pending_name.as_deref(), Some("new-name"));
    }

    #[test]
    fn edit_mode_enter_on_name_row_still_opens_rename_modal() {
        // Regression guard: the Create-mode extension to row 0 Enter must
        // not break the Edit-mode path that already worked.
        let ws = WorkspaceConfig {
            workdir: "/w".into(),
            mounts: vec![mount("/w", "/w")],
            ..Default::default()
        };
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let mut config = AppConfig::default();
        config.workspaces.insert("keep-me".into(), ws.clone());
        let toml = toml::to_string(&config).unwrap();
        std::fs::write(&paths.config_file, toml).unwrap();
        let mut config = AppConfig::load_or_init(&paths).unwrap();

        let cwd = tmp.path();
        let mut state = ManagerState::from_config(&config, cwd);
        let mut editor = EditorState::new_edit("keep-me".into(), ws);
        editor.active_field = FieldFocus::Row(0);
        state.stage = ManagerStage::Editor(editor);

        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!();
        };
        match &e.modal {
            Some(Modal::TextInput { target, state }) => {
                assert_eq!(target, &TextInputTarget::Name);
                assert_eq!(state.value(), "keep-me");
            }
            other => panic!("expected TextInput(Name); got {other:?}"),
        }
    }

    // ── Editor FileBrowser → MountDstChoice behavioral tests ────────────

    #[test]
    fn filebrowser_commit_opens_mount_dst_choice_not_text_input() {
        // Pin: the FileBrowser→TextInput chain is replaced by
        // FileBrowser→MountDstChoice. No mount should be pushed yet — the
        // push is deferred to the choice modal's commit handler.
        let editor = editor_with_browser_committed("/host/path");
        assert!(
            matches!(editor.modal, Some(Modal::MountDstChoice { .. })),
            "expected MountDstChoice modal; got {:?}",
            editor.modal
        );
        assert_eq!(
            editor.pending.mounts.len(),
            0,
            "no mount must be pushed until the operator commits in the choice modal"
        );
    }

    #[test]
    fn editor_mount_same_path_commits_mount_with_dst_equal_src() {
        // Mount-at-same-path shortcut on the choice modal → push MountConfig with dst = src
        // and close the modal. No TextInput should appear.
        let mut editor = editor_with_browser_committed("/host/path");
        handle_modal(&mut editor, key(KeyCode::Char('m')));
        assert!(
            editor.modal.is_none(),
            "Mount at same path must close the modal; got {:?}",
            editor.modal
        );
        assert_eq!(editor.pending.mounts.len(), 1, "exactly one mount pushed");
        let m = &editor.pending.mounts[0];
        assert_eq!(m.src, "/host/path");
        assert_eq!(
            m.dst, "/host/path",
            "Mount-at-same-path fast path sets dst = src"
        );
        assert!(!m.readonly);
    }

    #[test]
    fn editor_edit_opens_textinput_and_pushes_provisional() {
        // Edit destination → push provisional mount (dst = src) + open
        // the TextInput pre-filled with src. Mirrors today's flow so the
        // operator can edit dst in place.
        let mut editor = editor_with_browser_committed("/host/path");
        handle_modal(&mut editor, key(KeyCode::Char('e')));
        match &editor.modal {
            Some(Modal::TextInput { target, .. }) => {
                assert_eq!(target, &TextInputTarget::MountDst);
            }
            other => panic!("expected TextInput(MountDst); got {other:?}"),
        }
        assert_eq!(
            editor.pending.mounts.len(),
            1,
            "provisional mount pushed for the TextInput to mutate"
        );
        let m = &editor.pending.mounts[0];
        assert_eq!(m.src, "/host/path");
        assert_eq!(m.dst, "/host/path", "provisional dst mirrors src");
    }

    #[test]
    fn editor_cancel_does_not_push_mount() {
        // C / Esc dismisses the choice modal without touching pending.mounts.
        let mut editor = editor_with_browser_committed("/host/path");
        handle_modal(&mut editor, key(KeyCode::Esc));
        assert!(editor.modal.is_none(), "Esc closes the modal");
        assert_eq!(
            editor.pending.mounts.len(),
            0,
            "Cancel must not push a mount"
        );

        let mut editor = editor_with_browser_committed("/host/path");
        handle_modal(&mut editor, key(KeyCode::Char('c')));
        assert!(editor.modal.is_none(), "`c` closes the modal");
        assert_eq!(editor.pending.mounts.len(), 0, "`c` must not push a mount");
    }

    // ── Editor Left/Right = prev/next tab ──────────────────────────────

    #[test]
    fn editor_right_arrow_advances_tab() {
        // Right should match Tab's forward cycle: General → Mounts.
        let (mut state, mut config, paths, tmp) = editor_state_on_tab(EditorTab::General);
        handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Right),
        )
        .unwrap();
        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert_eq!(e.active_tab, EditorTab::Mounts);
    }

    #[test]
    fn editor_left_arrow_rewinds_tab() {
        // Left implements the reverse tab cycle: Mounts → General.
        let (mut state, mut config, paths, tmp) = editor_state_on_tab(EditorTab::Mounts);
        handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Left),
        )
        .unwrap();
        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert_eq!(e.active_tab, EditorTab::General);
    }

    #[test]
    fn editor_left_wraps_to_last_tab_from_first() {
        // Match Tab's wrap contract: Left from General → Secrets.
        let (mut state, mut config, paths, tmp) = editor_state_on_tab(EditorTab::General);
        handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Left),
        )
        .unwrap();
        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert_eq!(e.active_tab, EditorTab::Secrets);
    }

    #[test]
    fn editor_right_wraps_to_first_tab_from_last() {
        // Match Tab's wrap contract: Right from Secrets → General.
        let (mut state, mut config, paths, tmp) = editor_state_on_tab(EditorTab::Secrets);
        handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Right),
        )
        .unwrap();
        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert_eq!(e.active_tab, EditorTab::General);
    }

    // ── Roles tab: `*` default-toggle binding ───────────────────────

    #[test]
    fn roles_tab_enter_on_add_role_row_opens_role_input() {
        let (tmp, paths, mut config) = {
            let tmp = tempfile::tempdir().unwrap();
            let paths = JackinPaths::for_tests(tmp.path());
            paths.ensure_base_dirs().unwrap();
            let config = config_with_agents(&["agent-smith"]);
            (tmp, paths, config)
        };
        let cwd = tmp.path();
        let mut state = editor_on_agents_tab(empty_ws(), config.roles.len());

        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        match &e.modal {
            Some(Modal::TextInput { target, state }) => {
                assert_eq!(target, &TextInputTarget::Role);
                assert_eq!(state.label, "Add role");
            }
            other => panic!("expected TextInput(Role); got {other:?}"),
        }
    }

    #[test]
    fn role_input_resolves_then_persists_namespaced_role_after_trust() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let mut config = config_with_agents(&["agent-smith"]);
        std::fs::write(&paths.config_file, toml::to_string(&config).unwrap()).unwrap();

        let mut editor = EditorState::new_edit("ws".into(), empty_ws());
        editor.pending.allowed_roles = vec!["agent-smith".into()];
        let selector = crate::selector::RoleSelector::parse("chainargos/agent-brown").unwrap();
        let cached_repo = crate::repo::CachedRepo::new(&paths, &selector);
        let data_dir = paths.data_dir.clone();
        let mut runner = crate::runtime::FakeRunner::default();
        runner.side_effects.push((
            "git clone".to_string(),
            Box::new(move || seed_first_temp_valid_role_repo(&data_dir)),
        ));

        apply_role_input_with_runner(
            &mut editor,
            &mut config,
            &paths,
            "chainargos/agent-brown",
            &mut runner,
        );

        assert!(
            runner.recorded.iter().any(|cmd| cmd
                .contains("git clone https://github.com/chainargos/jackin-agent-brown.git")),
            "role add must clone through the normal repo resolver; got {:?}",
            runner.recorded
        );
        let clone_cmd = runner
            .recorded
            .iter()
            .find(|cmd| {
                cmd.contains("git clone https://github.com/chainargos/jackin-agent-brown.git")
            })
            .expect("clone command should be recorded");
        assert!(
            clone_cmd.contains(paths.data_dir.to_str().unwrap()),
            "role add should clone into a temp dir under data_dir first: {clone_cmd}"
        );
        assert!(
            !clone_cmd.contains(paths.roles_dir.to_str().unwrap()),
            "role add must not clone directly into the final role cache: {clone_cmd}"
        );
        assert!(
            cached_repo.repo_dir.join("jackin.role.toml").is_file(),
            "validated clone should be moved into the role cache"
        );

        match &editor.modal {
            Some(Modal::Confirm { target, state }) => {
                assert_eq!(state.title, "Trust role source");
                let crate::console::widgets::confirm::ConfirmKind::RoleTrust { role, repository } =
                    &state.kind
                else {
                    panic!("expected RoleTrust kind, got {:?}", state.kind);
                };
                assert_eq!(role, "chainargos/agent-brown");
                assert_eq!(
                    repository, "https://github.com/chainargos/jackin-agent-brown.git",
                    "trust prompt should show the repository URL"
                );
                match target {
                    ConfirmTarget::TrustRoleSource { key, source } => {
                        assert_eq!(key, "chainargos/agent-brown");
                        assert_eq!(
                            source.git,
                            "https://github.com/chainargos/jackin-agent-brown.git"
                        );
                        assert!(
                            !source.trusted,
                            "newly resolved third-party role should require explicit trust first"
                        );
                    }
                    other => panic!("expected TrustRoleSource target; got {other:?}"),
                }
            }
            other => panic!("expected trust Confirm modal; got {other:?}"),
        }
        assert!(
            !editor
                .pending
                .allowed_roles
                .contains(&"chainargos/agent-brown".to_string()),
            "role should not be allowed before trust confirmation"
        );
        assert!(
            config
                .roles
                .get("chainargos/agent-brown")
                .is_some_and(|source| !source.trusted),
            "validated role source should be registered untrusted before trust confirmation"
        );
        let before_trust = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(
            before_trust.contains("[roles.\"chainargos/agent-brown\"]"),
            "validated role source should be persisted before trust confirmation:\n{before_trust}"
        );
        assert!(
            !before_trust.contains("trusted = true"),
            "role source should remain untrusted before trust confirmation:\n{before_trust}"
        );

        handle_modal_with(&mut editor, key(KeyCode::Char('y')), &mut config, &paths);

        assert!(editor.modal.is_none(), "trust confirmation should close");
        assert!(
            editor
                .pending
                .allowed_roles
                .contains(&"chainargos/agent-brown".to_string()),
            "custom allow-list should include the newly resolved role"
        );
        let source = config
            .roles
            .get("chainargos/agent-brown")
            .expect("role source must be added to config");
        assert_eq!(
            source.git,
            "https://github.com/chainargos/jackin-agent-brown.git"
        );
        assert!(source.trusted, "trusted role should be marked trusted");
        let persisted = std::fs::read_to_string(paths.config_file).unwrap();
        assert!(
            persisted.contains("[roles.\"chainargos/agent-brown\"]"),
            "new role source should be persisted:\n{persisted}"
        );
        assert!(
            persisted.contains("trusted = true"),
            "trusted role should be persisted with trusted = true:\n{persisted}"
        );
    }

    #[test]
    fn role_input_trust_decline_keeps_registered_role_untrusted() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let mut config = config_with_agents(&["agent-smith"]);
        std::fs::write(&paths.config_file, toml::to_string(&config).unwrap()).unwrap();

        let mut editor = EditorState::new_edit("ws".into(), empty_ws());
        editor.pending.allowed_roles = vec!["agent-smith".into()];
        let data_dir = paths.data_dir.clone();
        let mut runner = crate::runtime::FakeRunner::default();
        runner.side_effects.push((
            "git clone".to_string(),
            Box::new(move || seed_first_temp_valid_role_repo(&data_dir)),
        ));

        apply_role_input_with_runner(
            &mut editor,
            &mut config,
            &paths,
            "chainargos/agent-brown",
            &mut runner,
        );
        assert!(matches!(editor.modal, Some(Modal::Confirm { .. })));

        handle_modal_with(&mut editor, key(KeyCode::Char('n')), &mut config, &paths);

        assert!(editor.modal.is_none(), "decline should close trust prompt");
        assert!(
            !editor
                .pending
                .allowed_roles
                .contains(&"chainargos/agent-brown".to_string()),
            "declined role must not be added to the custom allow-list"
        );
        assert!(
            config
                .roles
                .get("chainargos/agent-brown")
                .is_some_and(|source| !source.trusted),
            "declined role should remain registered but untrusted"
        );
        let persisted = std::fs::read_to_string(paths.config_file).unwrap();
        assert!(
            persisted.contains("[roles.\"chainargos/agent-brown\"]"),
            "declined role source should remain registered:\n{persisted}"
        );
        assert!(
            !persisted.contains("trusted = true"),
            "declined role must not be persisted as trusted:\n{persisted}"
        );
    }

    #[test]
    fn role_input_existing_untrusted_role_can_be_validated_and_trusted() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let mut config = config_with_agents(&["agent-smith"]);
        config.roles.insert(
            "chainargos/agent-brown".into(),
            crate::config::RoleSource {
                git: "https://github.com/chainargos/jackin-agent-brown.git".into(),
                trusted: false,
                ..Default::default()
            },
        );
        std::fs::write(&paths.config_file, toml::to_string(&config).unwrap()).unwrap();

        let mut editor = EditorState::new_edit("ws".into(), empty_ws());
        editor.pending.allowed_roles = vec!["agent-smith".into()];
        let data_dir = paths.data_dir.clone();
        let mut runner = crate::runtime::FakeRunner::default();
        runner.side_effects.push((
            "git clone".to_string(),
            Box::new(move || seed_first_temp_valid_role_repo(&data_dir)),
        ));

        apply_role_input_with_runner(
            &mut editor,
            &mut config,
            &paths,
            "chainargos/agent-brown",
            &mut runner,
        );
        assert!(matches!(
            editor.modal,
            Some(Modal::Confirm {
                target: ConfirmTarget::TrustRoleSource { .. },
                ..
            })
        ));

        handle_modal_with(&mut editor, key(KeyCode::Char('y')), &mut config, &paths);

        assert!(
            config
                .roles
                .get("chainargos/agent-brown")
                .is_some_and(|source| source.trusted),
            "existing untrusted role should become trusted after confirmation"
        );
        assert!(
            editor
                .pending
                .allowed_roles
                .contains(&"chainargos/agent-brown".to_string()),
            "trusted role should be added to the custom allow-list"
        );
        let persisted = std::fs::read_to_string(paths.config_file).unwrap();
        assert!(
            persisted.contains("trusted = true"),
            "confirmed role should persist trust:\n{persisted}"
        );
    }

    #[test]
    fn role_input_trusted_existing_role_skips_trust_prompt() {
        // When the config already has a trusted role source the editor
        // must register the cached repo and add it to the workspace
        // *without* re-prompting for trust (`Ok(source) if
        // source.trusted` branch in `apply_role_input_with_runner`).
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let mut config = config_with_agents(&["agent-smith"]);
        config.roles.insert(
            "chainargos/agent-brown".into(),
            crate::config::RoleSource {
                git: "https://github.com/chainargos/jackin-agent-brown.git".into(),
                trusted: true,
                ..Default::default()
            },
        );
        std::fs::write(&paths.config_file, toml::to_string(&config).unwrap()).unwrap();

        let mut editor = EditorState::new_edit("ws".into(), empty_ws());
        editor.pending.allowed_roles = vec!["agent-smith".into()];
        let data_dir = paths.data_dir.clone();
        let mut runner = crate::runtime::FakeRunner::default();
        runner.side_effects.push((
            "git clone".to_string(),
            Box::new(move || seed_first_temp_valid_role_repo(&data_dir)),
        ));

        apply_role_input_with_runner(
            &mut editor,
            &mut config,
            &paths,
            "chainargos/agent-brown",
            &mut runner,
        );

        assert!(
            editor.modal.is_none(),
            "trusted existing role must not open the trust-confirm modal: {:?}",
            editor.modal
        );
        assert!(
            editor
                .pending
                .allowed_roles
                .contains(&"chainargos/agent-brown".to_string()),
            "trusted role should be added to the custom allow-list directly: {:?}",
            editor.pending.allowed_roles
        );
    }

    #[test]
    fn role_input_clone_failure_reports_candidate_repository_url() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let mut config = config_with_agents(&["agent-smith"]);
        std::fs::write(&paths.config_file, toml::to_string(&config).unwrap()).unwrap();

        let mut editor = EditorState::new_edit("ws".into(), empty_ws());
        let mut runner = crate::runtime::FakeRunner::default();
        runner
            .fail_with
            .push(("git clone".into(), "repository not found".into()));

        apply_role_input_with_runner(
            &mut editor,
            &mut config,
            &paths,
            "the-architect2",
            &mut runner,
        );

        match &editor.modal {
            Some(Modal::ErrorPopup { state }) => {
                assert_eq!(state.title, "Role not found");
                assert!(state.message.contains("Could not resolve role"));
                assert!(
                    state
                        .message
                        .contains("https://github.com/jackin-project/jackin-the-architect2.git"),
                    "message should show the repository URL that was tried:\n{}",
                    state.message
                );
                assert!(
                    state
                        .message
                        .contains("Repository is not available, or you do not have access."),
                    "message should explain the repository is unavailable:\n{}",
                    state.message
                );
                assert!(
                    !state.message.contains("git clone"),
                    "user-facing popup should not include raw clone commands:\n{}",
                    state.message
                );
                assert!(
                    !state.message.contains(paths.roles_dir.to_str().unwrap()),
                    "user-facing popup should not expose the final role cache path:\n{}",
                    state.message
                );
            }
            other => panic!("expected ErrorPopup for failed clone; got {other:?}"),
        }
        assert!(
            !config.roles.contains_key("the-architect2"),
            "failed clone must not add the role to in-memory config"
        );
        let persisted = std::fs::read_to_string(paths.config_file).unwrap();
        assert!(
            !persisted.contains("the-architect2"),
            "failed clone must not persist the role:\n{persisted}"
        );
    }

    #[test]
    fn role_input_invalid_repo_reports_role_contract_error() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let mut config = config_with_agents(&["agent-smith"]);
        std::fs::write(&paths.config_file, toml::to_string(&config).unwrap()).unwrap();

        let mut editor = EditorState::new_edit("ws".into(), empty_ws());
        let data_dir = paths.data_dir.clone();
        let mut runner = crate::runtime::FakeRunner::default();
        runner.side_effects.push((
            "git clone".to_string(),
            Box::new(move || {
                let repo_dir = first_temp_role_repo(&data_dir);
                std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
                std::fs::write(
                    repo_dir.join("Dockerfile"),
                    "FROM projectjackin/construct:trixie\n",
                )
                .unwrap();
            }),
        ));

        apply_role_input_with_runner(
            &mut editor,
            &mut config,
            &paths,
            "chainargos/agent-brown",
            &mut runner,
        );

        match &editor.modal {
            Some(Modal::ErrorPopup { state }) => {
                assert_eq!(state.title, "Role not found");
                assert!(
                    state.message.contains(
                        "Repository is not a valid Jackin role: missing jackin.role.toml."
                    ),
                    "message should explain the failed role validation:\n{}",
                    state.message
                );
                assert!(
                    state
                        .message
                        .contains("https://github.com/chainargos/jackin-agent-brown.git"),
                    "message should show the repository URL that was tried:\n{}",
                    state.message
                );
            }
            other => panic!("expected ErrorPopup for invalid role repo; got {other:?}"),
        }
        assert!(
            !config.roles.contains_key("chainargos/agent-brown"),
            "invalid role repo must not register the role source"
        );
        let persisted = std::fs::read_to_string(paths.config_file).unwrap();
        assert!(
            !persisted.contains("chainargos/agent-brown"),
            "invalid role repo must not persist the role:\n{persisted}"
        );
    }

    #[test]
    fn role_input_rejects_invalid_selector_with_error_popup() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let mut config = AppConfig::default();
        std::fs::write(&paths.config_file, toml::to_string(&config).unwrap()).unwrap();

        let mut editor = EditorState::new_edit("ws".into(), empty_ws());
        editor.modal = Some(Modal::TextInput {
            target: TextInputTarget::Role,
            state: crate::console::widgets::text_input::TextInputState::new(
                "Add role",
                "Chain Argus Agent Brown",
            ),
        });

        handle_modal_with(&mut editor, key(KeyCode::Enter), &mut config, &paths);

        match &editor.modal {
            Some(Modal::ErrorPopup { state }) => {
                assert_eq!(state.title, "Role not found");
                assert!(state.message.contains("Could not understand role"));
            }
            other => panic!("expected ErrorPopup for invalid selector; got {other:?}"),
        }
        assert!(
            config.roles.is_empty(),
            "invalid selector must not mutate config"
        );
    }

    #[test]
    fn agents_tab_star_sets_default_on_allowed_agent() {
        // Cursor on row 1 (role "beta"), no default set yet. Workspace
        // starts in "all roles allowed" shorthand, so beta is
        // effectively allowed. Pressing `*` pins it as default while
        // preserving the shorthand (empty allow list).
        let mut config = config_with_agents(&["alpha", "beta", "gamma"]);
        let mut state = editor_on_agents_tab(empty_ws(), 1);

        press(&mut state, &mut config, KeyCode::Char('*')).unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert_eq!(
            e.pending.default_role.as_deref(),
            Some("beta"),
            "`*` on row 1 should pin role `beta` as default",
        );
        assert!(
            e.pending.allowed_roles.is_empty(),
            "default-role pick must preserve the all-roles shorthand; \
             got {:?}",
            e.pending.allowed_roles,
        );
    }

    #[test]
    fn agents_tab_star_on_current_default_clears_it() {
        // With default = "alpha" (effectively allowed under shorthand),
        // pressing `*` on the same row clears the default. Toggle-off is
        // symmetric with the Space allow/disallow toggle.
        let mut config = config_with_agents(&["alpha", "beta"]);
        let mut ws = empty_ws();
        ws.default_role = Some("alpha".into());
        let mut state = editor_on_agents_tab(ws, 0);

        press(&mut state, &mut config, KeyCode::Char('*')).unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert!(
            e.pending.default_role.is_none(),
            "`*` on the current default must clear it; got {:?}",
            e.pending.default_role,
        );
    }

    #[test]
    fn agents_tab_star_on_unallowed_agent_is_noop() {
        // Workspace in "custom" mode with only `alpha` allowed; cursor
        // on row 1 (`beta`, NOT in the allow list). `*` must not set
        // beta as default — defaults are meaningless on disallowed
        // roles and the operator should `Space` to allow first.
        let mut config = config_with_agents(&["alpha", "beta", "gamma"]);
        let mut ws = empty_ws();
        ws.allowed_roles = vec!["alpha".into()];
        let mut state = editor_on_agents_tab(ws, 1);

        press(&mut state, &mut config, KeyCode::Char('*')).unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert!(
            e.pending.default_role.is_none(),
            "`*` on a disallowed role must be a no-op; got {:?}",
            e.pending.default_role,
        );
        assert_eq!(
            e.pending.allowed_roles,
            vec!["alpha".to_string()],
            "`*` must not silently extend the allow list; got {:?}",
            e.pending.allowed_roles,
        );
    }

    #[test]
    fn agents_tab_disallow_default_clears_default() {
        // With "alpha" pinned as default (custom allow list = [alpha]),
        // pressing Space on alpha to disallow it must also clear the
        // default — defaults are only meaningful on allowed roles.
        let mut config = config_with_agents(&["alpha", "beta"]);
        let mut ws = empty_ws();
        ws.allowed_roles = vec!["alpha".into()];
        ws.default_role = Some("alpha".into());
        let mut state = editor_on_agents_tab(ws, 0);

        press(&mut state, &mut config, KeyCode::Char(' ')).unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert!(
            !e.pending.allowed_roles.contains(&"alpha".to_string()),
            "alpha must be removed from allowed_roles after Space; got {:?}",
            e.pending.allowed_roles,
        );
        assert!(
            e.pending.default_role.is_none(),
            "disallowing the current default must clear default_role; got {:?}",
            e.pending.default_role,
        );
    }

    #[test]
    fn d_key_no_longer_sets_default_agent_on_agents_tab() {
        // Regression guard: the `D` binding was removed in favour of `*`.
        // Pressing `D` on an role row must now be a no-op (no other
        // Roles-tab binding listens for `D`).
        let mut config = config_with_agents(&["alpha", "beta"]);
        let mut state = editor_on_agents_tab(empty_ws(), 1);

        press(&mut state, &mut config, KeyCode::Char('D')).unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert!(
            e.pending.default_role.is_none(),
            "`D` must no longer set the default role on the Roles tab",
        );
    }

    // ── Roles tab: Space toggle matches effective allow-state ────────

    #[test]
    fn toggle_in_all_mode_demotes_to_custom_without_this_agent() {
        // Starting state: "all" mode (empty list), three roles. Pressing
        // Space on row 1 (`beta`) must produce a custom list containing
        // every other role — i.e. `[alpha, gamma]` — so that `beta`
        // flips from `[x]` to `[ ]` and the status line reads
        // `custom (2 of 3 allowed)`.
        let mut config = config_with_agents(&["alpha", "beta", "gamma"]);
        let mut state = editor_on_agents_tab(empty_ws(), 1);

        press(&mut state, &mut config, KeyCode::Char(' ')).unwrap();

        let list = pending_allowed(&state);
        assert_eq!(
            list,
            vec!["alpha".to_string(), "gamma".to_string()],
            "list must be populated with every other role when demoting from 'all'"
        );
    }

    #[test]
    fn toggle_custom_last_item_clears_to_empty() {
        // Starting state: "custom" mode with a single allowed role.
        // Toggling that role off must leave the list empty (reverting
        // to the "all" shorthand) — NOT pinning it at a phantom
        // `custom (0 of N allowed)` state.
        let mut config = config_with_agents(&["alpha", "beta"]);
        let mut ws = empty_ws();
        ws.allowed_roles = vec!["alpha".into()];
        let mut state = editor_on_agents_tab(ws, 0);

        press(&mut state, &mut config, KeyCode::Char(' ')).unwrap();

        assert_eq!(
            pending_allowed(&state),
            Vec::<String>::new(),
            "removing the last custom entry must leave the list empty (= all allowed)",
        );
    }

    #[test]
    fn toggle_adds_back_to_custom() {
        // Starting state: "custom" mode with `[alpha]` (so `beta` reads
        // `[ ]`). Pressing Space on `beta` (row 1) must add it, producing
        // `[alpha, beta]` — and since that still doesn't cover every
        // role (`gamma` is missing), the list must stay non-empty.
        let mut config = config_with_agents(&["alpha", "beta", "gamma"]);
        let mut ws = empty_ws();
        ws.allowed_roles = vec!["alpha".into()];
        let mut state = editor_on_agents_tab(ws, 1);

        press(&mut state, &mut config, KeyCode::Char(' ')).unwrap();

        let mut list = pending_allowed(&state);
        list.sort();
        assert_eq!(
            list,
            vec!["alpha".to_string(), "beta".to_string()],
            "adding `beta` with `gamma` still missing must produce a 2-of-3 custom list",
        );
    }

    #[test]
    fn toggle_refills_custom_to_all_when_last_agent_added_makes_it_complete() {
        // Starting state: "custom" mode with all-but-one role present.
        // Adding the missing one would yield `custom (N of N allowed)` —
        // semantically identical to "all allowed". The toggle must
        // collapse back to the empty-list shorthand so the status badge
        // reads `all`, not `custom (3 of 3 allowed)`.
        let mut config = config_with_agents(&["alpha", "beta", "gamma"]);
        let mut ws = empty_ws();
        ws.allowed_roles = vec!["alpha".into(), "beta".into()];
        // Cursor on row 2 (role `gamma`, the missing one).
        let mut state = editor_on_agents_tab(ws, 2);

        press(&mut state, &mut config, KeyCode::Char(' ')).unwrap();

        assert_eq!(
            pending_allowed(&state),
            Vec::<String>::new(),
            "filling the custom list must collapse it to empty (= all allowed)",
        );
    }

    // ── Mounts tab: R toggles readonly (rw ↔ ro) ──────────────────────

    #[test]
    fn r_key_toggles_readonly_on_current_mount_row() {
        // Start rw → one R press should flip to ro and register as a change.
        let mut config = AppConfig::default();
        let mut state = editor_on_mounts_tab(ws_with_one_mount(false), 0);

        press(&mut state, &mut config, KeyCode::Char('R')).unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert!(
            e.pending.mounts[0].readonly,
            "R on rw mount must flip to ro",
        );
        assert!(
            e.change_count() > 0,
            "flipping readonly must surface as a change; got change_count={}",
            e.change_count()
        );
    }

    #[test]
    fn r_key_lowercase_also_toggles_readonly() {
        // Operators often hit `r` without holding shift; both cases must work.
        let mut config = AppConfig::default();
        let mut state = editor_on_mounts_tab(ws_with_one_mount(false), 0);

        press(&mut state, &mut config, KeyCode::Char('r')).unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert!(e.pending.mounts[0].readonly);
    }

    #[test]
    fn r_key_on_sentinel_is_noop() {
        // Cursor on the `+ Add mount` sentinel (row == mounts.len()) — R must
        // not mutate mounts or trigger a change.
        let mut config = AppConfig::default();
        let ws = ws_with_one_mount(false);
        let before = ws.mounts.clone();
        let mut state = editor_on_mounts_tab(ws, 1); // sentinel row

        press(&mut state, &mut config, KeyCode::Char('R')).unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert_eq!(
            e.pending.mounts, before,
            "R on sentinel must leave mounts untouched"
        );
        assert_eq!(
            e.change_count(),
            0,
            "R on sentinel must not mark editor dirty"
        );
    }

    #[test]
    fn r_key_twice_restores_original() {
        // Flipping twice must bring `readonly` back to the starting value AND
        // net out to zero changes — the diff-based change_count treats
        // identical mounts as unchanged.
        let mut config = AppConfig::default();
        let mut state = editor_on_mounts_tab(ws_with_one_mount(false), 0);

        press(&mut state, &mut config, KeyCode::Char('R')).unwrap();
        press(&mut state, &mut config, KeyCode::Char('R')).unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert!(
            !e.pending.mounts[0].readonly,
            "two R presses must restore original rw state"
        );
        assert_eq!(
            e.change_count(),
            0,
            "two R presses must net zero changes; got {}",
            e.change_count()
        );
    }

    #[test]
    fn r_key_on_non_mounts_tab_is_noop() {
        // Cursor set to row 0 on General tab with a mount present; pressing R
        // must not mutate the mount list (the handler is gated on
        // `active_tab == EditorTab::Mounts`).
        let mut config = AppConfig::default();
        let ws = ws_with_one_mount(false);
        let before = ws.mounts.clone();
        let mut state = editor_on_mounts_tab(ws, 0);
        if let ManagerStage::Editor(e) = &mut state.stage {
            e.active_tab = EditorTab::General;
        }

        press(&mut state, &mut config, KeyCode::Char('R')).unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert_eq!(
            e.pending.mounts, before,
            "R on non-Mounts tab must leave mounts untouched"
        );
    }

    // ── Mounts tab: I cycles isolation (shared ↔ worktree) ────────────

    #[test]
    fn i_key_cycles_isolation_on_current_mount_row() {
        // Start Shared → one I press should flip to Worktree and register
        // as a change. Mirrors `r_key_toggles_readonly_on_current_mount_row`.
        let mut config = AppConfig::default();
        let mut state = editor_on_mounts_tab(ws_with_one_mount(false), 0);

        press(&mut state, &mut config, KeyCode::Char('I')).unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert_eq!(
            e.pending.mounts[0].isolation,
            crate::isolation::MountIsolation::Worktree,
            "I on a Shared mount must cycle to Worktree",
        );
        assert!(
            e.change_count() > 0,
            "cycling isolation must surface as a change; got change_count={}",
            e.change_count(),
        );
    }

    #[test]
    fn i_key_lowercase_also_cycles_isolation() {
        // Operators often hit `i` without holding shift; both cases must work.
        let mut config = AppConfig::default();
        let mut state = editor_on_mounts_tab(ws_with_one_mount(false), 0);

        press(&mut state, &mut config, KeyCode::Char('i')).unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert_eq!(
            e.pending.mounts[0].isolation,
            crate::isolation::MountIsolation::Worktree,
        );
    }

    #[test]
    fn toggle_rw_to_ro_reflects_in_render() {
        // After pressing R, render the Mounts tab and check the visible
        // `mode` column displays `ro`. Guards against a future regression
        // where the flip only updates state but the render helper ignores
        // the new value.
        use ratatui::backend::TestBackend;

        let mut config = AppConfig::default();
        let mut state = editor_on_mounts_tab(ws_with_one_mount(false), 0);

        press(&mut state, &mut config, KeyCode::Char('R')).unwrap();

        let ManagerStage::Editor(editor) = &state.stage else {
            panic!("editor stage expected");
        };
        let backend = TestBackend::new(80, 10);
        let mut term = ratatui::Terminal::new(backend).unwrap();
        term.draw(|f| {
            crate::console::manager::render::render_editor(f, editor, &config, true);
        })
        .unwrap();
        let buf = term.backend().buffer();
        let area = buf.area;
        let mut found = false;
        for y in 0..area.height {
            let mut row = String::new();
            for x in 0..area.width {
                row.push_str(buf[(x, y)].symbol());
            }
            if row.contains(" ro ") || row.trim_end().ends_with(" ro") || row.contains(" ro  ") {
                found = true;
                break;
            }
        }
        assert!(
            found,
            "post-toggle render must show `ro` in the mode column"
        );
    }

    // ── Caps-Lock parity: SHIFT-modified letter shortcuts ──────────────

    /// Enter on an op:// key row must NOT open the `EnvValue` text-edit
    /// modal. The breadcrumb is a path, not a credential, and hand-
    /// editing the path is error-prone — the operator deletes via D
    /// and re-adds via the source picker (`P`).
    #[test]
    fn enter_on_op_workspace_key_row_is_noop() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let mut config = AppConfig::default();
        let mut ws = empty_ws();
        ws.env.insert(
            "DB_URL".into(),
            crate::operator_env::EnvValue::OpRef(crate::operator_env::OpRef {
                op: "op://abc-vault/abc-item/password".into(),
                path: "Work/db/password".into(),
            }),
        );

        let mut state = ManagerState::from_config(&config, tmp.path());
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        editor.active_field = FieldFocus::Row(0); // the only key row
        state.stage = ManagerStage::Editor(editor);

        handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Enter),
        )
        .unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert!(
            e.modal.is_none(),
            "Enter on an op:// row must not open any modal; got {:?}",
            e.modal
        );
    }

    /// Same guard for an role-override row: Enter on an op:// value in
    /// an expanded role section is also a no-op.
    #[test]
    fn enter_on_op_agent_key_row_is_noop() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let mut config = AppConfig::default();
        let mut ws = empty_ws();
        let mut ag_env = std::collections::BTreeMap::new();
        ag_env.insert(
            "API_TOKEN".into(),
            crate::operator_env::EnvValue::OpRef(crate::operator_env::OpRef {
                op: "op://abc-vault/abc-item/api-token".into(),
                path: "Personal/api/token".into(),
            }),
        );
        ws.roles.insert(
            "smith".into(),
            crate::workspace::WorkspaceRoleOverride {
                env: ag_env,
                claude: None,
                codex: None,
            },
        );

        let mut state = ManagerState::from_config(&config, tmp.path());
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        editor.secrets_expanded.insert("smith".into());
        // Rows: WorkspaceAddSentinel(0), SectionSpacer(1), AgentHeader(2),
        //       AgentKeyRow(3), AgentAddSentinel(4). Focus the key row.
        editor.active_field = FieldFocus::Row(3);
        state.stage = ManagerStage::Editor(editor);

        handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Enter),
        )
        .unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert!(
            e.modal.is_none(),
            "Enter on an role op:// row must not open any modal; got {:?}",
            e.modal
        );
    }

    /// Caps Lock causes terminals to send letter keys with the SHIFT
    /// modifier set. The Secrets-tab `M` (mask toggle) and `P` (1Password
    /// picker) bindings must accept SHIFT just like NONE — otherwise an
    /// operator with Caps Lock on sees a silent no-op.
    #[test]
    fn secrets_tab_m_accepts_shift_modifier_for_caps_lock_parity() {
        use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let mut config = AppConfig::default();
        let mut ws = empty_ws();
        ws.env.insert("DB_URL".into(), "literal-value".into());
        let mut state = ManagerState::from_config(&config, tmp.path());
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        editor.active_field = FieldFocus::Row(0); // the only key row
        state.stage = ManagerStage::Editor(editor);

        let shift_m = KeyEvent {
            code: KeyCode::Char('M'),
            modifiers: KeyModifiers::SHIFT,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        };
        handle_key(&mut state, &mut config, &paths, tmp.path(), shift_m).unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert!(
            e.unmasked_rows
                .contains(&(SecretsScopeTag::Workspace, "DB_URL".into())),
            "M with SHIFT modifier (Caps Lock parity) must add the focused \
             row to unmasked_rows; got {:?}",
            e.unmasked_rows
        );
    }

    /// `M` on a focused workspace key row toggles only that row's mask
    /// state — sibling rows stay masked. This is the operator's core
    /// commit-32 ask: never reveal an unintended row.
    #[test]
    fn m_on_focused_workspace_key_unmasks_only_that_row() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let mut config = AppConfig::default();
        let mut ws = empty_ws();
        ws.env.insert("ALPHA".into(), "first-value".into());
        ws.env.insert("BETA".into(), "second-value".into());
        let mut state = ManagerState::from_config(&config, tmp.path());
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        // Rows are alphabetically ordered: ALPHA(0), BETA(1), Sentinel(2).
        editor.active_field = FieldFocus::Row(0);
        state.stage = ManagerStage::Editor(editor);

        handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Char('m')),
        )
        .unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert!(
            e.unmasked_rows
                .contains(&(SecretsScopeTag::Workspace, "ALPHA".into())),
            "ALPHA must be unmasked"
        );
        assert!(
            !e.unmasked_rows
                .contains(&(SecretsScopeTag::Workspace, "BETA".into())),
            "BETA must remain masked"
        );
    }

    /// Pressing M twice on the same row toggles the mask back on —
    /// the per-row state is a flip, not a one-way reveal.
    #[test]
    fn m_on_already_unmasked_row_re_masks_it() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let mut config = AppConfig::default();
        let mut ws = empty_ws();
        ws.env.insert("ALPHA".into(), "first".into());
        let mut state = ManagerState::from_config(&config, tmp.path());
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        editor.active_field = FieldFocus::Row(0);
        state.stage = ManagerStage::Editor(editor);

        handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Char('m')),
        )
        .unwrap();
        handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Char('m')),
        )
        .unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!();
        };
        assert!(
            e.unmasked_rows.is_empty(),
            "second M must remove the row from unmasked_rows; got {:?}",
            e.unmasked_rows
        );
    }

    /// M on an op:// row is a no-op — those rows render as breadcrumbs
    /// regardless of the mask state, so adding them to `unmasked_rows`
    /// would be visually inert and confuse the operator.
    #[test]
    fn m_on_op_reference_row_is_noop() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let mut config = AppConfig::default();
        let mut ws = empty_ws();
        ws.env.insert(
            "DB_URL".into(),
            crate::operator_env::EnvValue::OpRef(crate::operator_env::OpRef {
                op: "op://abc-vault/abc-item/password".into(),
                path: "Work/db/password".into(),
            }),
        );
        let mut state = ManagerState::from_config(&config, tmp.path());
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        editor.active_field = FieldFocus::Row(0);
        state.stage = ManagerStage::Editor(editor);

        handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Char('m')),
        )
        .unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!();
        };
        assert!(
            e.unmasked_rows.is_empty(),
            "M on an op:// row must not modify unmasked_rows; got {:?}",
            e.unmasked_rows
        );
    }

    /// Leaving and re-entering the Secrets tab clears `unmasked_rows`
    /// — the all-masked baseline is restored each visit.
    #[test]
    fn tab_leave_resets_unmasked_rows() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let mut config = AppConfig::default();
        let mut ws = empty_ws();
        ws.env.insert("ALPHA".into(), "first".into());
        let mut state = ManagerState::from_config(&config, tmp.path());
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        editor.active_field = FieldFocus::Row(0);
        state.stage = ManagerStage::Editor(editor);

        // Unmask ALPHA.
        handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Char('m')),
        )
        .unwrap();
        // Tab to General → leaves Secrets.
        handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Tab),
        )
        .unwrap();
        // Tab around the wheel back to Secrets (General → Mounts → Roles
        // → Secrets is 3 more presses).
        handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Tab),
        )
        .unwrap();
        handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Tab),
        )
        .unwrap();
        handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Tab),
        )
        .unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!();
        };
        assert_eq!(e.active_tab, EditorTab::Secrets);
        assert!(
            e.unmasked_rows.is_empty(),
            "tab-leave must clear unmasked_rows; got {:?}",
            e.unmasked_rows
        );
    }

    /// Workspace and role scopes have separate mask state. M on an
    /// role row unmasks only the role row even when a workspace row
    /// shares the same key name.
    #[test]
    fn m_on_agent_key_unmasks_only_that_row_in_that_agent_scope() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let mut config = AppConfig::default();
        let mut ws = empty_ws();
        // Same key name in both scopes.
        ws.env.insert("API_TOKEN".into(), "ws-value".into());
        let mut ag_env = std::collections::BTreeMap::new();
        ag_env.insert("API_TOKEN".into(), "role-value".into());
        ws.roles.insert(
            "smith".into(),
            crate::workspace::WorkspaceRoleOverride {
                env: ag_env,
                claude: None,
                codex: None,
            },
        );
        let mut state = ManagerState::from_config(&config, tmp.path());
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        editor.secrets_expanded.insert("smith".into());
        // Rows: WorkspaceKeyRow(0), WorkspaceAddSentinel(1),
        // SectionSpacer(2), AgentHeader(3), AgentKeyRow(4),
        // AgentAddSentinel(5). Focus the role key row.
        editor.active_field = FieldFocus::Row(4);
        state.stage = ManagerStage::Editor(editor);

        handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Char('m')),
        )
        .unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!();
        };
        assert!(
            e.unmasked_rows
                .contains(&(SecretsScopeTag::Role("smith".into()), "API_TOKEN".into())),
            "role-scope API_TOKEN must be unmasked"
        );
        assert!(
            !e.unmasked_rows
                .contains(&(SecretsScopeTag::Workspace, "API_TOKEN".into())),
            "workspace-scope API_TOKEN with same key name must remain masked"
        );
    }

    /// Pressing `↓` from the workspace `+ Add` sentinel must skip past
    /// the `SectionSpacer` and land directly on the first focusable row
    /// of the role section (the `AgentHeader`). Same in reverse with
    /// `↑`. Regression guard for the cursor-skip logic added with the
    /// blank-line-between-sections layout polish.
    #[test]
    fn cursor_skips_section_spacer_on_down_arrow() {
        use super::super::super::render::editor::{SecretsRow, secrets_flat_rows};

        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let mut config = AppConfig::default();
        let mut ws = empty_ws();
        let mut ag_env = std::collections::BTreeMap::new();
        ag_env.insert("LOG_LEVEL".into(), "debug".into());
        ws.roles.insert(
            "agent-smith".into(),
            crate::workspace::WorkspaceRoleOverride {
                env: ag_env,
                claude: None,
                codex: None,
            },
        );

        let mut state = ManagerState::from_config(&config, tmp.path());
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        // Rows with no workspace env keys + one collapsed role section:
        //   0 WorkspaceAddSentinel
        //   1 SectionSpacer
        //   2 AgentHeader
        editor.active_field = FieldFocus::Row(0);
        state.stage = ManagerStage::Editor(editor);

        // Sanity-check the row layout matches the comment above before
        // exercising the navigation.
        if let ManagerStage::Editor(e) = &state.stage {
            let rows = secrets_flat_rows(e);
            assert!(matches!(
                rows.first(),
                Some(SecretsRow::WorkspaceAddSentinel)
            ));
            assert!(matches!(rows.get(1), Some(SecretsRow::SectionSpacer)));
            assert!(matches!(rows.get(2), Some(SecretsRow::RoleHeader { .. })));
        }

        // ↓ from row 0 must land on row 2, skipping the spacer at row 1.
        handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Down),
        )
        .unwrap();
        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert!(
            matches!(e.active_field, FieldFocus::Row(2)),
            "↓ from sentinel(0) must skip spacer(1) and land on header(2); \
             got {:?}",
            e.active_field
        );

        // ↑ from row 2 must land back on row 0, skipping the spacer.
        handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Up),
        )
        .unwrap();
        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert!(
            matches!(e.active_field, FieldFocus::Row(0)),
            "↑ from header(2) must skip spacer(1) and land on sentinel(0); \
             got {:?}",
            e.active_field
        );
    }

    // ── General tab: keep_awake Space toggle ──────────────────────────

    #[test]
    fn space_on_general_keep_awake_row_toggles_pending_flag() {
        // Row 2 of the General tab is the keep_awake toggle. Space
        // flips pending.keep_awake.enabled; subsequent Space flips
        // back. The change lives only on `pending` (not `original`)
        // until the operator saves — that's what build_workspace_edit
        // detects to populate WorkspaceEdit.keep_awake_enabled.
        let (mut state, mut config, paths, tmp) = editor_state_on_tab(EditorTab::General);
        if let ManagerStage::Editor(e) = &mut state.stage {
            e.active_field = FieldFocus::Row(2);
        }

        handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Char(' ')),
        )
        .unwrap();
        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert!(
            e.pending.keep_awake.enabled,
            "first Space on row 2 must enable keep_awake"
        );
        assert!(
            !e.original.keep_awake.enabled,
            "Space must mutate pending only, not original (so the diff is visible to save)"
        );

        handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Char(' ')),
        )
        .unwrap();
        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert!(
            !e.pending.keep_awake.enabled,
            "second Space must toggle keep_awake back off",
        );
    }

    #[test]
    fn space_on_general_non_toggle_rows_does_not_flip_keep_awake() {
        // Row 0 (Name) and row 1 (Working dir) ignore Space — those
        // are modal-opening fields driven by Enter. A regression that
        // applied the toggle from any General row would flip the flag
        // when the operator was just typing a Space in a name input.
        for row in [0usize, 1usize] {
            let (mut state, mut config, paths, tmp) = editor_state_on_tab(EditorTab::General);
            if let ManagerStage::Editor(e) = &mut state.stage {
                e.active_field = FieldFocus::Row(row);
            }
            handle_key(
                &mut state,
                &mut config,
                &paths,
                tmp.path(),
                key(KeyCode::Char(' ')),
            )
            .unwrap();
            let ManagerStage::Editor(e) = &state.stage else {
                panic!("editor stage expected");
            };
            assert!(
                !e.pending.keep_awake.enabled,
                "Space on General row {row} must NOT toggle keep_awake",
            );
        }
    }

    #[test]
    fn down_arrow_on_general_can_reach_keep_awake_row() {
        // max_row_for_tab(General) must allow the cursor to navigate
        // to row 2; otherwise the toggle would be reachable only via
        // direct mutation, defeating the operator-discoverable
        // workflow.
        let (mut state, mut config, paths, tmp) = editor_state_on_tab(EditorTab::General);
        if let ManagerStage::Editor(e) = &mut state.stage {
            e.active_field = FieldFocus::Row(0);
        }
        handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Down),
        )
        .unwrap();
        handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Down),
        )
        .unwrap();
        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert!(
            matches!(e.active_field, FieldFocus::Row(2)),
            "two ↓ presses from row 0 must land on row 2 (Keep awake); got {:?}",
            e.active_field,
        );
    }

    // ── TUI text-entry regression: typing or pasting op:// must stay Plain ──

    /// Typing or pasting `op://...` into a value cell (text-entry path)
    /// must always commit as `EnvValue::Plain`. The picker is the ONLY
    /// TUI path that produces `EnvValue::OpRef`; this test pins that
    /// invariant so an accidental auto-resolve can never sneak in.
    ///
    /// The structural guarantee: `apply_text_input_to_pending` for the
    /// `EnvValue` target calls `set_pending_env_value`, which
    /// unconditionally wraps its `&str` argument in
    /// `EnvValue::Plain(value.to_string())`. There is no `op://` pattern
    /// match in the text-entry commit path.
    #[test]
    fn tui_text_entry_op_uri_always_commits_as_plain() {
        let mut editor =
            EditorState::new_edit("CLAUDE_TOKEN_WS".into(), WorkspaceConfig::default());

        let target = TextInputTarget::EnvValue {
            scope: SecretsScopeTag::Workspace,
            key: "CLAUDE_TOKEN".into(),
        };

        // Simulate committing a typed/pasted op:// string via the
        // text-entry path (Enter in the EnvValue modal).
        apply_text_input(&target, &mut editor, "op://Vault/Item/Field");

        let stored = editor
            .pending
            .env
            .get("CLAUDE_TOKEN")
            .expect("CLAUDE_TOKEN must be present after commit");

        assert_eq!(
            stored,
            &crate::operator_env::EnvValue::Plain("op://Vault/Item/Field".into()),
            "text-entry commit of op:// string must store EnvValue::Plain, \
             not EnvValue::OpRef — the picker is the only path to OpRef"
        );
        // Belt-and-suspenders: confirm it is NOT an OpRef.
        assert!(
            !matches!(stored, crate::operator_env::EnvValue::OpRef(_)),
            "text entry must never produce EnvValue::OpRef"
        );
    }
}
