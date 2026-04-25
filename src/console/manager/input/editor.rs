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
                            state: crate::console::widgets::save_discard::SaveDiscardState::new(
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
        // GithubPicker is a list-view modal — the editor never opens it.
        // If one somehow ends up here, treat any key as cancel so the
        // operator isn't stuck.
        Modal::GithubPicker { .. } => {
            editor.modal = None;
        }
        Modal::ConfirmSave { state: modal_state } => {
            use crate::console::widgets::confirm_save::SaveChoice;
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
        TextInputTarget,
    };
    use super::super::test_support::{key, mount};
    use super::{apply_file_browser_to_editor, apply_text_input_to_pending, handle_editor_modal};
    use crate::config::AppConfig;
    use crate::console::manager::input::handle_key;
    use crate::paths::JackinPaths;
    use crate::workspace::{MountConfig, WorkspaceConfig};
    use crossterm::event::KeyCode;
    use tempfile::TempDir;

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
                isolation: crate::isolation::MountIsolation::Shared,
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
                assert_eq!(*target, TextInputTarget::Name);
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

        apply_text_input_to_pending(TextInputTarget::Name, &mut editor, "new-name");

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
                assert_eq!(*target, TextInputTarget::Name);
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
        handle_editor_modal(&mut editor, key(KeyCode::Char('o')));
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
        handle_editor_modal(&mut editor, key(KeyCode::Char('e')));
        match &editor.modal {
            Some(Modal::TextInput { target, .. }) => {
                assert_eq!(*target, TextInputTarget::MountDst);
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
        handle_editor_modal(&mut editor, key(KeyCode::Esc));
        assert!(editor.modal.is_none(), "Esc closes the modal");
        assert_eq!(
            editor.pending.mounts.len(),
            0,
            "Cancel must not push a mount"
        );

        let mut editor = editor_with_browser_committed("/host/path");
        handle_editor_modal(&mut editor, key(KeyCode::Char('c')));
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
        // Left should match BackTab's reverse cycle: Mounts → General.
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

    // ── Agents tab: D-key default binding ──────────────────────────────

    #[test]
    fn d_key_sets_default_agent_on_current_row() {
        let mut config = config_with_agents(&["alpha", "beta", "gamma"]);
        // Cursor on row 1 (agent "beta"), no default set yet. The
        // workspace starts in the "all agents allowed" shorthand (empty
        // `allowed_agents`), so picking a default must NOT collapse the
        // shorthand into a single-agent allow list — see finding #1.
        let mut state = editor_on_agents_tab(empty_ws(), 1);

        press(&mut state, &mut config, KeyCode::Char('D')).unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert_eq!(
            e.pending.default_agent.as_deref(),
            Some("beta"),
            "D on row 1 should pin agent `beta` as default",
        );
        assert!(
            e.pending.allowed_agents.is_empty(),
            "default-agent pick must preserve the all-agents shorthand \
             (empty allowed_agents); got {:?}",
            e.pending.allowed_agents,
        );
    }

    #[test]
    fn d_key_preserves_all_agents_shorthand() {
        // Explicit guard on the shorthand-preservation behavior: setting
        // a default on a workspace in "all agents" mode must leave the
        // allow list empty, not switch it to a one-agent custom list.
        let mut config = config_with_agents(&["alpha", "beta", "gamma"]);
        let mut state = editor_on_agents_tab(empty_ws(), 2);
        {
            let ManagerStage::Editor(e) = &state.stage else {
                panic!("editor stage expected");
            };
            assert!(
                e.pending.allowed_agents.is_empty(),
                "precondition: workspace should start in all-agents mode",
            );
        }

        press(&mut state, &mut config, KeyCode::Char('D')).unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert!(
            e.pending.allowed_agents.is_empty(),
            "all-agents shorthand must survive D; got {:?}",
            e.pending.allowed_agents,
        );
        assert_eq!(e.pending.default_agent.as_deref(), Some("gamma"));
    }

    #[test]
    fn d_key_appends_to_custom_allow_list_when_missing() {
        // Complementary case: when the workspace is already in "custom"
        // mode (non-empty allow list) and the chosen default is NOT in
        // the list, pressing D must append it — otherwise the config
        // would reference a forbidden default.
        let mut config = config_with_agents(&["alpha", "beta", "gamma"]);
        let mut ws = empty_ws();
        ws.allowed_agents = vec!["alpha".into()];
        // Cursor on row 1 (agent "beta"), which is NOT in the allow list.
        let mut state = editor_on_agents_tab(ws, 1);

        press(&mut state, &mut config, KeyCode::Char('D')).unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert_eq!(e.pending.default_agent.as_deref(), Some("beta"));
        assert_eq!(
            e.pending.allowed_agents,
            vec!["alpha".to_string(), "beta".to_string()],
            "custom allow list must pick up the new default when missing",
        );
    }

    #[test]
    fn lowercase_d_key_sets_default_agent_on_current_row() {
        // Operators often hit `d` without holding shift; the binding
        // must accept both cases.
        let mut config = config_with_agents(&["alpha", "beta"]);
        let mut state = editor_on_agents_tab(empty_ws(), 0);

        press(&mut state, &mut config, KeyCode::Char('d')).unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert_eq!(e.pending.default_agent.as_deref(), Some("alpha"));
    }

    #[test]
    fn star_key_no_longer_sets_default_agent() {
        // Regression guard: the legacy `*` binding was removed in favour
        // of `D`. Pressing `*` on an agent row must now be a no-op.
        let mut config = config_with_agents(&["alpha", "beta"]);
        let mut state = editor_on_agents_tab(empty_ws(), 1);

        press(&mut state, &mut config, KeyCode::Char('*')).unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert!(
            e.pending.default_agent.is_none(),
            "`*` must no longer set the default agent",
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
            crate::console::manager::render::render_editor(f, editor, &config);
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
}
