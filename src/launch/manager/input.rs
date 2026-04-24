//! Key dispatch for the workspace manager. Modal-first precedence:
//! if a modal is open, events go to the modal handler; otherwise they
//! go to the active stage's handler.

use crossterm::event::{KeyCode, KeyEvent};

use crate::config::AppConfig;
use crate::paths::JackinPaths;
use super::state::{
    ConfirmTarget, EditorMode, EditorState, EditorTab, FieldFocus,
    FileBrowserTarget, ManagerStage, ManagerState, Modal, Toast, ToastKind,
};
use super::super::widgets::{
    confirm::ConfirmState, file_browser::FileBrowserState, ModalOutcome,
    workdir_pick::WorkdirPickState,
};

pub enum InputOutcome {
    /// Stay in the manager.
    Continue,
    /// Back to the launcher's Workspace stage.
    ExitToLauncher,
}

pub fn handle_key(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    paths: &JackinPaths,
    key: KeyEvent,
) -> anyhow::Result<InputOutcome> {
    // Modal precedence: if a modal is open, it gets the event.
    // Use a discriminant check so we can take &mut without keeping an
    // immutable borrow alive across the call.
    if let ManagerStage::Editor(editor) = &mut state.stage {
        if editor.modal.is_some() {
            handle_editor_modal(editor, key);
            return Ok(InputOutcome::Continue);
        }
    }
    if let ManagerStage::CreatePrelude(prelude) = &mut state.stage {
        if prelude.modal.is_some() {
            handle_prelude_modal(prelude, key);
            return Ok(InputOutcome::Continue);
        }
    }

    // Non-modal routing per stage — capture which stage we're in as a
    // simple enum discriminant so the immutable borrow ends before we
    // pass &mut state into the stage handler.
    enum StageDis { List, Editor, CreatePrelude, ConfirmDelete }
    let dis = match &state.stage {
        ManagerStage::List => StageDis::List,
        ManagerStage::Editor(_) => StageDis::Editor,
        ManagerStage::CreatePrelude(_) => StageDis::CreatePrelude,
        ManagerStage::ConfirmDelete { .. } => StageDis::ConfirmDelete,
    };

    match dis {
        StageDis::List => handle_list_key(state, config, paths, key),
        StageDis::Editor => handle_editor_key(state, config, paths, key),
        StageDis::CreatePrelude => handle_prelude_key(state, config, paths, key),
        StageDis::ConfirmDelete => handle_confirm_delete_key(state, config, paths, key),
    }
}

fn handle_list_key(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    _paths: &JackinPaths,
    key: KeyEvent,
) -> anyhow::Result<InputOutcome> {
    match key.code {
        KeyCode::Esc => Ok(InputOutcome::ExitToLauncher),
        KeyCode::Up | KeyCode::Char('k') => {
            state.selected = state.selected.saturating_sub(1);
            Ok(InputOutcome::Continue)
        }
        KeyCode::Down | KeyCode::Char('j') => {
            // +1 because the "+ New workspace" row is also selectable.
            let max = state.workspaces.len();
            state.selected = (state.selected + 1).min(max);
            Ok(InputOutcome::Continue)
        }
        KeyCode::Enter => {
            if state.selected == state.workspaces.len() {
                // [+ New workspace] sentinel: start the create prelude
                // with a FileBrowser modal open.
                let mut prelude = super::state::CreatePreludeState::new();
                prelude.modal = Some(Modal::FileBrowser {
                    target: FileBrowserTarget::CreateFirstMountSrc,
                    state: FileBrowserState::new_from_home()?,
                });
                state.stage = ManagerStage::CreatePrelude(prelude);
            } else if let Some(summary) = state.workspaces.get(state.selected) {
                // Edit existing workspace — load full WorkspaceConfig from AppConfig.
                let name = summary.name.clone();
                if let Some(ws) = config.workspaces.get(&name) {
                    state.stage = ManagerStage::Editor(EditorState::new_edit(
                        name,
                        ws.clone(),
                    ));
                }
            }
            Ok(InputOutcome::Continue)
        }
        KeyCode::Char('n') => {
            let mut prelude = super::state::CreatePreludeState::new();
            prelude.modal = Some(Modal::FileBrowser {
                target: FileBrowserTarget::CreateFirstMountSrc,
                state: FileBrowserState::new_from_home()?,
            });
            state.stage = ManagerStage::CreatePrelude(prelude);
            Ok(InputOutcome::Continue)
        }
        KeyCode::Char('d') => {
            if let Some(ws) = state.workspaces.get(state.selected) {
                let name = ws.name.clone();
                state.stage = ManagerStage::ConfirmDelete {
                    name: name.clone(),
                    state: ConfirmState::new(format!("Delete \"{name}\"?")),
                };
            }
            Ok(InputOutcome::Continue)
        }
        _ => Ok(InputOutcome::Continue),
    }
}

fn handle_editor_key(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    paths: &JackinPaths,
    key: KeyEvent,
) -> anyhow::Result<InputOutcome> {
    // Handle s and Esc outside the editor borrow to avoid re-borrow
    // conflicts (both need to call back into state or config).
    match key.code {
        KeyCode::Char('s') => {
            if matches!(&state.stage, ManagerStage::Editor(_)) {
                save_editor(state, config, paths)?;
            }
            return Ok(InputOutcome::Continue);
        }
        KeyCode::Esc => {
            // Re-borrow pattern — immutable read first, then mutate.
            if let ManagerStage::Editor(editor) = &state.stage {
                let dirty = editor.is_dirty();
                if dirty {
                    if let ManagerStage::Editor(editor) = &mut state.stage {
                        editor.modal = Some(Modal::Confirm {
                            target: ConfirmTarget::DiscardChanges,
                            state: ConfirmState::new("Discard unsaved changes?"),
                        });
                    }
                } else {
                    *state = ManagerState::from_config(config);
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
        KeyCode::Tab => {
            editor.active_tab = match editor.active_tab {
                EditorTab::General => EditorTab::Mounts,
                EditorTab::Mounts => EditorTab::Agents,
                EditorTab::Agents => EditorTab::Secrets,
                EditorTab::Secrets => EditorTab::General,
            };
            editor.active_field = FieldFocus::Row(0);
        }
        KeyCode::BackTab => {
            editor.active_tab = match editor.active_tab {
                EditorTab::General => EditorTab::Secrets,
                EditorTab::Mounts => EditorTab::General,
                EditorTab::Agents => EditorTab::Mounts,
                EditorTab::Secrets => EditorTab::Agents,
            };
            editor.active_field = FieldFocus::Row(0);
        }
        KeyCode::Up | KeyCode::Char('k') => {
            let FieldFocus::Row(n) = editor.active_field;
            editor.active_field = FieldFocus::Row(n.saturating_sub(1));
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let FieldFocus::Row(n) = editor.active_field;
            editor.active_field = FieldFocus::Row(n + 1);
        }
        KeyCode::Enter => {
            open_editor_field_modal(editor);
        }
        KeyCode::Char(' ') if editor.active_tab == EditorTab::Agents => {
            toggle_agent_allowed_at_cursor(editor, config);
        }
        KeyCode::Char('*') if editor.active_tab == EditorTab::Agents => {
            set_default_agent_at_cursor(editor);
        }
        KeyCode::Char('a') if editor.active_tab == EditorTab::Mounts => {
            editor.modal = Some(Modal::FileBrowser {
                target: FileBrowserTarget::EditAddMountSrc,
                state: FileBrowserState::new_from_home()?,
            });
        }
        KeyCode::Char('d') if editor.active_tab == EditorTab::Mounts => {
            remove_mount_at_cursor(editor);
        }
        _ => {}
    }
    Ok(InputOutcome::Continue)
}

fn open_editor_field_modal(editor: &mut EditorState<'_>) {
    match editor.active_tab {
        EditorTab::General => {
            let FieldFocus::Row(n) = editor.active_field;
            match n {
                1 => {
                    // workdir — use WorkdirPick if mounts exist
                    if !editor.pending.mounts.is_empty() {
                        editor.modal = Some(Modal::WorkdirPick {
                            state: WorkdirPickState::from_mounts(&editor.pending.mounts),
                        });
                    }
                }
                _ => {}
            }
        }
        _ => {}
    }
}

fn toggle_agent_allowed_at_cursor(editor: &mut EditorState<'_>, config: &AppConfig) {
    let FieldFocus::Row(n) = editor.active_field;
    if n == 0 { return; }  // header row
    let idx = n - 1;
    let agent_names: Vec<String> = config.agents.keys().cloned().collect();
    if let Some(agent) = agent_names.get(idx) {
        if let Some(pos) = editor.pending.allowed_agents.iter().position(|a| a == agent) {
            editor.pending.allowed_agents.remove(pos);
            if editor.pending.default_agent.as_deref() == Some(agent) {
                editor.pending.default_agent = None;
            }
        } else {
            editor.pending.allowed_agents.push(agent.clone());
        }
    }
}

fn set_default_agent_at_cursor(editor: &mut EditorState<'_>) {
    let FieldFocus::Row(n) = editor.active_field;
    if n == 0 { return; }
    let idx = n - 1;
    if let Some(agent) = editor.pending.allowed_agents.get(idx).cloned() {
        editor.pending.default_agent = Some(agent);
    }
}

fn remove_mount_at_cursor(editor: &mut EditorState<'_>) {
    let FieldFocus::Row(n) = editor.active_field;
    if n < editor.pending.mounts.len() {
        editor.pending.mounts.remove(n);
    }
}

fn save_editor(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    paths: &JackinPaths,
) -> anyhow::Result<()> {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return Ok(());
    };
    let mut ce = crate::config::ConfigEditor::open(paths)?;
    match &editor.mode {
        EditorMode::Edit { name } => {
            let name = name.clone();
            let edit = build_workspace_edit(&editor.original, &editor.pending);
            if let Err(e) = ce.edit_workspace(&name, edit) {
                editor.error_banner = Some(e.to_string());
                return Ok(());
            }
        }
        EditorMode::Create => {
            let Some(name) = editor.pending_name.clone() else {
                editor.error_banner = Some("missing workspace name".into());
                return Ok(());
            };
            if let Err(e) = ce.create_workspace(&name, editor.pending.clone()) {
                editor.error_banner = Some(e.to_string());
                return Ok(());
            }
        }
    }
    match ce.save() {
        Ok(fresh) => {
            *config = fresh;
            // Refresh editor original/pending from the new config.
            if let ManagerStage::Editor(editor) = &mut state.stage {
                let change_count = editor.change_count();
                match &editor.mode {
                    EditorMode::Edit { name } => {
                        if let Some(ws) = config.workspaces.get(name) {
                            editor.original = ws.clone();
                            editor.pending = ws.clone();
                        }
                    }
                    EditorMode::Create => {
                        // After create, jump back to manager list with toast.
                    }
                }
                editor.error_banner = None;
                state.toast = Some(Toast {
                    message: format!("saved · {change_count} changes written"),
                    kind: ToastKind::Success,
                    shown_at: std::time::Instant::now(),
                });
            }
        }
        Err(e) => {
            if let ManagerStage::Editor(editor) = &mut state.stage {
                editor.error_banner = Some(e.to_string());
            }
        }
    }
    Ok(())
}

fn build_workspace_edit(
    original: &crate::workspace::WorkspaceConfig,
    pending: &crate::workspace::WorkspaceConfig,
) -> crate::workspace::WorkspaceEdit {
    let mut edit = crate::workspace::WorkspaceEdit::default();
    if pending.workdir != original.workdir {
        edit.workdir = Some(pending.workdir.clone());
    }
    for m in &pending.mounts {
        if !original.mounts.iter().any(|o| o == m) {
            edit.upsert_mounts.push(m.clone());
        }
    }
    for o in &original.mounts {
        if !pending.mounts.iter().any(|p| p.dst == o.dst) {
            edit.remove_destinations.push(o.dst.clone());
        }
    }
    for a in &pending.allowed_agents {
        if !original.allowed_agents.contains(a) {
            edit.allowed_agents_to_add.push(a.clone());
        }
    }
    for a in &original.allowed_agents {
        if !pending.allowed_agents.contains(a) {
            edit.allowed_agents_to_remove.push(a.clone());
        }
    }
    if pending.default_agent != original.default_agent {
        edit.default_agent = Some(pending.default_agent.clone());
    }
    edit
}

fn handle_prelude_key(
    _state: &mut ManagerState<'_>,
    _config: &mut AppConfig,
    _paths: &JackinPaths,
    _key: KeyEvent,
) -> anyhow::Result<InputOutcome> {
    // Stub — full implementation in Task 17.
    Ok(InputOutcome::Continue)
}

fn handle_confirm_delete_key(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    paths: &JackinPaths,
    key: KeyEvent,
) -> anyhow::Result<InputOutcome> {
    let ManagerStage::ConfirmDelete { name, state: confirm_state } = &mut state.stage else {
        return Ok(InputOutcome::Continue);
    };
    let outcome = confirm_state.handle_key(key);
    let ws_name = name.clone();
    match outcome {
        ModalOutcome::Commit(true) => {
            let mut editor = crate::config::ConfigEditor::open(paths)?;
            editor.remove_workspace(&ws_name)?;
            *config = editor.save()?;
            *state = ManagerState::from_config(config);
            state.toast = Some(Toast {
                message: format!("deleted \"{ws_name}\""),
                kind: ToastKind::Success,
                shown_at: std::time::Instant::now(),
            });
            Ok(InputOutcome::Continue)
        }
        ModalOutcome::Commit(false) | ModalOutcome::Cancel => {
            state.stage = ManagerStage::List;
            Ok(InputOutcome::Continue)
        }
        ModalOutcome::Continue => Ok(InputOutcome::Continue),
    }
}

fn handle_editor_modal(editor: &mut EditorState<'_>, key: KeyEvent) {
    let Some(modal) = editor.modal.as_mut() else { return; };
    match modal {
        Modal::TextInput { target, state } => {
            match state.handle_key(key) {
                ModalOutcome::Commit(value) => {
                    let target = *target;
                    editor.modal = None;
                    apply_text_input_to_pending(target, editor, &value);
                }
                ModalOutcome::Cancel => {
                    editor.modal = None;
                }
                ModalOutcome::Continue => {}
            }
        }
        Modal::FileBrowser { target, state } => {
            match state.handle_key(key) {
                ModalOutcome::Commit(path) => {
                    let target = *target;
                    editor.modal = None;
                    apply_file_browser_to_editor(target, editor, path);
                }
                ModalOutcome::Cancel => {
                    editor.modal = None;
                }
                ModalOutcome::Continue => {}
            }
        }
        Modal::WorkdirPick { state } => {
            match state.handle_key(key) {
                ModalOutcome::Commit(workdir) => {
                    editor.pending.workdir = workdir;
                    editor.modal = None;
                }
                ModalOutcome::Cancel => {
                    editor.modal = None;
                }
                ModalOutcome::Continue => {}
            }
        }
        Modal::Confirm { target, state } => {
            let target = *target;
            match state.handle_key(key) {
                ModalOutcome::Commit(yes) => {
                    editor.modal = None;
                    if target == ConfirmTarget::DiscardChanges && yes {
                        // Caller transitions out of editor — we set a
                        // flag by making the editor look "clean" again.
                        editor.pending = editor.original.clone();
                        editor.error_banner = None;
                        // The transition back to List happens on next Esc.
                    }
                }
                ModalOutcome::Cancel => {
                    editor.modal = None;
                }
                ModalOutcome::Continue => {}
            }
        }
    }
}

fn apply_text_input_to_pending(
    target: super::state::TextInputTarget,
    editor: &mut EditorState<'_>,
    value: &str,
) {
    use super::state::TextInputTarget;
    match target {
        TextInputTarget::Name => {
            // Rename in Edit mode requires AppConfig key rotation (future work).
            // In Create mode the name lives on the prelude, not editor.
            let _ = value;
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

fn apply_file_browser_to_editor(
    target: FileBrowserTarget,
    editor: &mut EditorState<'_>,
    path: std::path::PathBuf,
) {
    use super::super::widgets::text_input::TextInputState;
    match target {
        FileBrowserTarget::EditAddMountSrc => {
            // Provisional mount with dst defaulting to same as src.
            editor.pending.mounts.push(crate::workspace::MountConfig {
                src: path.display().to_string(),
                dst: path.display().to_string(),
                readonly: false,
            });
            // Open a TextInput for dst refinement (pre-filled with path).
            editor.modal = Some(Modal::TextInput {
                target: super::state::TextInputTarget::MountDst,
                state: TextInputState::new(
                    "Mount dst (default: same as host path)",
                    path.display().to_string(),
                ),
            });
        }
        FileBrowserTarget::CreateFirstMountSrc => {
            // Only meaningful in prelude path — Task 17 handles this.
            let _ = (editor, path);
        }
    }
}

fn handle_prelude_modal(prelude: &mut super::state::CreatePreludeState<'_>, key: KeyEvent) {
    // Stub — full wizard chain in Task 17. For now, only close-on-Esc.
    let Some(modal) = prelude.modal.as_mut() else { return; };
    match modal {
        Modal::TextInput { state, .. } => {
            if matches!(state.handle_key(key), ModalOutcome::Cancel) { prelude.modal = None; }
        }
        Modal::FileBrowser { state, .. } => {
            if matches!(state.handle_key(key), ModalOutcome::Cancel) { prelude.modal = None; }
        }
        Modal::WorkdirPick { state } => {
            if matches!(state.handle_key(key), ModalOutcome::Cancel) { prelude.modal = None; }
        }
        Modal::Confirm { state, .. } => {
            if matches!(state.handle_key(key), ModalOutcome::Cancel) { prelude.modal = None; }
        }
    }
}
