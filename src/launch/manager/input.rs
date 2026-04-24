//! Key dispatch for the workspace manager. Modal-first precedence:
//! if a modal is open, events go to the modal handler; otherwise they
//! go to the active stage's handler.

use crossterm::event::{KeyCode, KeyEvent};

use crate::config::AppConfig;
use crate::paths::JackinPaths;
use super::state::{
    EditorState, FileBrowserTarget, ManagerStage, ManagerState, Modal,
    Toast, ToastKind,
};
use super::super::widgets::{
    confirm::ConfirmState, file_browser::FileBrowserState, ModalOutcome,
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
    _state: &mut ManagerState<'_>,
    _config: &mut AppConfig,
    _paths: &JackinPaths,
    _key: KeyEvent,
) -> anyhow::Result<InputOutcome> {
    // Stub — full implementation in Task 16.
    Ok(InputOutcome::Continue)
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
    // Route to the active modal's handle_key. For Task 14 we scaffold
    // only the close-on-Esc path for each variant; real commit handling
    // lives in Task 16 (editor) / Task 17 (prelude).
    let Some(modal) = editor.modal.as_mut() else { return; };
    match modal {
        Modal::TextInput { state, .. } => {
            if matches!(state.handle_key(key), ModalOutcome::Cancel) {
                editor.modal = None;
            }
        }
        Modal::FileBrowser { state, .. } => {
            if matches!(state.handle_key(key), ModalOutcome::Cancel) {
                editor.modal = None;
            }
        }
        Modal::WorkdirPick { state } => {
            if matches!(state.handle_key(key), ModalOutcome::Cancel) {
                editor.modal = None;
            }
        }
        Modal::Confirm { state, .. } => {
            if matches!(state.handle_key(key), ModalOutcome::Cancel) {
                editor.modal = None;
            }
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
