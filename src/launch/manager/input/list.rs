//! List-stage dispatch: workspace-picker key handling and the
//! list-level modal (`GithubPicker`).

use crossterm::event::{KeyCode, KeyEvent};

use super::super::super::widgets::{
    ModalOutcome, confirm::ConfirmState, file_browser::FileBrowserState,
};
use super::super::state::{
    EditorState, FileBrowserTarget, ManagerListRow, ManagerStage, ManagerState, Modal, Toast,
    ToastKind,
};
use super::InputOutcome;
use crate::config::AppConfig;
use crate::paths::JackinPaths;

pub(super) fn handle_list_key(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    _paths: &JackinPaths,
    _cwd: &std::path::Path,
    key: KeyEvent,
) -> anyhow::Result<InputOutcome> {
    // See ManagerListRow docs for row layout.
    match key.code {
        KeyCode::Esc | KeyCode::Char('q' | 'Q') => Ok(InputOutcome::ExitJackin),
        KeyCode::Up | KeyCode::Char('k' | 'K') => {
            state.selected = state.selected.saturating_sub(1);
            Ok(InputOutcome::Continue)
        }
        KeyCode::Down | KeyCode::Char('j' | 'J') => {
            state.selected = (state.selected + 1).min(state.row_count() - 1);
            Ok(InputOutcome::Continue)
        }
        KeyCode::Enter => match state.selected_row() {
            ManagerListRow::CurrentDirectory => {
                // Launch against cwd. Run-loop routes through the same
                // agent-picker stage as LaunchNamed.
                Ok(InputOutcome::LaunchCurrentDir)
            }
            ManagerListRow::NewWorkspace => {
                // Start the create prelude with a FileBrowser modal open.
                let mut prelude = super::super::state::CreatePreludeState::new();
                prelude.modal = Some(Modal::FileBrowser {
                    target: FileBrowserTarget::CreateFirstMountSrc,
                    state: FileBrowserState::new_from_home()?,
                });
                state.stage = ManagerStage::CreatePrelude(prelude);
                Ok(InputOutcome::Continue)
            }
            ManagerListRow::SavedWorkspace(i) => Ok(state
                .workspaces
                .get(i)
                .map_or(InputOutcome::Continue, |summary| {
                    InputOutcome::LaunchNamed(summary.name.clone())
                })),
        },
        KeyCode::Char('e' | 'E') => {
            match state.selected_row() {
                ManagerListRow::CurrentDirectory => {
                    state.toast = Some(Toast {
                        message: "Current directory cannot be edited".into(),
                        kind: ToastKind::Error,
                        shown_at: std::time::Instant::now(),
                    });
                }
                ManagerListRow::NewWorkspace => {
                    // Silent no-op on the sentinel.
                }
                ManagerListRow::SavedWorkspace(i) => {
                    if let Some(summary) = state.workspaces.get(i) {
                        let name = summary.name.clone();
                        if let Some(ws) = config.workspaces.get(&name) {
                            state.stage =
                                ManagerStage::Editor(EditorState::new_edit(name, ws.clone()));
                        }
                    }
                }
            }
            Ok(InputOutcome::Continue)
        }
        KeyCode::Char('n' | 'N') => {
            let mut prelude = super::super::state::CreatePreludeState::new();
            prelude.modal = Some(Modal::FileBrowser {
                target: FileBrowserTarget::CreateFirstMountSrc,
                state: FileBrowserState::new_from_home()?,
            });
            state.stage = ManagerStage::CreatePrelude(prelude);
            Ok(InputOutcome::Continue)
        }
        KeyCode::Char('d' | 'D') => {
            match state.selected_row() {
                ManagerListRow::CurrentDirectory => {
                    state.toast = Some(Toast {
                        message: "Current directory cannot be deleted".into(),
                        kind: ToastKind::Error,
                        shown_at: std::time::Instant::now(),
                    });
                }
                ManagerListRow::NewWorkspace => {
                    // Silent no-op on the sentinel.
                }
                ManagerListRow::SavedWorkspace(i) => {
                    if let Some(ws) = state.workspaces.get(i) {
                        let name = ws.name.clone();
                        state.stage = ManagerStage::ConfirmDelete {
                            name: name.clone(),
                            state: ConfirmState::new(format!("Delete \"{name}\"?")),
                        };
                    }
                }
            }
            Ok(InputOutcome::Continue)
        }
        KeyCode::Char('o' | 'O') => {
            handle_list_open_in_github(state, config);
            Ok(InputOutcome::Continue)
        }
        _ => Ok(InputOutcome::Continue),
    }
}

/// Dispatch the `o` key on the workspace list view. Keeps `handle_list_key`
/// below clippy's `too_many_lines` threshold and isolates the
/// toast/open/picker decision tree.
fn handle_list_open_in_github(state: &mut ManagerState<'_>, config: &AppConfig) {
    let Some(summary) = state.selected_workspace_summary() else {
        state.toast = Some(Toast {
            message: "no workspace selected".into(),
            kind: ToastKind::Error,
            shown_at: std::time::Instant::now(),
        });
        return;
    };
    let Some(ws) = config.workspaces.get(&summary.name) else {
        return;
    };
    let choices = super::super::github_mounts::resolve_for_workspace(ws);
    match choices.len() {
        0 => {
            state.toast = Some(Toast {
                message: "no GitHub URLs for this workspace".into(),
                kind: ToastKind::Error,
                shown_at: std::time::Instant::now(),
            });
        }
        1 => {
            if let Err(e) = open::that_detached(&choices[0].url) {
                state.toast = Some(Toast {
                    message: format!("failed to open URL: {e}"),
                    kind: ToastKind::Error,
                    shown_at: std::time::Instant::now(),
                });
            }
        }
        _ => {
            state.list_modal = Some(Modal::GithubPicker {
                state: crate::launch::widgets::github_picker::GithubPickerState::new(choices),
            });
        }
    }
}

/// Dispatch a key into whatever modal currently sits on `state.list_modal`.
/// Only `Modal::GithubPicker` is expected here today; any other variant that
/// sneaks in is treated as cancel so the operator isn't stuck.
pub(super) fn handle_list_modal(state: &mut ManagerState<'_>, key: KeyEvent) {
    let Some(modal) = state.list_modal.as_mut() else {
        return;
    };
    match modal {
        Modal::GithubPicker { state: picker } => match picker.handle_key(key) {
            ModalOutcome::Commit(url) => {
                state.list_modal = None;
                if let Err(e) = open::that_detached(&url) {
                    state.toast = Some(Toast {
                        message: format!("failed to open URL: {e}"),
                        kind: ToastKind::Error,
                        shown_at: std::time::Instant::now(),
                    });
                }
            }
            ModalOutcome::Cancel => {
                state.list_modal = None;
            }
            ModalOutcome::Continue => {}
        },
        // Defensive catch-all — no other Modal variants are placed on the
        // list_modal slot today.
        _ => {
            state.list_modal = None;
        }
    }
}
