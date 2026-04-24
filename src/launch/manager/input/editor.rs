//! Editor-stage dispatch: tab navigation, field focus, per-tab key
//! handling, and the editor-level modal dispatcher.

use crossterm::event::{KeyCode, KeyEvent};

use super::super::super::widgets::{
    ModalOutcome, file_browser::FileBrowserState, workdir_pick::WorkdirPickState,
};
use super::super::state::{
    EditorMode, EditorSaveFlow, EditorState, ExitIntent, FieldFocus, FileBrowserTarget,
    ManagerStage, ManagerState, Modal, Toast, ToastKind,
};
use super::InputOutcome;
use crate::config::AppConfig;
use crate::paths::JackinPaths;

// Central keymap dispatch for the editor view: one giant match on
// `key.code` with per-tab guards. Extracting each arm into a helper would
// just scatter the keymap across a dozen tiny functions without making
// the dispatch easier to read — the table-like structure here is the
// point. Accept the length over an awkward split.
#[allow(clippy::too_many_lines)]
pub(super) fn handle_editor_key(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    paths: &JackinPaths,
    cwd: &std::path::Path,
    key: KeyEvent,
) -> anyhow::Result<InputOutcome> {
    // Handle s and Esc outside the editor borrow to avoid re-borrow
    // conflicts (both need to call back into state or config).
    match key.code {
        KeyCode::Char('s' | 'S') => {
            if let ManagerStage::Editor(editor) = &state.stage {
                // No-op when there's nothing to save — avoid putting up
                // an empty ConfirmSave dialog.
                if editor.change_count() == 0 {
                    return Ok(InputOutcome::Continue);
                }
            }
            if matches!(&state.stage, ManagerStage::Editor(_)) {
                // Direct `s` press: stay in the editor after a successful
                // save. The `ExitIntent::Save` path in the outer dispatcher
                // passes `true` so that path exits to the list on success.
                super::save::begin_editor_save(state, config, false)?;
            }
            // `paths` is not needed until the operator actually commits
            // in the ConfirmSave dialog; silence the unused binding until
            // the reborrow for commit happens in handle_editor_modal.
            let _ = paths;
            return Ok(InputOutcome::Continue);
        }
        KeyCode::Esc => {
            // Re-borrow pattern — immutable read first, then mutate.
            if let ManagerStage::Editor(editor) = &state.stage {
                let dirty = editor.is_dirty();
                if dirty {
                    if let ManagerStage::Editor(editor) = &mut state.stage {
                        editor.modal = Some(Modal::SaveDiscardCancel {
                            state: crate::launch::widgets::save_discard::SaveDiscardState::new(
                                "Save changes before leaving?",
                            ),
                        });
                    }
                } else {
                    *state = ManagerState::from_config(config, cwd);
                }
            }
            return Ok(InputOutcome::Continue);
        }
        _ => {}
    }

    let ManagerStage::Editor(editor) = &mut state.stage else {
        return Ok(InputOutcome::Continue);
    };

    match key.code {
        KeyCode::Tab | KeyCode::Right => {
            editor.active_tab = match editor.active_tab {
                super::super::state::EditorTab::General => super::super::state::EditorTab::Mounts,
                super::super::state::EditorTab::Mounts => super::super::state::EditorTab::Agents,
                super::super::state::EditorTab::Agents => super::super::state::EditorTab::Secrets,
                super::super::state::EditorTab::Secrets => super::super::state::EditorTab::General,
            };
            editor.active_field = FieldFocus::Row(0);
        }
        KeyCode::BackTab | KeyCode::Left => {
            editor.active_tab = match editor.active_tab {
                super::super::state::EditorTab::General => super::super::state::EditorTab::Secrets,
                super::super::state::EditorTab::Mounts => super::super::state::EditorTab::General,
                super::super::state::EditorTab::Agents => super::super::state::EditorTab::Mounts,
                super::super::state::EditorTab::Secrets => super::super::state::EditorTab::Agents,
            };
            editor.active_field = FieldFocus::Row(0);
        }
        KeyCode::Up | KeyCode::Char('k' | 'K') => {
            let FieldFocus::Row(n) = editor.active_field;
            editor.active_field = FieldFocus::Row(n.saturating_sub(1));
        }
        KeyCode::Down | KeyCode::Char('j' | 'J') => {
            let FieldFocus::Row(n) = editor.active_field;
            let max = max_row_for_tab(editor, config);
            editor.active_field = FieldFocus::Row((n + 1).min(max));
        }
        KeyCode::Enter => {
            match editor.active_tab {
                super::super::state::EditorTab::General => open_editor_field_modal(editor),
                super::super::state::EditorTab::Mounts => {
                    // Enter on the "+ Add mount" sentinel row triggers add flow.
                    let FieldFocus::Row(n) = editor.active_field;
                    if n == editor.pending.mounts.len() {
                        editor.modal = Some(Modal::FileBrowser {
                            target: FileBrowserTarget::EditAddMountSrc,
                            state: FileBrowserState::new_from_home()?,
                        });
                    }
                    // Enter on an existing mount row: no-op for now.
                }
                _ => {}
            }
        }
        KeyCode::Char(' ') if editor.active_tab == super::super::state::EditorTab::Agents => {
            toggle_agent_allowed_at_cursor(editor, config);
        }
        KeyCode::Char('D' | 'd') if editor.active_tab == super::super::state::EditorTab::Agents => {
            set_default_agent_at_cursor(editor, config);
        }
        KeyCode::Char('a' | 'A') if editor.active_tab == super::super::state::EditorTab::Mounts => {
            editor.modal = Some(Modal::FileBrowser {
                target: FileBrowserTarget::EditAddMountSrc,
                state: FileBrowserState::new_from_home()?,
            });
        }
        KeyCode::Char('d' | 'D') if editor.active_tab == super::super::state::EditorTab::Mounts => {
            remove_mount_at_cursor(editor);
        }
        KeyCode::Char('r' | 'R') if editor.active_tab == super::super::state::EditorTab::Mounts => {
            // Flip the `readonly` flag on the highlighted mount row. Silent
            // no-op on the `+ Add mount` sentinel. The change propagates
            // through `change_count`/`is_dirty` via the standard diff-based
            // path (no extra plumbing — a flipped `readonly` makes the
            // pending mount non-equal to the original, so the mount counts
            // as removed + added and nets a +2 delta until flipped back).
            let FieldFocus::Row(n) = editor.active_field;
            if let Some(m) = editor.pending.mounts.get_mut(n) {
                m.readonly = !m.readonly;
            }
        }
        KeyCode::Char('o' | 'O') if editor.active_tab == super::super::state::EditorTab::Mounts => {
            // Open the highlighted mount's GitHub URL in the system browser.
            // Silent no-op when the cursor is on the `+ Add mount` sentinel,
            // or when the row's MountKind doesn't expose a resolvable URL
            // (non-GitHub remotes, repos without `origin`, plain folders).
            // On non-GitHub mounts we emit a toast so the hint is discoverable.
            let FieldFocus::Row(n) = editor.active_field;
            if let Some(m) = editor.pending.mounts.get(n) {
                let kind = super::super::mount_info::inspect(&m.src);
                match kind {
                    super::super::mount_info::MountKind::Git {
                        host: super::super::mount_info::GitHost::Github,
                        web_url: Some(url),
                        ..
                    } => {
                        // End the editor borrow before we set `state.toast`.
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
            // Sentinel row (n == mounts.len()): silent no-op.
        }
        _ => {}
    }
    Ok(InputOutcome::Continue)
}

/// Returns the highest valid `FieldFocus::Row` index for the current tab.
fn max_row_for_tab(editor: &EditorState<'_>, config: &AppConfig) -> usize {
    use super::super::state::EditorTab;
    match editor.active_tab {
        EditorTab::General => match editor.mode {
            // Edit: name (0), workdir (1), default_agent (2), last_used (3)
            EditorMode::Edit { .. } => 3,
            // Create: name read-only (0), workdir (1)
            EditorMode::Create => 1,
        },
        EditorTab::Mounts => editor.pending.mounts.len(), // mounts fill 0..N-1, sentinel at N
        EditorTab::Agents => config.agents.len().saturating_sub(1), // 0-based into agents
        EditorTab::Secrets => 0,
    }
}

fn open_editor_field_modal(editor: &mut EditorState<'_>) {
    use super::super::super::widgets::text_input::TextInputState;
    use super::super::state::EditorTab;
    if editor.active_tab == EditorTab::General {
        let FieldFocus::Row(n) = editor.active_field;
        match n {
            0 => {
                // Name — editable in both Edit and Create modes. The
                // TextInput is pre-filled with the current pending name
                // (in Create mode that's the value captured by the
                // create-prelude; in Edit mode it's the workspace's
                // current on-disk name unless the operator has already
                // staged a rename).
                let current = match &editor.mode {
                    EditorMode::Edit { name } => {
                        editor.pending_name.clone().unwrap_or_else(|| name.clone())
                    }
                    EditorMode::Create => editor.pending_name.clone().unwrap_or_default(),
                };
                editor.modal = Some(Modal::TextInput {
                    target: super::super::state::TextInputTarget::Name,
                    state: TextInputState::new("Rename workspace", current),
                });
            }
            1 if !editor.pending.mounts.is_empty() => {
                // workdir — use WorkdirPick if mounts exist
                editor.modal = Some(Modal::WorkdirPick {
                    state: WorkdirPickState::from_mounts(&editor.pending.mounts),
                });
            }
            _ => {}
        }
    }
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

    // Read the "all" state via the shared helper before taking the mutable
    // borrow on `allowed_agents` below — Rust borrow rules bar the call
    // otherwise. See `super::agent_allow` for the shorthand rule.
    let is_all_mode = super::super::agent_allow::allows_all_agents(&editor.pending);
    let list = &mut editor.pending.allowed_agents;
    let in_list = list.iter().position(|a| a == agent);

    if is_all_mode {
        // "all" mode → effective-allowed. Demote to "custom" without this
        // agent by enumerating the full roster minus the current row.
        *list = agent_names
            .iter()
            .filter(|a| a.as_str() != agent.as_str())
            .cloned()
            .collect();
        if editor.pending.default_agent.as_deref() == Some(agent.as_str()) {
            editor.pending.default_agent = None;
        }
    } else if let Some(pos) = in_list {
        // "custom" mode, row is present → remove it. A resulting empty
        // list reverts to "all" shorthand on the next render tick.
        list.remove(pos);
        if editor.pending.default_agent.as_deref() == Some(agent.as_str()) {
            editor.pending.default_agent = None;
        }
    } else {
        // "custom" mode, row absent → add it. If the addition fills in the
        // complete roster, collapse back to the "all" shorthand (empty
        // list) so the status badge reads `all` rather than
        // `custom (N of N allowed)` — the two states are semantically
        // identical and the shorthand is less noisy.
        list.push(agent.clone());
        if list.len() == agent_names.len() && agent_names.iter().all(|a| list.contains(a)) {
            list.clear();
        }
    }
}

fn set_default_agent_at_cursor(editor: &mut EditorState<'_>, config: &AppConfig) {
    let FieldFocus::Row(n) = editor.active_field;
    let agent_names: Vec<String> = config.agents.keys().cloned().collect();
    if let Some(agent) = agent_names.get(n) {
        // In "all agents allowed" shorthand (empty list) the agent is
        // already effectively allowed — don't collapse the shorthand into
        // a single-agent allow list just because the operator picked a
        // default. Only append when we're already in "custom" mode and
        // the new default isn't in the list yet.
        if !super::super::agent_allow::allows_all_agents(&editor.pending)
            && !editor.pending.allowed_agents.contains(agent)
        {
            editor.pending.allowed_agents.push(agent.clone());
        }
        editor.pending.default_agent = Some(agent.clone());
    }
}

fn remove_mount_at_cursor(editor: &mut EditorState<'_>) {
    let FieldFocus::Row(n) = editor.active_field;
    if n < editor.pending.mounts.len() {
        editor.pending.mounts.remove(n);
    }
}

#[allow(clippy::too_many_lines)]
pub(super) fn handle_editor_modal(editor: &mut EditorState<'_>, key: KeyEvent) {
    let Some(modal) = editor.modal.as_mut() else {
        return;
    };
    match modal {
        Modal::TextInput { target, state } => match state.handle_key(key) {
            ModalOutcome::Commit(value) => {
                let target = *target;
                editor.modal = None;
                apply_text_input_to_pending(target, editor, &value);
            }
            ModalOutcome::Cancel => {
                editor.modal = None;
            }
            ModalOutcome::Continue => {}
        },
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
        Modal::Confirm { target: _, state } => match state.handle_key(key) {
            // Editor-side Confirm only reaches here for non-destructive
            // variants now that SaveCollapse folds into ConfirmSave.
            // Treat Commit/Cancel identically — close the modal.
            ModalOutcome::Commit(_) | ModalOutcome::Cancel => {
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
            use crate::launch::widgets::save_discard::SaveDiscardChoice;
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
        // GithubPicker is a list-view modal — the editor never opens it.
        // If one somehow ends up here, treat any key as cancel so the
        // operator isn't stuck.
        Modal::GithubPicker { .. } => {
            editor.modal = None;
        }
        Modal::ConfirmSave { state: modal_state } => {
            use crate::launch::widgets::confirm_save::SaveChoice;
            match modal_state.handle_key(key) {
                ModalOutcome::Commit(SaveChoice::Save) => {
                    // Transition `Confirming → PendingCommit` atomically so
                    // the plan and the originating `exit_on_success` flag
                    // travel together. The outer handler (which has
                    // `paths` / `cwd`) drains this and drives the write.
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
    }
}

pub(super) fn apply_text_input_to_pending(
    target: super::super::state::TextInputTarget,
    editor: &mut EditorState<'_>,
    value: &str,
) {
    use super::super::state::TextInputTarget;
    match target {
        TextInputTarget::Name => {
            // Both Edit and Create modes stash the pending name on the editor.
            // Save-time plumbing distinguishes: Edit calls rename_workspace,
            // Create passes it to create_workspace.
            editor.pending_name = Some(value.to_string());
        }
        TextInputTarget::Workdir => editor.pending.workdir = value.to_string(),
        TextInputTarget::MountDst => {
            // Completing the add-mount flow: we stashed the src when the
            // FileBrowser committed; see apply_file_browser_to_editor.
            // At this point editor.pending.mounts already has a provisional
            // entry with src == dst == default_path. Update its dst.
            if let Some(last) = editor.pending.mounts.last_mut() {
                last.dst = value.to_string();
            }
        }
    }
}

/// Dispatch the three outcomes of `MountDstChoiceState::handle_key` into
/// concrete editor mutations. Only the `EditAddMountSrc` target is
/// meaningful here — the prelude's `CreateFirstMountSrc` target is routed
/// through `handle_prelude_modal` instead.
fn dispatch_editor_mount_dst_choice(
    editor: &mut EditorState<'_>,
    target: FileBrowserTarget,
    src: &str,
    outcome: &ModalOutcome<crate::launch::widgets::mount_dst_choice::MountDstChoice>,
) {
    use crate::launch::widgets::mount_dst_choice::MountDstChoice;
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
                    state: crate::launch::widgets::text_input::TextInputState::new(
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
    use crate::launch::widgets::mount_dst_choice::MountDstChoiceState;
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
