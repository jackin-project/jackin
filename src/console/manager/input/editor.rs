//! Editor-stage dispatch: tab navigation, field focus, per-tab key
//! handling, and the editor-level modal dispatcher.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::super::super::widgets::{
    ModalOutcome, file_browser::FileBrowserState, op_picker::OpPickerState,
    workdir_pick::WorkdirPickState,
};
use super::super::render::editor::{SecretsRow, secrets_flat_row_count, secrets_flat_rows};
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
                // Direct `s` stays in editor; ExitIntent::Save passes
                // `true` to exit to list on success.
                super::save::begin_editor_save(state, config, false)?;
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

    // Clone before the editor borrow so the picker open site can
    // hand it off without re-borrowing `state`.
    let op_cache = state.op_cache.clone();

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
                let rows = secrets_flat_rows(editor, config);
                if let Some(SecretsRow::AgentHeader { agent, expanded }) = rows.get(n).cloned() {
                    if !expanded {
                        editor.secrets_expanded.insert(agent);
                    }
                    return Ok(InputOutcome::Continue);
                }
            }
            let was_secrets = editor.active_tab == EditorTab::Secrets;
            editor.active_tab = match editor.active_tab {
                EditorTab::General => EditorTab::Mounts,
                EditorTab::Mounts => EditorTab::Agents,
                EditorTab::Agents => EditorTab::Secrets,
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
                let rows = secrets_flat_rows(editor, config);
                if let Some(SecretsRow::AgentHeader { agent, expanded }) = rows.get(n).cloned() {
                    if expanded {
                        editor.secrets_expanded.remove(&agent);
                    }
                    return Ok(InputOutcome::Continue);
                }
            }
            let was_secrets = editor.active_tab == EditorTab::Secrets;
            editor.active_tab = match editor.active_tab {
                EditorTab::General => EditorTab::Secrets,
                EditorTab::Mounts => EditorTab::General,
                EditorTab::Agents => EditorTab::Mounts,
                EditorTab::Secrets => EditorTab::Agents,
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
                step_secrets_cursor_up(editor, config, candidate)
            } else {
                candidate
            };
            editor.active_field = FieldFocus::Row(next);
        }
        KeyCode::Down | KeyCode::Char('j' | 'J') => {
            let FieldFocus::Row(n) = editor.active_field;
            let max = max_row_for_tab(editor, config);
            let candidate = (n + 1).min(max);
            let next = if editor.active_tab == EditorTab::Secrets {
                step_secrets_cursor_down(editor, config, candidate, max)
            } else {
                candidate
            };
            editor.active_field = FieldFocus::Row(next);
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
                open_secrets_enter_modal(editor, config);
            }
            EditorTab::Agents => {}
        },
        KeyCode::Char(' ') if editor.active_tab == EditorTab::Agents => {
            toggle_agent_allowed_at_cursor(editor, config);
        }
        KeyCode::Char('*') if editor.active_tab == EditorTab::Agents => {
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
            toggle_focused_row_mask(editor, config);
        }
        // P sits at row level (not inside the EnvValue modal) so it
        // doesn't collide with text input. SHIFT tolerated per the
        // `m|M` arm above.
        KeyCode::Char('p' | 'P')
            if editor.active_tab == EditorTab::Secrets
                && (key.modifiers - KeyModifiers::SHIFT).is_empty() =>
        {
            open_secrets_picker_modal(editor, config, op_cache);
        }
        KeyCode::Char('d' | 'D')
            if editor.active_tab == EditorTab::Secrets
                && (key.modifiers - KeyModifiers::SHIFT).is_empty() =>
        {
            open_secrets_delete_confirm(editor, config);
        }
        KeyCode::Char('a' | 'A')
            if editor.active_tab == EditorTab::Secrets
                && (key.modifiers - KeyModifiers::SHIFT).is_empty() =>
        {
            open_secrets_add_modal(editor, config);
        }
        KeyCode::Char('r' | 'R') if editor.active_tab == super::super::state::EditorTab::Mounts => {
            let FieldFocus::Row(n) = editor.active_field;
            if let Some(m) = editor.pending.mounts.get_mut(n) {
                m.readonly = !m.readonly;
            }
        }
        KeyCode::Char('o' | 'O') if editor.active_tab == super::super::state::EditorTab::Mounts => {
            // Open in browser; toast for non-GitHub mounts so the
            // binding stays discoverable.
            let FieldFocus::Row(n) = editor.active_field;
            if let Some(m) = editor.pending.mounts.get(n) {
                let kind = super::super::mount_info::inspect(&m.src);
                match kind {
                    super::super::mount_info::MountKind::Git {
                        host: super::super::mount_info::GitHost::Github,
                        web_url: Some(url),
                        ..
                    } => {
                        if let Err(e) = open::that_detached(&url) {
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
        EditorTab::General => 1,
        EditorTab::Mounts => editor.pending.mounts.len(),
        EditorTab::Agents => config.agents.len().saturating_sub(1),
        EditorTab::Secrets => secrets_flat_row_count(editor, config).saturating_sub(1),
    }
}

/// Walks forward past spacer rows. Defensive fallback to `candidate`
/// if every row through `max` is a spacer (currently impossible).
fn step_secrets_cursor_down(
    editor: &EditorState<'_>,
    config: &AppConfig,
    candidate: usize,
    max: usize,
) -> usize {
    use super::super::render::editor::SecretsRow;
    let rows = secrets_flat_rows(editor, config);
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
fn step_secrets_cursor_up(editor: &EditorState<'_>, config: &AppConfig, candidate: usize) -> usize {
    use super::super::render::editor::SecretsRow;
    let rows = secrets_flat_rows(editor, config);
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
fn toggle_focused_row_mask(editor: &mut EditorState<'_>, config: &AppConfig) {
    let FieldFocus::Row(n) = editor.active_field;
    let rows = secrets_flat_rows(editor, config);
    let Some(row) = rows.get(n).cloned() else {
        return;
    };
    let key = match row {
        SecretsRow::WorkspaceKeyRow(key) => {
            // Op:// rows render as breadcrumbs and ignore mask state.
            let value = editor.pending.env.get(&key).cloned().unwrap_or_default();
            if crate::operator_env::is_op_reference(&value) {
                return;
            }
            (SecretsScopeTag::Workspace, key)
        }
        SecretsRow::AgentKeyRow { agent, key } => {
            let value = editor
                .pending
                .agents
                .get(&agent)
                .and_then(|o| o.env.get(&key))
                .cloned()
                .unwrap_or_default();
            if crate::operator_env::is_op_reference(&value) {
                return;
            }
            (SecretsScopeTag::Agent(agent), key)
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

fn open_secrets_enter_modal(editor: &mut EditorState<'_>, config: &AppConfig) {
    use super::super::super::widgets::text_input::TextInputState;
    let FieldFocus::Row(n) = editor.active_field;
    let rows = secrets_flat_rows(editor, config);
    let Some(row) = rows.get(n).cloned() else {
        return;
    };
    match row {
        SecretsRow::WorkspaceKeyRow(key) => {
            let current = editor.pending.env.get(&key).cloned().unwrap_or_default();
            // Op:// rows are not text-editable — operator deletes via
            // D and re-adds via the source picker.
            if crate::operator_env::is_op_reference(&current) {
                return;
            }
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
            // per-agent sentinel fast-path stays direct.
            use crate::console::widgets::scope_picker::ScopePickerState;
            editor.modal = Some(Modal::ScopePicker {
                state: ScopePickerState::new(),
            });
        }
        SecretsRow::AgentHeader { agent, expanded } => {
            if !expanded {
                editor.secrets_expanded.insert(agent);
            }
        }
        SecretsRow::AgentKeyRow { agent, key } => {
            let current = editor
                .pending
                .agents
                .get(&agent)
                .and_then(|o| o.env.get(&key))
                .cloned()
                .unwrap_or_default();
            if crate::operator_env::is_op_reference(&current) {
                return;
            }
            let label = format!("Edit {key}");
            editor.modal = Some(Modal::TextInput {
                target: TextInputTarget::EnvValue {
                    scope: SecretsScopeTag::Agent(agent),
                    key,
                },
                state: TextInputState::new_allow_empty(label, current),
            });
        }
        SecretsRow::AgentAddSentinel(agent) => {
            // In-section fast-path — already viewing the agent, don't
            // re-ask the scope question.
            let label = format!("New {agent} environment key");
            let scope = SecretsScopeTag::Agent(agent);
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
/// every agent in `config.agents`. Agents already carrying an
/// override are NOT filtered out — operator may want to add more
/// keys.
fn open_agent_override_picker(editor: &mut EditorState<'_>, config: &AppConfig) {
    use super::super::super::widgets::agent_picker::AgentPickerState;
    use crate::selector::ClassSelector;
    let eligible: Vec<ClassSelector> =
        super::super::render::editor::eligible_agents_for_override(editor, config)
            .into_iter()
            .filter_map(|name| ClassSelector::parse(&name).ok())
            .collect();
    if eligible.is_empty() {
        return;
    }
    editor.modal = Some(Modal::AgentOverridePicker {
        state: AgentPickerState::with_confirm_label(eligible, "select"),
    });
}

fn open_secrets_delete_confirm(editor: &mut EditorState<'_>, config: &AppConfig) {
    use crate::console::widgets::confirm::ConfirmState;
    let FieldFocus::Row(n) = editor.active_field;
    let rows = secrets_flat_rows(editor, config);
    let Some(row) = rows.get(n).cloned() else {
        return;
    };
    let (scope, key) = match row {
        SecretsRow::WorkspaceKeyRow(key) => (SecretsScopeTag::Workspace, key),
        SecretsRow::AgentKeyRow { agent, key } => (SecretsScopeTag::Agent(agent), key),
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
fn open_secrets_add_modal(editor: &mut EditorState<'_>, config: &AppConfig) {
    let FieldFocus::Row(n) = editor.active_field;
    let rows = secrets_flat_rows(editor, config);
    let Some(row) = rows.get(n).cloned() else {
        return;
    };
    let (scope, label) = match row {
        SecretsRow::WorkspaceKeyRow(_) | SecretsRow::WorkspaceAddSentinel => (
            SecretsScopeTag::Workspace,
            "New workspace environment key".to_string(),
        ),
        SecretsRow::AgentHeader { agent, .. }
        | SecretsRow::AgentKeyRow { agent, .. }
        | SecretsRow::AgentAddSentinel(agent) => (
            SecretsScopeTag::Agent(agent.clone()),
            format!("New {agent} environment key"),
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

/// Space on an agent row toggles its **effective** allow-state.
///
/// The underlying data model uses an "empty list = all allowed" shorthand,
/// so the checkbox on each row must reflect
/// `list.is_empty() || list.contains(name)`. The toggle preserves that
/// invariant in both directions:
///
/// - **Effective-allowed + empty list** (in "all" mode): populate the list
///   with every agent *except* this one. Status flips to
///   `custom (total-1 of total)`; the row flips to `[ ]`.
/// - **Effective-allowed + non-empty list** (row is in the list): remove it.
///   An empty remainder is left empty (semantically = "all"); otherwise
///   stays `custom`. The row flips to `[ ]`.
/// - **Effective-blocked** (row not in list): add the name. If the list now
///   contains every agent in `config.agents`, clear it back to empty
///   (= "all"). Otherwise stays `custom`. The row flips to `[x]`.
fn toggle_agent_allowed_at_cursor(editor: &mut EditorState<'_>, config: &AppConfig) {
    let FieldFocus::Row(n) = editor.active_field;
    // n is 0-based into config.agents (no header offset).
    let agent_names: Vec<String> = config.agents.keys().cloned().collect();
    let Some(agent) = agent_names.get(n) else {
        return;
    };

    // Read "all" state before the mutable borrow on `allowed_agents`.
    let is_all_mode = super::super::agent_allow::allows_all_agents(&editor.pending);
    let list = &mut editor.pending.allowed_agents;
    let in_list = list.iter().position(|a| a == agent);

    if is_all_mode {
        // Demote "all" to "custom" without this row by enumerating
        // the full roster minus the current agent.
        *list = agent_names
            .iter()
            .filter(|a| a.as_str() != agent.as_str())
            .cloned()
            .collect();
        if editor.pending.default_agent.as_deref() == Some(agent.as_str()) {
            editor.pending.default_agent = None;
        }
    } else if let Some(pos) = in_list {
        list.remove(pos);
        if editor.pending.default_agent.as_deref() == Some(agent.as_str()) {
            editor.pending.default_agent = None;
        }
    } else {
        // Filling in the full roster collapses back to the "all"
        // shorthand so the badge reads `all` rather than
        // `custom (N of N)`.
        list.push(agent.clone());
        if list.len() == agent_names.len() && agent_names.iter().all(|a| list.contains(a)) {
            list.clear();
        }
    }
}

/// On the current default → clear; on allowed → set; on disallowed
/// → no-op (operator must `Space` to allow first).
fn toggle_default_agent_at_cursor(editor: &mut EditorState<'_>, config: &AppConfig) {
    let FieldFocus::Row(n) = editor.active_field;
    let agent_names: Vec<String> = config.agents.keys().cloned().collect();
    let Some(agent) = agent_names.get(n) else {
        return;
    };

    if editor.pending.default_agent.as_deref() == Some(agent.as_str()) {
        editor.pending.default_agent = None;
        return;
    }

    if !super::super::agent_allow::agent_is_effectively_allowed(&editor.pending, agent) {
        return;
    }

    editor.pending.default_agent = Some(agent.clone());
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
    config: &AppConfig,
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
                    apply_text_input_to_pending(&target, editor, &value, op_available);
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
                    apply_editor_confirm(editor, &target);
                }
            }
            ModalOutcome::Cancel => {
                editor.modal = None;
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
        Modal::GithubPicker { .. } | Modal::AgentPicker { .. } => {
            editor.modal = None;
        }
        Modal::AgentOverridePicker { state: picker } => {
            match picker.handle_key(key) {
                ModalOutcome::Commit(agent) => {
                    // The override section materializes organically on
                    // the first value commit; we don't touch
                    // `pending.agents` here, so a cancel mid-flow leaves
                    // no empty placeholder.
                    let agent_name = agent.key();
                    let scope = SecretsScopeTag::Agent(agent_name.clone());
                    let label = format!("New {agent_name} environment key");
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
                    if !matches!(editor.modal, Some(Modal::AgentOverridePicker { .. })) {
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
                ModalOutcome::Commit(path) => {
                    // Operator picked a Vault → Item → Field path. The
                    // dispatch depends on whether `P` was pressed on a
                    // key row (write directly) or on an `+ Add` sentinel
                    // (stash the path, ask for the key name first).
                    let target = editor.pending_picker_target.take();
                    match target {
                        Some((scope, Some(key))) => {
                            set_pending_env_value(editor, &scope, &key, &path);
                            editor.modal = None;
                        }
                        Some((scope, None)) => {
                            editor.pending_picker_value = Some(path);
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
    config: &AppConfig,
    op_cache: std::rc::Rc<std::cell::RefCell<crate::console::op_cache::OpCache>>,
) {
    let FieldFocus::Row(n) = editor.active_field;
    let rows = secrets_flat_rows(editor, config);
    let Some(row) = rows.get(n).cloned() else {
        return;
    };
    let target = match row {
        SecretsRow::WorkspaceKeyRow(key) => Some((SecretsScopeTag::Workspace, Some(key))),
        SecretsRow::AgentKeyRow { agent, key } => Some((SecretsScopeTag::Agent(agent), Some(key))),
        SecretsRow::WorkspaceAddSentinel => Some((SecretsScopeTag::Workspace, None)),
        SecretsRow::AgentAddSentinel(agent) => Some((SecretsScopeTag::Agent(agent), None)),
        SecretsRow::AgentHeader { .. } | SecretsRow::SectionSpacer => None,
    };
    let Some(target) = target else {
        return;
    };
    editor.pending_picker_target = Some(target);
    editor.modal = Some(Modal::OpPicker {
        state: Box::new(OpPickerState::new_with_cache(op_cache)),
    });
}

fn scope_label(scope: &SecretsScopeTag) -> String {
    match scope {
        SecretsScopeTag::Workspace => "workspace".to_string(),
        SecretsScopeTag::Agent(agent) => agent.clone(),
    }
}

/// From `editor.pending` (not on-disk config) so a same-session
/// add blocks a follow-up duplicate.
fn forbidden_keys_for_scope(editor: &EditorState<'_>, scope: &SecretsScopeTag) -> Vec<String> {
    match scope {
        SecretsScopeTag::Workspace => editor.pending.env.keys().cloned().collect(),
        SecretsScopeTag::Agent(agent) => editor
            .pending
            .agents
            .get(agent)
            .map(|o| o.env.keys().cloned().collect())
            .unwrap_or_default(),
    }
}

fn forbidden_label_for_scope(scope: &SecretsScopeTag) -> String {
    match scope {
        SecretsScopeTag::Workspace => "workspace env".to_string(),
        SecretsScopeTag::Agent(agent) => format!("agent {agent}"),
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
/// draft. Agent scope auto-creates the override entry and
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
            editor
                .pending
                .env
                .insert(key.to_string(), value.to_string());
        }
        SecretsScopeTag::Agent(agent) => {
            let entry = editor.pending.agents.entry(agent.clone()).or_default();
            entry.env.insert(key.to_string(), value.to_string());
            editor.secrets_expanded.insert(agent.clone());
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
            // Sentinel-picker fast path: P committed a path before the
            // key existed; both fields land here.
            if let Some(stashed) = editor.pending_picker_value.take() {
                set_pending_env_value(editor, scope, &key, &stashed);
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

fn apply_editor_confirm(editor: &mut EditorState<'_>, target: &ConfirmTarget) {
    match target {
        ConfirmTarget::DeleteEnvVar { scope, key } => match scope {
            SecretsScopeTag::Workspace => {
                editor.pending.env.remove(key);
            }
            SecretsScopeTag::Agent(agent) => {
                let mut drop_agent = false;
                if let Some(ov) = editor.pending.agents.get_mut(agent) {
                    ov.env.remove(key);
                    // Drop empty override so change_count reports
                    // clean when the agent's overrides are later
                    // re-added.
                    if ov.env.is_empty() {
                        drop_agent = true;
                    }
                }
                if drop_agent {
                    editor.pending.agents.remove(agent);
                }
            }
        },
        // List-side target; never reaches the editor modal.
        ConfirmTarget::DeleteWorkspace => {}
    }
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
        ModalOutcome::Commit(MountDstChoice::Ok) => {
            if target == FileBrowserTarget::EditAddMountSrc {
                editor.pending.mounts.push(crate::workspace::MountConfig {
                    src: src.to_string(),
                    dst: src.to_string(),
                    readonly: false,
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
            // the operator will take "OK" (dst = src) and we skip the
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
    //! Editor-stage tests: tab cycling, modal dispatch, agent allow/default
    //! bindings, and mount-row readonly toggle.
    use super::super::super::state::{
        EditorState, EditorTab, FieldFocus, FileBrowserTarget, ManagerStage, ManagerState, Modal,
        SecretsScopeTag, TextInputTarget,
    };
    use super::super::test_support::{key, mount};
    use super::{apply_file_browser_to_editor, apply_text_input_to_pending, handle_editor_modal};
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
        handle_editor_modal(
            editor,
            k,
            false,
            std::rc::Rc::new(std::cell::RefCell::new(OpCache::default())),
            &AppConfig::default(),
        );
    }

    /// Test helper: invoke `apply_text_input_to_pending` with
    /// `op_available = false`. Tests that don't open the `SourcePicker`
    /// don't care about the flag.
    fn apply_text_input(target: &TextInputTarget, editor: &mut EditorState<'_>, value: &str) {
        apply_text_input_to_pending(target, editor, value, false);
    }

    fn empty_ws() -> WorkspaceConfig {
        WorkspaceConfig {
            workdir: String::new(),
            mounts: Vec::new(),
            allowed_agents: Vec::new(),
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        }
    }

    fn config_with_agents(names: &[&str]) -> AppConfig {
        let mut config = AppConfig::default();
        for name in names {
            config.agents.insert(
                (*name).into(),
                crate::config::AgentSource {
                    git: format!("https://example.test/{name}.git"),
                    ..Default::default()
                },
            );
        }
        config.workspaces.insert("ws".into(), empty_ws());
        config
    }

    fn editor_on_agents_tab<'a>(ws: WorkspaceConfig, row: usize) -> ManagerState<'a> {
        let mut state = ManagerState::from_config(&AppConfig::default(), std::path::Path::new("/"));
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Agents;
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
            workdir: String::new(),
            mounts: vec![MountConfig {
                src: "/host/a".into(),
                dst: "/host/a".into(),
                readonly,
            }],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
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
        e.pending.allowed_agents.clone()
    }

    /// Build an editor sitting on the Mounts tab with an empty mount list,
    /// and simulate the commit of a FileBrowser at `/host/path`. The bridge
    /// function is `apply_file_browser_to_editor`, which opens the new
    /// `MountDstChoice` modal instead of the old "push + TextInput" chain.
    fn editor_with_browser_committed(src: &str) -> EditorState<'static> {
        let ws = WorkspaceConfig {
            workdir: String::new(),
            mounts: vec![],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        };
        let mut editor = EditorState::new_edit("ws".into(), ws);
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
        let ws = WorkspaceConfig {
            workdir: String::new(),
            mounts: vec![],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        };
        let mut state = ManagerState::from_config(&config, tmp.path());
        let mut editor = EditorState::new_edit("ws".into(), ws);
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
        let (_tmp, paths, mut config) = {
            let tmp = tempfile::tempdir().unwrap();
            let paths = JackinPaths::for_tests(tmp.path());
            paths.ensure_base_dirs().unwrap();
            let config = AppConfig::default();
            let toml = toml::to_string(&config).unwrap();
            std::fs::write(&paths.config_file, toml).unwrap();
            let loaded = AppConfig::load_or_init(&paths).unwrap();
            (tmp, paths, loaded)
        };
        let cwd = _tmp.path();
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
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
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
    fn editor_ok_commits_mount_with_dst_equal_src() {
        // OK shortcut on the choice modal → push MountConfig with dst = src
        // and close the modal. No TextInput should appear.
        let mut editor = editor_with_browser_committed("/host/path");
        handle_modal(&mut editor, key(KeyCode::Char('o')));
        assert!(
            editor.modal.is_none(),
            "OK must close the modal; got {:?}",
            editor.modal
        );
        assert_eq!(editor.pending.mounts.len(), 1, "exactly one mount pushed");
        let m = &editor.pending.mounts[0];
        assert_eq!(m.src, "/host/path");
        assert_eq!(m.dst, "/host/path", "OK fast-path sets dst = src");
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

    // ── Agents tab: `*` default-toggle binding ───────────────────────

    #[test]
    fn agents_tab_star_sets_default_on_allowed_agent() {
        // Cursor on row 1 (agent "beta"), no default set yet. Workspace
        // starts in "all agents allowed" shorthand, so beta is
        // effectively allowed. Pressing `*` pins it as default while
        // preserving the shorthand (empty allow list).
        let mut config = config_with_agents(&["alpha", "beta", "gamma"]);
        let mut state = editor_on_agents_tab(empty_ws(), 1);

        press(&mut state, &mut config, KeyCode::Char('*')).unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert_eq!(
            e.pending.default_agent.as_deref(),
            Some("beta"),
            "`*` on row 1 should pin agent `beta` as default",
        );
        assert!(
            e.pending.allowed_agents.is_empty(),
            "default-agent pick must preserve the all-agents shorthand; \
             got {:?}",
            e.pending.allowed_agents,
        );
    }

    #[test]
    fn agents_tab_star_on_current_default_clears_it() {
        // With default = "alpha" (effectively allowed under shorthand),
        // pressing `*` on the same row clears the default. Toggle-off is
        // symmetric with the Space allow/disallow toggle.
        let mut config = config_with_agents(&["alpha", "beta"]);
        let mut ws = empty_ws();
        ws.default_agent = Some("alpha".into());
        let mut state = editor_on_agents_tab(ws, 0);

        press(&mut state, &mut config, KeyCode::Char('*')).unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert!(
            e.pending.default_agent.is_none(),
            "`*` on the current default must clear it; got {:?}",
            e.pending.default_agent,
        );
    }

    #[test]
    fn agents_tab_star_on_unallowed_agent_is_noop() {
        // Workspace in "custom" mode with only `alpha` allowed; cursor
        // on row 1 (`beta`, NOT in the allow list). `*` must not set
        // beta as default — defaults are meaningless on disallowed
        // agents and the operator should `Space` to allow first.
        let mut config = config_with_agents(&["alpha", "beta", "gamma"]);
        let mut ws = empty_ws();
        ws.allowed_agents = vec!["alpha".into()];
        let mut state = editor_on_agents_tab(ws, 1);

        press(&mut state, &mut config, KeyCode::Char('*')).unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert!(
            e.pending.default_agent.is_none(),
            "`*` on a disallowed agent must be a no-op; got {:?}",
            e.pending.default_agent,
        );
        assert_eq!(
            e.pending.allowed_agents,
            vec!["alpha".to_string()],
            "`*` must not silently extend the allow list; got {:?}",
            e.pending.allowed_agents,
        );
    }

    #[test]
    fn agents_tab_disallow_default_clears_default() {
        // With "alpha" pinned as default (custom allow list = [alpha]),
        // pressing Space on alpha to disallow it must also clear the
        // default — defaults are only meaningful on allowed agents.
        let mut config = config_with_agents(&["alpha", "beta"]);
        let mut ws = empty_ws();
        ws.allowed_agents = vec!["alpha".into()];
        ws.default_agent = Some("alpha".into());
        let mut state = editor_on_agents_tab(ws, 0);

        press(&mut state, &mut config, KeyCode::Char(' ')).unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert!(
            !e.pending.allowed_agents.contains(&"alpha".to_string()),
            "alpha must be removed from allowed_agents after Space; got {:?}",
            e.pending.allowed_agents,
        );
        assert!(
            e.pending.default_agent.is_none(),
            "disallowing the current default must clear default_agent; got {:?}",
            e.pending.default_agent,
        );
    }

    #[test]
    fn d_key_no_longer_sets_default_agent_on_agents_tab() {
        // Regression guard: the `D` binding was removed in favour of `*`.
        // Pressing `D` on an agent row must now be a no-op (no other
        // Agents-tab binding listens for `D`).
        let mut config = config_with_agents(&["alpha", "beta"]);
        let mut state = editor_on_agents_tab(empty_ws(), 1);

        press(&mut state, &mut config, KeyCode::Char('D')).unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert!(
            e.pending.default_agent.is_none(),
            "`D` must no longer set the default agent on the Agents tab",
        );
    }

    // ── Agents tab: Space toggle matches effective allow-state ────────

    #[test]
    fn toggle_in_all_mode_demotes_to_custom_without_this_agent() {
        // Starting state: "all" mode (empty list), three agents. Pressing
        // Space on row 1 (`beta`) must produce a custom list containing
        // every other agent — i.e. `[alpha, gamma]` — so that `beta`
        // flips from `[x]` to `[ ]` and the status line reads
        // `custom (2 of 3 allowed)`.
        let mut config = config_with_agents(&["alpha", "beta", "gamma"]);
        let mut state = editor_on_agents_tab(empty_ws(), 1);

        press(&mut state, &mut config, KeyCode::Char(' ')).unwrap();

        let list = pending_allowed(&state);
        assert_eq!(
            list,
            vec!["alpha".to_string(), "gamma".to_string()],
            "list must be populated with every other agent when demoting from 'all'"
        );
    }

    #[test]
    fn toggle_custom_last_item_clears_to_empty() {
        // Starting state: "custom" mode with a single allowed agent.
        // Toggling that agent off must leave the list empty (reverting
        // to the "all" shorthand) — NOT pinning it at a phantom
        // `custom (0 of N allowed)` state.
        let mut config = config_with_agents(&["alpha", "beta"]);
        let mut ws = empty_ws();
        ws.allowed_agents = vec!["alpha".into()];
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
        // agent (`gamma` is missing), the list must stay non-empty.
        let mut config = config_with_agents(&["alpha", "beta", "gamma"]);
        let mut ws = empty_ws();
        ws.allowed_agents = vec!["alpha".into()];
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
        // Starting state: "custom" mode with all-but-one agent present.
        // Adding the missing one would yield `custom (N of N allowed)` —
        // semantically identical to "all allowed". The toggle must
        // collapse back to the empty-list shorthand so the status badge
        // reads `all`, not `custom (3 of 3 allowed)`.
        let mut config = config_with_agents(&["alpha", "beta", "gamma"]);
        let mut ws = empty_ws();
        ws.allowed_agents = vec!["alpha".into(), "beta".into()];
        // Cursor on row 2 (agent `gamma`, the missing one).
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
            crate::console::manager::render::render_editor(f, editor, &config, &[]);
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

    /// Enter on an op:// key row must NOT open the EnvValue text-edit
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
        ws.env
            .insert("DB_URL".into(), "op://Work/db/password".into());

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

    /// Same guard for an agent-override row: Enter on an op:// value in
    /// an expanded agent section is also a no-op.
    #[test]
    fn enter_on_op_agent_key_row_is_noop() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let mut config = AppConfig::default();
        let mut ws = empty_ws();
        let mut ag_env = std::collections::BTreeMap::new();
        ag_env.insert("API_TOKEN".into(), "op://acct/Personal/api/token".into());
        ws.agents.insert(
            "smith".into(),
            crate::workspace::WorkspaceAgentOverride { env: ag_env },
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
            "Enter on an agent op:// row must not open any modal; got {:?}",
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
        ws.env
            .insert("DB_URL".into(), "op://Work/db/password".into());
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
        // Tab around the wheel back to Secrets (General → Mounts → Agents
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

    /// Workspace and agent scopes have separate mask state. M on an
    /// agent row unmasks only the agent row even when a workspace row
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
        ag_env.insert("API_TOKEN".into(), "agent-value".into());
        ws.agents.insert(
            "smith".into(),
            crate::workspace::WorkspaceAgentOverride { env: ag_env },
        );
        let mut state = ManagerState::from_config(&config, tmp.path());
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        editor.secrets_expanded.insert("smith".into());
        // Rows: WorkspaceKeyRow(0), WorkspaceAddSentinel(1),
        // SectionSpacer(2), AgentHeader(3), AgentKeyRow(4),
        // AgentAddSentinel(5). Focus the agent key row.
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
                .contains(&(SecretsScopeTag::Agent("smith".into()), "API_TOKEN".into())),
            "agent-scope API_TOKEN must be unmasked"
        );
        assert!(
            !e.unmasked_rows
                .contains(&(SecretsScopeTag::Workspace, "API_TOKEN".into())),
            "workspace-scope API_TOKEN with same key name must remain masked"
        );
    }

    /// Pressing `↓` from the workspace `+ Add` sentinel must skip past
    /// the `SectionSpacer` and land directly on the first focusable row
    /// of the agent section (the `AgentHeader`). Same in reverse with
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
        ws.agents.insert(
            "agent-smith".into(),
            crate::workspace::WorkspaceAgentOverride { env: ag_env },
        );

        let mut state = ManagerState::from_config(&config, tmp.path());
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        // Rows with no workspace env keys + one collapsed agent section:
        //   0 WorkspaceAddSentinel
        //   1 SectionSpacer
        //   2 AgentHeader
        editor.active_field = FieldFocus::Row(0);
        state.stage = ManagerStage::Editor(editor);

        // Sanity-check the row layout matches the comment above before
        // exercising the navigation.
        if let ManagerStage::Editor(e) = &state.stage {
            let rows = secrets_flat_rows(e, &config);
            assert!(matches!(
                rows.first(),
                Some(SecretsRow::WorkspaceAddSentinel)
            ));
            assert!(matches!(rows.get(1), Some(SecretsRow::SectionSpacer)));
            assert!(matches!(rows.get(2), Some(SecretsRow::AgentHeader { .. })));
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
}
