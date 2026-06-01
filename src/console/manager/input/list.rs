//! List-stage dispatch: workspace-picker key handling and the
//! list-level modal (`GithubPicker`).

use crossterm::event::{KeyCode, KeyEvent};

use super::super::super::widgets::ModalOutcome;
use super::super::message::{ManagerMessage, update_manager};
use super::super::state::{
    CreatePreludeState, EditorState, FileBrowserTarget, ManagerListRow, ManagerState, Modal,
    ProviderPickerState, SettingsState,
};
use super::InputOutcome;
use crate::config::AppConfig;
use crate::console::ConsoleInstanceAction;
use crate::paths::JackinPaths;
use jackin_console::widgets::file_browser::FileBrowserState;

#[allow(clippy::too_many_lines)]
pub(super) fn handle_list_key(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    _paths: &JackinPaths,
    cwd: &std::path::Path,
    key: KeyEvent,
) -> anyhow::Result<InputOutcome> {
    // Preview-pane navigation mode: the operator dropped focus into
    // the right-hand snapshot tree via Tab / →. Keys are reinterpreted
    // for preview navigation; nothing falls through to the workspace
    // tree until they Esc / ← / BackTab out.
    if state.preview_focused {
        return Ok(handle_preview_focused_key(state, key));
    }
    // Tab / → on a running-instance row drops focus INTO the preview
    // pane. Tab takes precedence over the existing right-arrow
    // tree-expand because instance rows have no expand semantics; →
    // on a non-instance row continues to the existing handler below.
    let selected_row = state.selected_row();
    if matches!(key.code, KeyCode::Tab | KeyCode::Right)
        && matches!(
            selected_row,
            ManagerListRow::WorkspaceInstance(_, _) | ManagerListRow::CurrentDirectoryInstance(_)
        )
        && let Some(container) =
            selected_instance_container(state, ConsoleInstanceAction::Reconnect)
        && !state.flattened_preview_panes(&container).is_empty()
    {
        dispatch_manager(state, ManagerMessage::EnterPreview);
        return Ok(InputOutcome::Continue);
    }
    match key.code {
        KeyCode::Esc | KeyCode::Char('q' | 'Q') => Ok(InputOutcome::ExitJackin),
        // Left/Right arrows: tree expand/collapse.
        // h/l keep horizontal scroll so the details pane stays scrollable.
        KeyCode::Left => {
            dispatch_manager(state, ManagerMessage::CollapseSelectedTree);
            Ok(InputOutcome::Continue)
        }
        KeyCode::Right => {
            dispatch_manager(state, ManagerMessage::ExpandSelectedTree);
            Ok(InputOutcome::Continue)
        }
        KeyCode::Char('h' | 'H') => {
            dispatch_manager(state, ManagerMessage::ScrollListHorizontal(-8));
            clamp_list_scroll_after_key(state, config, cwd);
            Ok(InputOutcome::Continue)
        }
        KeyCode::Char('l' | 'L') => {
            dispatch_manager(state, ManagerMessage::ScrollListHorizontal(8));
            clamp_list_scroll_after_key(state, config, cwd);
            Ok(InputOutcome::Continue)
        }
        KeyCode::Up | KeyCode::Char('k' | 'K') => {
            if state.list_scroll_focus.is_some() {
                dispatch_manager(state, ManagerMessage::ScrollFocusedListBlockVertical(-3));
                clamp_list_scroll_after_key(state, config, cwd);
            } else {
                dispatch_manager(state, ManagerMessage::MoveListSelection(-1));
            }
            Ok(InputOutcome::Continue)
        }
        KeyCode::Down | KeyCode::Char('j' | 'J') => {
            if state.list_scroll_focus.is_some() {
                dispatch_manager(state, ManagerMessage::ScrollFocusedListBlockVertical(3));
                clamp_list_scroll_after_key(state, config, cwd);
            } else {
                dispatch_manager(state, ManagerMessage::MoveListSelection(1));
            }
            Ok(InputOutcome::Continue)
        }
        KeyCode::Enter => match state.selected_row() {
            ManagerListRow::CurrentDirectory => Ok(InputOutcome::LaunchCurrentDir),
            ManagerListRow::NewWorkspace => {
                let mut prelude = CreatePreludeState::new();
                prelude.modal = Some(Modal::FileBrowser {
                    target: FileBrowserTarget::CreateFirstMountSrc,
                    state: FileBrowserState::new_from_home()?,
                });
                dispatch_manager(state, ManagerMessage::EnterCreatePrelude(prelude));
                Ok(InputOutcome::Continue)
            }
            ManagerListRow::SavedWorkspace(i) => Ok(state
                .workspaces
                .get(i)
                .map_or(InputOutcome::Continue, |summary| {
                    InputOutcome::LaunchNamed(summary.name.clone())
                })),
            ManagerListRow::WorkspaceInstance(_, _)
            | ManagerListRow::CurrentDirectoryInstance(_) => Ok(instance_action_outcome(
                state,
                ConsoleInstanceAction::Reconnect,
                "No recoverable instance selected.",
            )),
        },
        KeyCode::Char('e' | 'E') => {
            match state.selected_row() {
                ManagerListRow::CurrentDirectory
                | ManagerListRow::CurrentDirectoryInstance(_)
                | ManagerListRow::NewWorkspace
                | ManagerListRow::WorkspaceInstance(_, _) => {}
                ManagerListRow::SavedWorkspace(i) => {
                    if let Some(summary) = state.workspaces.get(i) {
                        let name = summary.name.clone();
                        if let Some(ws) = config.workspaces.get(&name) {
                            dispatch_manager(
                                state,
                                ManagerMessage::EnterEditor(EditorState::new_edit(
                                    name,
                                    ws.clone(),
                                )),
                            );
                        }
                    }
                }
            }
            Ok(InputOutcome::Continue)
        }
        KeyCode::Char('n' | 'N') => {
            if let ManagerListRow::WorkspaceInstance(ws_idx, inst_idx) = state.selected_row() {
                let instances = state.workspace_active_instances(ws_idx);
                if let Some(entry) = instances.get(inst_idx) {
                    let container = entry.container_base.clone();
                    let picker = crate::agent::AgentChoiceState::with_choices(
                        crate::agent::Agent::ALL.to_vec(),
                    );
                    // The host config does not prove what env the already-running
                    // Capsule daemon captured. Offer provider choices only from
                    // daemon-owned flows that know `ZAI_API_KEY` exists there.
                    let providers = Vec::new();
                    state.inline_new_session_picker = Some((container, picker, providers));
                } else {
                    dispatch_manager(
                        state,
                        ManagerMessage::OpenListErrorPopup {
                            title: "Instance unavailable".into(),
                            message: "Instance no longer active; list refreshes automatically."
                                .into(),
                        },
                    );
                }
            } else {
                let mut prelude = CreatePreludeState::new();
                prelude.modal = Some(Modal::FileBrowser {
                    target: FileBrowserTarget::CreateFirstMountSrc,
                    state: FileBrowserState::new_from_home()?,
                });
                dispatch_manager(state, ManagerMessage::EnterCreatePrelude(prelude));
            }
            Ok(InputOutcome::Continue)
        }
        KeyCode::Char('d' | 'D') => {
            match state.selected_row() {
                ManagerListRow::CurrentDirectory
                | ManagerListRow::CurrentDirectoryInstance(_)
                | ManagerListRow::NewWorkspace
                | ManagerListRow::WorkspaceInstance(_, _) => {}
                ManagerListRow::SavedWorkspace(i) => {
                    if let Some(ws) = state.workspaces.get(i) {
                        let name = ws.name.clone();
                        dispatch_manager(state, ManagerMessage::EnterConfirmDelete { name });
                    }
                }
            }
            Ok(InputOutcome::Continue)
        }
        KeyCode::Char('o' | 'O') => {
            handle_list_open_in_github(state, config);
            Ok(InputOutcome::Continue)
        }
        KeyCode::Char('r' | 'R') => Ok(instance_action_outcome(
            state,
            ConsoleInstanceAction::Reconnect,
            "No recoverable instance for this workspace.",
        )),
        KeyCode::Char('a' | 'A') => Ok(instance_action_outcome(
            state,
            ConsoleInstanceAction::NewSession,
            "No running instance for this workspace.",
        )),
        KeyCode::Char('x' | 'X') => Ok(instance_action_outcome(
            state,
            ConsoleInstanceAction::Shell,
            "No running instance for this workspace.",
        )),
        KeyCode::Char('i' | 'I') => Ok(instance_action_outcome(
            state,
            ConsoleInstanceAction::Inspect,
            "No instance state for this workspace.",
        )),
        KeyCode::Char('p' | 'P') => Ok(confirm_purge_outcome(state)),
        KeyCode::Char('t' | 'T') => Ok(instance_action_outcome(
            state,
            ConsoleInstanceAction::Stop,
            "No running instance to stop.",
        )),
        KeyCode::Char('s' | 'S') => {
            if !matches!(
                state.selected_row(),
                ManagerListRow::WorkspaceInstance(_, _)
                    | ManagerListRow::CurrentDirectoryInstance(_)
            ) {
                dispatch_manager(
                    state,
                    ManagerMessage::EnterSettings(SettingsState::from_config(config)),
                );
            }
            Ok(InputOutcome::Continue)
        }
        _ => Ok(InputOutcome::Continue),
    }
}

fn clamp_list_scroll_after_key(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
) {
    let area = state.cached_term_size;
    let body = ratatui::layout::Rect {
        x: 0,
        y: 2,
        width: area.width,
        height: area.height.saturating_sub(4),
    };
    super::super::list_geometry::clamp_list_scroll_for_area(body, state, config, cwd);
}

fn dispatch_manager(state: &mut ManagerState<'_>, message: ManagerMessage) {
    let _dirty = update_manager(state, message);
}

fn instance_action_outcome(
    state: &mut ManagerState<'_>,
    action: ConsoleInstanceAction,
    empty_message: &str,
) -> InputOutcome {
    let Some(container) = selected_instance_container(state, action) else {
        dispatch_manager(
            state,
            ManagerMessage::OpenListErrorPopup {
                title: "No instance".into(),
                message: empty_message.into(),
            },
        );
        return InputOutcome::Continue;
    };
    InputOutcome::InstanceAction { container, action }
}

/// Resolve the container for Purge, then stage a Y/N confirmation
/// modal. Purge now also calls `eject_role` before deleting preserved
/// state (so a mis-keyed `P` on a running instance destroys role +
/// `DinD` + volume + network plus on-disk state in one stroke), so the
/// confirmation step is non-optional. Mirrors the workspace-delete
/// pattern at `handle_list_key` line 158.
fn confirm_purge_outcome(state: &mut ManagerState<'_>) -> InputOutcome {
    let Some(container) = selected_instance_container(state, ConsoleInstanceAction::Purge) else {
        dispatch_manager(
            state,
            ManagerMessage::OpenListErrorPopup {
                title: "No instance".into(),
                message: "No purgeable instance for this workspace.".into(),
            },
        );
        return InputOutcome::Continue;
    };
    let label = state
        .instances
        .iter()
        .find(|entry| entry.container_base == container)
        .map_or_else(
            || container.clone(),
            |entry| format!("{} ({})", entry.container_base, entry.role_key),
        );
    dispatch_manager(
        state,
        ManagerMessage::EnterConfirmInstancePurge { container, label },
    );
    InputOutcome::Continue
}

/// Preview-pane navigation: the operator pressed a key while
/// `state.preview_focused` is `true`.
///
/// ↑/↓ cycles the selected pane inside the snapshot, Esc / ← /
/// `BackTab` pops focus back to the tree, Enter reattaches with
/// `--focus <session_id>`. Any other key is a no-op so a stray Tab
/// or text key cannot dump the operator back to the workspace tree
/// mid-pick.
fn handle_preview_focused_key(state: &mut ManagerState<'_>, key: KeyEvent) -> InputOutcome {
    let Some(container) = selected_instance_container(state, ConsoleInstanceAction::Reconnect)
    else {
        // Selection slid off an instance row while preview was open
        // (refresh purged the entry). Clear focus and let the next
        // input fall through to the workspace tree.
        dispatch_manager(state, ManagerMessage::ExitPreview);
        return InputOutcome::Continue;
    };
    let panes = state.flattened_preview_panes(&container);
    if panes.is_empty() {
        dispatch_manager(state, ManagerMessage::ExitPreview);
        return InputOutcome::Continue;
    }
    let cursor = state
        .preview_pane_cursor
        .get(&container)
        .copied()
        .unwrap_or(0)
        .min(panes.len() - 1);
    match key.code {
        KeyCode::Esc | KeyCode::BackTab | KeyCode::Left => {
            dispatch_manager(state, ManagerMessage::ExitPreview);
            InputOutcome::Continue
        }
        KeyCode::Up | KeyCode::Char('k' | 'K') => {
            dispatch_manager(
                state,
                ManagerMessage::MovePreviewPane {
                    container,
                    delta: -1,
                },
            );
            InputOutcome::Continue
        }
        KeyCode::Down | KeyCode::Char('j' | 'J') => {
            dispatch_manager(
                state,
                ManagerMessage::MovePreviewPane {
                    container,
                    delta: 1,
                },
            );
            InputOutcome::Continue
        }
        KeyCode::Enter => {
            let (_, session_id) = panes[cursor];
            dispatch_manager(state, ManagerMessage::ExitPreview);
            InputOutcome::InstanceAction {
                container,
                action: ConsoleInstanceAction::ReconnectFocus(session_id),
            }
        }
        _ => InputOutcome::Continue,
    }
}

fn selected_instance_container(
    state: &ManagerState<'_>,
    action: ConsoleInstanceAction,
) -> Option<String> {
    if let ManagerListRow::WorkspaceInstance(ws_idx, inst_idx) = state.selected_row() {
        let instances = state.workspace_active_instances(ws_idx);
        let entry = instances.get(inst_idx)?;
        return if instance_action_accepts_status(action, entry.status) {
            Some(entry.container_base.clone())
        } else {
            None
        };
    }
    if let ManagerListRow::CurrentDirectoryInstance(inst_idx) = state.selected_row() {
        let instances = state.current_dir_active_instances();
        let entry = instances.get(inst_idx)?;
        return if instance_action_accepts_status(action, entry.status) {
            Some(entry.container_base.clone())
        } else {
            None
        };
    }
    let (workspace_name, workspace_label, workdir) = selected_instance_scope(state)?;
    let query = crate::instance::InstanceQuery {
        workspace_name,
        workspace_label,
        workdir,
        role_key: None,
        agent_runtime: None,
    };
    state.instances.iter().find_map(|entry| {
        (entry.matches(query) && instance_action_accepts_status(action, entry.status))
            .then(|| entry.container_base.clone())
    })
}

fn selected_instance_scope<'a>(
    state: &'a ManagerState<'_>,
) -> Option<(Option<&'a str>, &'a str, &'a str)> {
    match state.selected_row() {
        ManagerListRow::CurrentDirectory | ManagerListRow::CurrentDirectoryInstance(_) => {
            let current_dir = state.current_dir.as_str();
            Some((None, current_dir, current_dir))
        }
        ManagerListRow::SavedWorkspace(i) => state.workspaces.get(i).map(|summary| {
            (
                Some(summary.name.as_str()),
                summary.name.as_str(),
                summary.workdir.as_str(),
            )
        }),
        ManagerListRow::WorkspaceInstance(ws_idx, _) => state.workspaces.get(ws_idx).map(|ws| {
            (
                Some(ws.name.as_str()),
                ws.name.as_str(),
                ws.workdir.as_str(),
            )
        }),
        ManagerListRow::NewWorkspace => None,
    }
}

/// Action × status acceptance grid. Each arm enumerates the exact set
/// of statuses the action runs against. Negative `!matches!` idioms
/// were intentionally avoided: a future `InstanceStatus` variant
/// (e.g. `Stopping`) would silently flip every action that used a
/// negative match to "accept this new state too", which is almost
/// never what the operator wants. The positive form forces the
/// developer adding a variant to consider each action explicitly.
const fn instance_action_accepts_status(
    action: ConsoleInstanceAction,
    status: crate::instance::InstanceStatus,
) -> bool {
    use crate::instance::InstanceStatus as S;
    match (action, status) {
        // Reconnect / ReconnectFocus / Inspect: anything that still has on-disk state to read.
        (
            ConsoleInstanceAction::Reconnect
            | ConsoleInstanceAction::ReconnectFocus(_)
            | ConsoleInstanceAction::Inspect,
            status,
        ) => match status {
            S::Active
            | S::Running
            | S::CleanExited
            | S::Crashed
            | S::PreservedDirty
            | S::PreservedUnpushed
            | S::RestoreAvailable
            | S::Superseded
            | S::FailedSetup => true,
            S::Purged => false,
        },
        // NewSession / Shell / Stop: live container required.
        (
            ConsoleInstanceAction::NewSession
            | ConsoleInstanceAction::NewSessionWithAgent(_)
            | ConsoleInstanceAction::Shell
            | ConsoleInstanceAction::Stop,
            status,
        ) => match status {
            S::Active | S::Running => true,
            S::CleanExited
            | S::Crashed
            | S::PreservedDirty
            | S::PreservedUnpushed
            | S::RestoreAvailable
            | S::Superseded
            | S::Purged
            | S::FailedSetup => false,
        },
        // Purge: anything that hasn't already been purged. Crashed /
        // CleanExited / Preserved* rows have local state worth deleting
        // even though their containers are gone — Purge cleans both
        // halves of the leftover.
        (ConsoleInstanceAction::Purge, status) => match status {
            S::Active
            | S::Running
            | S::CleanExited
            | S::Crashed
            | S::PreservedDirty
            | S::PreservedUnpushed
            | S::RestoreAvailable
            | S::Superseded
            | S::FailedSetup => true,
            S::Purged => false,
        },
    }
}

/// Dispatch the `o` key on the workspace list view.
fn handle_list_open_in_github(state: &mut ManagerState<'_>, config: &AppConfig) {
    // Silent no-op when there is no workspace or no GitHub URLs — the hint is
    // already suppressed in those cases so the operator never sees the key.
    let Some(summary) = state.selected_workspace_summary() else {
        return;
    };
    let Some(ws) = config.workspaces.get(&summary.name) else {
        return;
    };
    let choices = jackin_console::github_mounts::resolve_for_workspace(ws);
    match choices.len() {
        0 => {}
        1 => {
            if let Err(e) = open::that_detached(&choices[0].url) {
                dispatch_manager(
                    state,
                    ManagerMessage::OpenListErrorPopup {
                        title: "Failed to open URL".into(),
                        message: format!("{e}"),
                    },
                );
            }
        }
        _ => {
            dispatch_manager(
                state,
                ManagerMessage::OpenListGithubPicker {
                    state: jackin_console::tui::components::github_picker::GithubPickerState::new(
                        choices,
                    ),
                },
            );
        }
    }
}

/// Dispatch a key into whatever modal currently sits on `state.list_modal`.
pub(super) fn handle_list_modal(state: &mut ManagerState<'_>, key: KeyEvent) -> InputOutcome {
    let Some(modal) = state.list_modal.as_mut() else {
        return InputOutcome::Continue;
    };
    match modal {
        Modal::GithubPicker { state: picker } => match picker.handle_key(key) {
            ModalOutcome::Commit(url) => {
                dispatch_manager(state, ManagerMessage::DismissListModal);
                if let Err(e) = open::that_detached(&url) {
                    dispatch_manager(
                        state,
                        ManagerMessage::OpenListErrorPopup {
                            title: "Failed to open URL".into(),
                            message: format!("{e}"),
                        },
                    );
                }
                InputOutcome::Continue
            }
            ModalOutcome::Cancel => {
                dispatch_manager(state, ManagerMessage::DismissListModal);
                InputOutcome::Continue
            }
            ModalOutcome::Continue => InputOutcome::Continue,
        },
        Modal::RolePicker { state: picker } => match picker.handle_key(key) {
            ModalOutcome::Commit(role) => {
                dispatch_manager(state, ManagerMessage::DismissListModal);
                InputOutcome::LaunchWithAgent(role)
            }
            ModalOutcome::Cancel => {
                dispatch_manager(state, ManagerMessage::DismissListModal);
                InputOutcome::Continue
            }
            ModalOutcome::Continue => InputOutcome::Continue,
        },
        Modal::ErrorPopup { state: popup } => match popup.handle_key(key) {
            ModalOutcome::Commit(()) | ModalOutcome::Cancel => {
                dispatch_manager(state, ManagerMessage::DismissListModal);
                InputOutcome::Continue
            }
            ModalOutcome::Continue => InputOutcome::Continue,
        },
        Modal::ContainerInfo { state: info } => match info.handle_key(key) {
            ModalOutcome::Commit(()) | ModalOutcome::Cancel => {
                dispatch_manager(state, ManagerMessage::DismissListModal);
                InputOutcome::Continue
            }
            ModalOutcome::Continue => InputOutcome::Continue,
        },
        _ => {
            dispatch_manager(state, ManagerMessage::DismissListModal);
            InputOutcome::Continue
        }
    }
}

pub(super) fn handle_inline_role_picker(
    state: &mut ManagerState<'_>,
    key: KeyEvent,
) -> InputOutcome {
    let Some(picker) = state.inline_role_picker.as_mut() else {
        return InputOutcome::Continue;
    };
    match key.code {
        KeyCode::Left | KeyCode::Char('h' | 'H') => {
            dispatch_manager(state, ManagerMessage::ScrollListHorizontal(-8));
            InputOutcome::Continue
        }
        KeyCode::Right | KeyCode::Char('l' | 'L') => {
            dispatch_manager(state, ManagerMessage::ScrollListHorizontal(8));
            InputOutcome::Continue
        }
        KeyCode::Char('q' | 'Q') => InputOutcome::ExitJackin,
        _ => match picker.handle_key(key) {
            ModalOutcome::Commit(role) => {
                dispatch_manager(state, ManagerMessage::DismissInlineRolePicker);
                InputOutcome::LaunchWithAgent(role)
            }
            ModalOutcome::Cancel => {
                dispatch_manager(state, ManagerMessage::DismissInlineRolePicker);
                InputOutcome::Continue
            }
            ModalOutcome::Continue => InputOutcome::Continue,
        },
    }
}

pub(super) fn handle_inline_agent_picker(
    state: &mut ManagerState<'_>,
    key: KeyEvent,
) -> InputOutcome {
    let Some((_, picker)) = state.inline_agent_picker.as_mut() else {
        return InputOutcome::Continue;
    };
    match key.code {
        KeyCode::Left | KeyCode::Char('h' | 'H') => {
            dispatch_manager(state, ManagerMessage::ScrollListHorizontal(-8));
            InputOutcome::Continue
        }
        KeyCode::Right | KeyCode::Char('l' | 'L') => {
            dispatch_manager(state, ManagerMessage::ScrollListHorizontal(8));
            InputOutcome::Continue
        }
        _ => match picker.handle_key(key) {
            ModalOutcome::Commit(agent) => {
                dispatch_manager(state, ManagerMessage::DismissInlineAgentPicker);
                InputOutcome::LaunchWithRuntimeAgent(agent)
            }
            ModalOutcome::Cancel => {
                dispatch_manager(state, ManagerMessage::DismissInlineAgentPicker);
                InputOutcome::Continue
            }
            ModalOutcome::Continue => InputOutcome::Continue,
        },
    }
}

/// Handle key events while the new-session agent picker is open in the left
/// sidebar. Commit → dispatch `NewSessionWithAgent`; Cancel/Esc → dismiss.
pub(super) fn handle_new_session_picker(
    state: &mut ManagerState<'_>,
    key: KeyEvent,
) -> InputOutcome {
    let Some((container, picker, providers)) = state.inline_new_session_picker.as_mut() else {
        return InputOutcome::Continue;
    };
    match picker.handle_key(key) {
        ModalOutcome::Commit(agent) => {
            let container = container.clone();
            if providers.is_empty() || agent != crate::agent::Agent::Claude {
                dispatch_manager(state, ManagerMessage::DismissInlineSessionPicker);
                InputOutcome::InstanceAction {
                    container,
                    action: crate::console::ConsoleInstanceAction::NewSessionWithAgent(agent),
                }
            } else {
                let providers = providers.clone();
                dispatch_manager(state, ManagerMessage::DismissInlineSessionPicker);
                state.inline_provider_picker =
                    Some(ProviderPickerState::new(container, agent, providers));
                InputOutcome::Continue
            }
        }
        ModalOutcome::Cancel => {
            dispatch_manager(state, ManagerMessage::DismissInlineSessionPicker);
            InputOutcome::Continue
        }
        ModalOutcome::Continue => InputOutcome::Continue,
    }
}

/// Handle key events while the inline provider picker is open (shown after
/// agent selection when multiple providers are available). Enter commits;
/// Esc cancels; Up/Down navigate.
pub(super) fn handle_inline_provider_picker(
    state: &mut ManagerState<'_>,
    key: KeyEvent,
) -> InputOutcome {
    let Some(picker) = state.inline_provider_picker.as_mut() else {
        return InputOutcome::Continue;
    };
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            picker.move_up();
            InputOutcome::Continue
        }
        KeyCode::Down | KeyCode::Char('j') => {
            picker.move_down();
            InputOutcome::Continue
        }
        KeyCode::Enter => {
            let Some(provider) = picker.selected_provider() else {
                return InputOutcome::Continue;
            };
            let container = picker.context.clone();
            let agent = picker.agent;
            dispatch_manager(state, ManagerMessage::DismissInlineProviderPicker);
            InputOutcome::NewSessionWithProvider {
                container,
                agent,
                provider,
            }
        }
        KeyCode::Esc => {
            dispatch_manager(state, ManagerMessage::DismissInlineProviderPicker);
            InputOutcome::Continue
        }
        _ => InputOutcome::Continue,
    }
}

pub(super) fn handle_launch_provider_picker(
    state: &mut ManagerState<'_>,
    key: KeyEvent,
) -> InputOutcome {
    let Some(picker) = state.launch_provider_picker.as_mut() else {
        return InputOutcome::Continue;
    };
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            picker.move_up();
            InputOutcome::Continue
        }
        KeyCode::Down | KeyCode::Char('j') => {
            picker.move_down();
            InputOutcome::Continue
        }
        KeyCode::Enter => {
            let Some(provider) = picker.selected_provider() else {
                return InputOutcome::Continue;
            };
            let picker = state.launch_provider_picker.take().expect("checked above");
            InputOutcome::LaunchWithProvider {
                selector: picker.context,
                agent: picker.agent,
                provider,
            }
        }
        KeyCode::Esc => {
            dispatch_manager(state, ManagerMessage::DismissLaunchProviderPicker);
            InputOutcome::Continue
        }
        _ => InputOutcome::Continue,
    }
}

#[cfg(test)]
mod tests {
    //! List-stage tests: row-0 (current dir) gating, Enter routing,
    //! `o`-key resolver to GitHub URLs, and the `GithubPicker` modal.
    use super::super::super::state::{ManagerStage, ManagerState, Modal, MountScrollFocus};
    use super::super::test_support::{key, mount};
    use super::{InputOutcome, handle_new_session_picker, instance_action_accepts_status};
    use crate::agent::AgentChoiceState;
    use crate::config::AppConfig;
    use crate::console::manager::input::handle_key;
    use crate::instance::{InstanceIndexEntry, InstanceStatus};
    use crate::paths::JackinPaths;
    use crate::workspace::WorkspaceConfig;
    use crossterm::event::KeyCode;
    use tempfile::TempDir;

    /// Build a git repo under `root` with a `github.com` origin remote on
    /// `branch`. Returns the path so callers can use it as a mount src.
    fn make_github_repo(root: &std::path::Path, name: &str, branch: &str) -> std::path::PathBuf {
        let path = root.join(name);
        let git_dir = path.join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        std::fs::write(git_dir.join("HEAD"), format!("ref: refs/heads/{branch}\n")).unwrap();
        std::fs::write(
            git_dir.join("config"),
            format!("[remote \"origin\"]\n    url = git@github.com:owner/{name}.git\n"),
        )
        .unwrap();
        path
    }

    /// Helper: seed an `AppConfig` + `ManagerState` with `ws` as a saved workspace,
    /// cwd far away so selection lands on row 1 (the saved workspace).
    fn list_state_selecting_ws(
        ws: WorkspaceConfig,
    ) -> (ManagerState<'static>, AppConfig, JackinPaths, TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let mut config = AppConfig::default();
        config.workspaces.insert("demo".into(), ws);
        let mut state = ManagerState::from_config(&config, tmp.path());
        state.selected = 1; // force selection onto the saved workspace row
        (state, config, paths, tmp)
    }

    fn instance_entry(
        container: &str,
        status: InstanceStatus,
        workdir: &str,
    ) -> InstanceIndexEntry {
        InstanceIndexEntry {
            instance_id: format!("{container}-id"),
            container_base: container.into(),
            workspace_name: Some("demo".into()),
            workspace_label: "demo".into(),
            workdir: workdir.into(),
            role_key: "the-architect".into(),
            agent_runtime: "codex".into(),
            status,
            updated_at: "2026-05-11T00:00:00Z".into(),
        }
    }

    fn current_dir_instance_entry(
        container: &str,
        status: InstanceStatus,
        workdir: &str,
    ) -> InstanceIndexEntry {
        InstanceIndexEntry {
            instance_id: format!("{container}-id"),
            container_base: container.into(),
            workspace_name: None,
            workspace_label: workdir.into(),
            workdir: workdir.into(),
            role_key: "the-architect".into(),
            agent_runtime: "codex".into(),
            status,
            updated_at: "2026-05-11T00:00:00Z".into(),
        }
    }

    fn provider_choices() -> Vec<jackin_protocol::Provider> {
        vec![
            jackin_protocol::Provider::Anthropic,
            jackin_protocol::Provider::Zai,
        ]
    }

    #[test]
    fn new_session_provider_picker_only_opens_for_claude() {
        let config = AppConfig::default();
        let tmp = tempfile::tempdir().unwrap();
        let mut state = ManagerState::from_config(&config, tmp.path());
        let mut picker = AgentChoiceState::with_choices(vec![crate::agent::Agent::Codex]);
        picker.focused = crate::agent::Agent::Codex;
        state.inline_new_session_picker =
            Some(("jackin-demo-architect".into(), picker, provider_choices()));

        let outcome = handle_new_session_picker(&mut state, key(KeyCode::Enter));

        match outcome {
            InputOutcome::InstanceAction { container, action } => {
                assert_eq!(container, "jackin-demo-architect");
                assert_eq!(
                    action,
                    crate::console::ConsoleInstanceAction::NewSessionWithAgent(
                        crate::agent::Agent::Codex,
                    )
                );
            }
            other => panic!("expected direct new-session dispatch; got {other:?}"),
        }
        assert!(
            state.inline_provider_picker.is_none(),
            "non-Claude agents must not open the provider picker"
        );
    }

    #[test]
    fn new_session_provider_picker_opens_for_claude() {
        let config = AppConfig::default();
        let tmp = tempfile::tempdir().unwrap();
        let mut state = ManagerState::from_config(&config, tmp.path());
        let mut picker = AgentChoiceState::with_choices(vec![crate::agent::Agent::Claude]);
        picker.focused = crate::agent::Agent::Claude;
        state.inline_new_session_picker =
            Some(("jackin-demo-architect".into(), picker, provider_choices()));

        let outcome = handle_new_session_picker(&mut state, key(KeyCode::Enter));

        assert!(matches!(outcome, InputOutcome::Continue));
        let Some(picker) = state.inline_provider_picker else {
            panic!("Claude with providers must open provider picker");
        };
        assert_eq!(picker.context, "jackin-demo-architect");
        assert_eq!(picker.agent, crate::agent::Agent::Claude);
        assert_eq!(picker.providers().len(), 2);
        assert_eq!(picker.selected(), 0);
    }

    #[test]
    fn new_session_picker_does_not_offer_host_config_providers_for_running_container() {
        let workdir = "/workspace/demo";
        let ws = WorkspaceConfig {
            workdir: workdir.into(),
            mounts: vec![],
            ..Default::default()
        };
        let (mut state, mut config, paths, tmp) = list_state_selecting_ws(ws);
        config.env.insert(
            "ZAI_API_KEY".into(),
            crate::operator_env::EnvValue::Plain("host-key-added-after-launch".into()),
        );
        state.instances = vec![instance_entry(
            "jackin-demo-architect-running",
            InstanceStatus::Running,
            workdir,
        )];
        state.expand_workspace(0);
        state.selected = state
            .index_of_row(crate::console::manager::state::ManagerListRow::WorkspaceInstance(0, 0))
            .expect("expanded workspace instance row exists");

        let outcome = handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Char('n')),
        )
        .unwrap();

        assert!(matches!(outcome, InputOutcome::Continue));
        let Some((_container, _picker, providers)) = state.inline_new_session_picker.as_ref()
        else {
            panic!("N on a running instance must open the agent picker");
        };
        assert!(
            providers.is_empty(),
            "host config must not offer providers for an already-running container"
        );
    }

    fn live_snapshot() -> crate::runtime::snapshot::InstanceSnapshot {
        crate::runtime::snapshot::InstanceSnapshot {
            tabs: vec![jackin_protocol::control::TabSnapshot {
                label: "Codex".into(),
                focused_pane: 1,
                panes: vec![jackin_protocol::control::PaneSnapshot {
                    session_id: 1,
                    label: "Codex".into(),
                    agent: Some("codex".into()),
                    state: jackin_protocol::control::AgentState::Idle,
                }],
            }],
            active_tab: 0,
        }
    }

    #[test]
    fn right_on_current_directory_parent_expands_even_with_live_snapshot() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let cwd = tmp.path();
        let workdir = cwd.display().to_string();
        let container = "jackin-current-dir-the-architect-live";

        let mut config = AppConfig::default();
        let mut state = ManagerState::from_config(&config, cwd);
        state.instances = vec![current_dir_instance_entry(
            container,
            InstanceStatus::Running,
            &workdir,
        )];
        state
            .instance_snapshots
            .insert(container.into(), live_snapshot());

        let outcome =
            handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Right)).unwrap();

        assert!(matches!(outcome, InputOutcome::Continue));
        assert!(
            state.current_dir_expanded,
            "→ on the Current directory parent must expand the tree"
        );
        assert!(
            !state.preview_focused,
            "preview focus is only reachable from instance child rows"
        );
        assert!(matches!(
            state.row_at(1),
            Some(crate::console::manager::state::ManagerListRow::CurrentDirectoryInstance(0))
        ));
    }

    /// `e` and `d` on the current-directory row must be silent no-ops —
    /// no modal, no stage transition.
    #[test]
    fn current_directory_row_silently_ignores_edit_and_delete() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let cwd = tmp.path();

        let mut config = AppConfig::default();
        config.workspaces.insert(
            "some-ws".into(),
            WorkspaceConfig {
                workdir: "/unrelated".into(),
                mounts: vec![],
                ..Default::default()
            },
        );
        let mut state = ManagerState::from_config(&config, cwd);
        assert_eq!(state.selected, 0);

        handle_key(
            &mut state,
            &mut config,
            &paths,
            cwd,
            key(KeyCode::Char('e')),
        )
        .unwrap();
        assert!(
            matches!(&state.stage, ManagerStage::List),
            "e on row 0 must not open the Editor; got {:?}",
            state.stage
        );

        handle_key(
            &mut state,
            &mut config,
            &paths,
            cwd,
            key(KeyCode::Char('d')),
        )
        .unwrap();
        assert!(
            matches!(&state.stage, ManagerStage::List),
            "d on row 0 must not open ConfirmDelete; got {:?}",
            state.stage
        );
    }

    /// Enter on row 0 returns `LaunchCurrentDir`; Enter on row 1 returns
    /// `LaunchNamed(<name>)`. Pins the index arithmetic that maps list-row
    /// indices to launch targets.
    #[test]
    fn enter_on_current_directory_returns_launch_current_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let cwd = tmp.path();

        let mut config = AppConfig::default();
        config.workspaces.insert(
            "alpha".into(),
            WorkspaceConfig {
                workdir: "/alpha".into(),
                mounts: vec![],
                ..Default::default()
            },
        );
        let mut state = ManagerState::from_config(&config, cwd);
        state.selected = 0;
        let outcome =
            handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();
        assert!(
            matches!(outcome, InputOutcome::LaunchCurrentDir),
            "row 0 Enter must produce LaunchCurrentDir"
        );

        state.selected = 1;
        let outcome =
            handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();
        match outcome {
            InputOutcome::LaunchNamed(name) => assert_eq!(name, "alpha"),
            other => panic!("row 1 Enter must produce LaunchNamed(\"alpha\"); got {other:?}"),
        }
    }

    #[test]
    fn s_opens_settings_stage() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let cwd = tmp.path();
        let mut config = AppConfig::default();
        let mut state = ManagerState::from_config(&config, cwd);

        let outcome = handle_key(
            &mut state,
            &mut config,
            &paths,
            cwd,
            key(KeyCode::Char('s')),
        )
        .unwrap();

        assert!(matches!(outcome, InputOutcome::Continue));
        assert!(
            matches!(&state.stage, ManagerStage::Settings(settings) if settings.mounts.pending.is_empty())
        );
    }

    #[test]
    fn instance_shortcuts_return_selected_workspace_actions() {
        let workdir = "/workspace/demo";
        let ws = WorkspaceConfig {
            workdir: workdir.into(),
            mounts: vec![],
            ..Default::default()
        };
        let (mut state, mut config, paths, tmp) = list_state_selecting_ws(ws);
        state.instances = vec![instance_entry(
            "jackin-demo-architect-123456",
            InstanceStatus::RestoreAvailable,
            workdir,
        )];

        let outcome = handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Char('r')),
        )
        .unwrap();
        match outcome {
            InputOutcome::InstanceAction { container, action } => {
                assert_eq!(container, "jackin-demo-architect-123456");
                assert_eq!(action, crate::console::ConsoleInstanceAction::Reconnect);
            }
            other => panic!("expected reconnect instance action; got {other:?}"),
        }

        let outcome = handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Char('i')),
        )
        .unwrap();
        match outcome {
            InputOutcome::InstanceAction { container, action } => {
                assert_eq!(container, "jackin-demo-architect-123456");
                assert_eq!(action, crate::console::ConsoleInstanceAction::Inspect);
            }
            other => panic!("expected inspect instance action; got {other:?}"),
        }

        // P now stages a confirm modal instead of dispatching Purge
        // directly — the action destroys role + DinD + volume + network
        // + local state in one stroke, so an unconditional confirmation
        // step keeps mis-keyed `P` from destroying running work.
        let outcome = handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Char('p')),
        )
        .unwrap();
        assert!(
            matches!(outcome, InputOutcome::Continue),
            "P should stage the confirm modal and return Continue; got {outcome:?}"
        );
        assert!(
            matches!(
                state.stage,
                crate::console::manager::state::ManagerStage::ConfirmInstancePurge { .. }
            ),
            "P should have set ConfirmInstancePurge stage"
        );

        // Confirm via Y → the staged action fires.
        let outcome = handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Char('y')),
        )
        .unwrap();
        match outcome {
            InputOutcome::InstanceAction { container, action } => {
                assert_eq!(container, "jackin-demo-architect-123456");
                assert_eq!(action, crate::console::ConsoleInstanceAction::Purge);
            }
            other => panic!("expected purge instance action after Y; got {other:?}"),
        }
    }

    #[test]
    fn confirm_instance_purge_n_dismisses_without_dispatch() {
        let workdir = "/workspace/demo";
        let ws = WorkspaceConfig {
            workdir: workdir.into(),
            mounts: vec![],
            ..Default::default()
        };
        let (mut state, mut config, paths, tmp) = list_state_selecting_ws(ws);
        state.instances = vec![instance_entry(
            "jackin-demo-architect-cancel",
            InstanceStatus::Running,
            workdir,
        )];
        handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Char('p')),
        )
        .unwrap();
        assert!(matches!(
            state.stage,
            crate::console::manager::state::ManagerStage::ConfirmInstancePurge { .. }
        ));
        let outcome = handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Char('n')),
        )
        .unwrap();
        assert!(
            matches!(outcome, InputOutcome::Continue),
            "N must return Continue (no dispatch); got {outcome:?}"
        );
        assert!(
            matches!(
                state.stage,
                crate::console::manager::state::ManagerStage::List
            ),
            "N must reset stage to List"
        );
    }

    #[test]
    fn confirm_instance_purge_esc_dismisses_without_dispatch() {
        let workdir = "/workspace/demo";
        let ws = WorkspaceConfig {
            workdir: workdir.into(),
            mounts: vec![],
            ..Default::default()
        };
        let (mut state, mut config, paths, tmp) = list_state_selecting_ws(ws);
        state.instances = vec![instance_entry(
            "jackin-demo-architect-esc",
            InstanceStatus::Running,
            workdir,
        )];
        handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Char('p')),
        )
        .unwrap();
        let outcome = handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Esc),
        )
        .unwrap();
        assert!(matches!(outcome, InputOutcome::Continue));
        assert!(matches!(
            state.stage,
            crate::console::manager::state::ManagerStage::List
        ));
    }

    #[test]
    fn t_key_dispatches_stop_for_running_instance() {
        let workdir = "/workspace/demo";
        let ws = WorkspaceConfig {
            workdir: workdir.into(),
            mounts: vec![],
            ..Default::default()
        };
        let (mut state, mut config, paths, tmp) = list_state_selecting_ws(ws);
        state.instances = vec![instance_entry(
            "jackin-demo-architect-stop",
            InstanceStatus::Running,
            workdir,
        )];
        let outcome = handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Char('t')),
        )
        .unwrap();
        match outcome {
            InputOutcome::InstanceAction { container, action } => {
                assert_eq!(container, "jackin-demo-architect-stop");
                assert_eq!(action, crate::console::ConsoleInstanceAction::Stop);
            }
            other => panic!("expected stop instance action; got {other:?}"),
        }
    }

    #[test]
    fn t_key_shows_no_instance_popup_when_no_running_instance() {
        let workdir = "/workspace/demo";
        let ws = WorkspaceConfig {
            workdir: workdir.into(),
            mounts: vec![],
            ..Default::default()
        };
        let (mut state, mut config, paths, tmp) = list_state_selecting_ws(ws);
        // Only a CleanExited entry — Stop must not accept it.
        state.instances = vec![instance_entry(
            "jackin-demo-architect-stale",
            InstanceStatus::CleanExited,
            workdir,
        )];
        let outcome = handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Char('t')),
        )
        .unwrap();
        assert!(
            matches!(outcome, InputOutcome::Continue),
            "T on non-Running must yield Continue (with the no-instance modal); got {outcome:?}"
        );
        assert!(
            matches!(state.list_modal, Some(Modal::ErrorPopup { .. })),
            "expected ErrorPopup modal explaining no running instance"
        );
    }

    #[test]
    fn instance_action_accepts_status_grid_smoke() {
        use crate::console::ConsoleInstanceAction as A;
        use crate::instance::InstanceStatus as S;
        // Smoke test the grid: a couple of cells per action so a
        // future refactor that flips the action × status matrix has to
        // touch this test.
        assert!(instance_action_accepts_status(A::Stop, S::Running));
        assert!(!instance_action_accepts_status(A::Stop, S::CleanExited));
        assert!(!instance_action_accepts_status(A::Stop, S::Purged));
        assert!(instance_action_accepts_status(A::Purge, S::Running));
        assert!(instance_action_accepts_status(A::Purge, S::PreservedDirty));
        assert!(!instance_action_accepts_status(A::Purge, S::Purged));
        assert!(instance_action_accepts_status(A::Reconnect, S::Crashed));
        assert!(!instance_action_accepts_status(A::Reconnect, S::Purged));
    }

    #[test]
    fn a_key_starts_new_session_in_running_instance() {
        let workdir = "/workspace/demo";
        let ws = WorkspaceConfig {
            workdir: workdir.into(),
            mounts: vec![],
            ..Default::default()
        };
        let (mut state, mut config, paths, tmp) = list_state_selecting_ws(ws);
        state.instances = vec![instance_entry(
            "jackin-demo-architect-123456",
            InstanceStatus::Active,
            workdir,
        )];

        let outcome = handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Char('a')),
        )
        .unwrap();
        match outcome {
            InputOutcome::InstanceAction { container, action } => {
                assert_eq!(container, "jackin-demo-architect-123456");
                assert_eq!(action, crate::console::ConsoleInstanceAction::NewSession);
            }
            other => panic!("expected NewSession action; got {other:?}"),
        }
    }

    #[test]
    fn x_key_opens_shell_in_running_instance() {
        let workdir = "/workspace/demo";
        let ws = WorkspaceConfig {
            workdir: workdir.into(),
            mounts: vec![],
            ..Default::default()
        };
        let (mut state, mut config, paths, tmp) = list_state_selecting_ws(ws);
        state.instances = vec![instance_entry(
            "jackin-demo-architect-123456",
            InstanceStatus::Active,
            workdir,
        )];

        let outcome = handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Char('x')),
        )
        .unwrap();
        match outcome {
            InputOutcome::InstanceAction { container, action } => {
                assert_eq!(container, "jackin-demo-architect-123456");
                assert_eq!(action, crate::console::ConsoleInstanceAction::Shell);
            }
            other => panic!("expected Shell action; got {other:?}"),
        }
    }

    #[test]
    fn a_and_x_return_continue_for_non_running_instance() {
        let workdir = "/workspace/demo";
        let ws = WorkspaceConfig {
            workdir: workdir.into(),
            mounts: vec![],
            ..Default::default()
        };
        let (mut state, mut config, paths, tmp) = list_state_selecting_ws(ws);
        // RestoreAvailable instance — not active/running, so a/x must return Continue.
        state.instances = vec![instance_entry(
            "jackin-demo-architect-123456",
            InstanceStatus::RestoreAvailable,
            workdir,
        )];

        for key_char in ['a', 'x'] {
            state.list_modal = None;
            let outcome = handle_key(
                &mut state,
                &mut config,
                &paths,
                tmp.path(),
                key(KeyCode::Char(key_char)),
            )
            .unwrap();
            assert!(
                matches!(outcome, InputOutcome::Continue),
                "'{key_char}' on non-running instance must return Continue; got {outcome:?}",
            );
            assert!(
                matches!(state.list_modal, Some(Modal::ErrorPopup { .. })),
                "'{key_char}' on non-running instance must open an ErrorPopup; got {:?}",
                state.list_modal,
            );
        }
    }

    #[test]
    fn moving_selection_resets_mount_scroll_state() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let cwd = tmp.path();

        let mut config = AppConfig::default();
        config.workspaces.insert(
            "alpha".into(),
            WorkspaceConfig {
                workdir: "/alpha".into(),
                mounts: vec![],
                ..Default::default()
            },
        );
        // When no block is focused, Down navigates the workspace list and resets scroll.
        let mut state = ManagerState::from_config(&config, cwd);
        state.selected = 0;
        state.list_mounts_scroll_x = 24;
        state.list_global_mounts_scroll_x = 16;
        state.list_role_global_mounts_scroll_x = 8;
        state.list_scroll_focus = None;

        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Down)).unwrap();

        assert_eq!(state.selected, 1);
        assert_eq!(state.list_mounts_scroll_x, 0);
        assert_eq!(state.list_global_mounts_scroll_x, 0);
        assert_eq!(state.list_role_global_mounts_scroll_x, 0);
        assert_eq!(state.list_scroll_focus, None);
    }

    #[test]
    fn down_key_with_focused_block_clamps_vertical_scroll_without_selection_move() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let cwd = tmp.path();

        let mut config = AppConfig::default();
        config.workspaces.insert(
            "alpha".into(),
            WorkspaceConfig {
                workdir: "/alpha".into(),
                mounts: vec![],
                ..Default::default()
            },
        );
        // When a block is focused, Down scrolls that block vertically, not the list.
        let mut state = ManagerState::from_config(&config, cwd);
        state.selected = 0;
        state.list_scroll_focus = Some(MountScrollFocus::Workspace);

        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Down)).unwrap();

        assert_eq!(
            state.selected, 0,
            "selection must not change while block focused"
        );
        assert_eq!(
            state.list_mounts_scroll_y, 0,
            "non-overflowing block stays clamped"
        );
    }

    // ── List-view `o` key → GitHub resolver + picker ──────────────────

    #[test]
    fn resolve_github_mounts_returns_one_per_github_repo() {
        // A workspace with two github mounts + one folder + one gitlab repo
        // should yield exactly two picker choices.
        let tmp = tempfile::tempdir().unwrap();
        let repo_a = make_github_repo(tmp.path(), "repo-a", "main");
        let repo_b = make_github_repo(tmp.path(), "repo-b", "dev");
        let plain = tmp.path().join("plain");
        std::fs::create_dir(&plain).unwrap();
        // Gitlab repo should be skipped.
        let gitlab = tmp.path().join("gl");
        let gl_git = gitlab.join(".git");
        std::fs::create_dir_all(&gl_git).unwrap();
        std::fs::write(gl_git.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::write(
            gl_git.join("config"),
            "[remote \"origin\"]\n    url = git@gitlab.com:owner/repo.git\n",
        )
        .unwrap();

        let ws = WorkspaceConfig {
            mounts: vec![
                mount(repo_a.to_str().unwrap(), "/a"),
                mount(plain.to_str().unwrap(), "/p"),
                mount(repo_b.to_str().unwrap(), "/b"),
                mount(gitlab.to_str().unwrap(), "/g"),
            ],
            ..WorkspaceConfig::default()
        };

        let choices = jackin_console::github_mounts::resolve_for_workspace(&ws);
        assert_eq!(choices.len(), 2);
        // URLs track the HEAD ref per-repo.
        let urls: Vec<&str> = choices.iter().map(|c| c.url.as_str()).collect();
        assert!(urls.contains(&"https://github.com/owner/repo-a/tree/main"));
        assert!(urls.contains(&"https://github.com/owner/repo-b/tree/dev"));
        // Branch label matches Named variant.
        let branches: Vec<&str> = choices.iter().map(|c| c.branch.as_str()).collect();
        assert!(branches.contains(&"main"));
        assert!(branches.contains(&"dev"));
    }

    #[test]
    fn list_o_with_single_github_mount_has_one_resolved_url() {
        // Resolver-side check — we can't cleanly assert `open::that_detached`
        // ran, but we can pin that there's exactly one URL to hand to it so
        // the 1-mount branch's immediate-open path is taken.
        let tmp = tempfile::tempdir().unwrap();
        let repo = make_github_repo(tmp.path(), "solo", "trunk");
        let ws = WorkspaceConfig {
            mounts: vec![mount(repo.to_str().unwrap(), "/solo")],
            ..WorkspaceConfig::default()
        };
        let choices = jackin_console::github_mounts::resolve_for_workspace(&ws);
        assert_eq!(choices.len(), 1);
        assert_eq!(choices[0].url, "https://github.com/owner/solo/tree/trunk");
    }

    #[test]
    fn list_o_with_multiple_github_mounts_opens_picker() {
        let tmp = tempfile::tempdir().unwrap();
        let repo_a = make_github_repo(tmp.path(), "repo-a", "main");
        let repo_b = make_github_repo(tmp.path(), "repo-b", "main");
        let ws = WorkspaceConfig {
            mounts: vec![
                mount(repo_a.to_str().unwrap(), "/a"),
                mount(repo_b.to_str().unwrap(), "/b"),
            ],
            ..WorkspaceConfig::default()
        };
        let (mut state, mut config, paths, tmp) = list_state_selecting_ws(ws);

        handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Char('o')),
        )
        .unwrap();

        match &state.list_modal {
            Some(Modal::GithubPicker { state: picker }) => {
                assert_eq!(picker.choices.len(), 2);
            }
            other => panic!("expected GithubPicker modal; got {other:?}"),
        }
    }

    #[test]
    fn list_o_with_zero_github_mounts_is_silent_noop() {
        let tmp_src = tempfile::tempdir().unwrap();
        let plain = tmp_src.path().join("plain");
        std::fs::create_dir(&plain).unwrap();
        let ws = WorkspaceConfig {
            mounts: vec![mount(plain.to_str().unwrap(), "/p")],
            ..WorkspaceConfig::default()
        };
        let (mut state, mut config, paths, tmp) = list_state_selecting_ws(ws);

        handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Char('o')),
        )
        .unwrap();

        assert!(state.list_modal.is_none(), "no modal when no GitHub URLs");
    }

    #[test]
    fn list_o_on_row_zero_is_silent_noop() {
        // Row 0 is "Current directory" — O must be a silent no-op.
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let mut config = AppConfig::default();
        config
            .workspaces
            .insert("demo".into(), WorkspaceConfig::default());
        let mut state = ManagerState::from_config(&config, tmp.path());
        state.selected = 0;

        handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Char('o')),
        )
        .unwrap();

        assert!(
            state.list_modal.is_none(),
            "O on row 0 must not open a modal"
        );
    }

    #[test]
    fn picker_commit_closes_list_modal_and_clears_state() {
        // Seed the state directly with an open GithubPicker, then commit.
        // We can't assert `open::that_detached` ran, but we *can* pin that
        // the modal closes (no lingering state) and no ErrorPopup appears
        // when the underlying call path doesn't error out synchronously.
        use jackin_console::{
            github_mounts::GithubChoice, tui::components::github_picker::GithubPickerState,
        };
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let mut config = AppConfig::default();
        let mut state = ManagerState::from_config(&config, tmp.path());
        // Use an unreachable file:// URL so `open::that_detached` is a
        // cheap no-op on most platforms (still spawns the browser handler
        // but doesn't block on network).
        state.list_modal = Some(Modal::GithubPicker {
            state: GithubPickerState::new(vec![GithubChoice {
                src: "/tmp/a".into(),
                branch: "main".into(),
                url: "file:///dev/null".into(),
            }]),
        });

        handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Enter),
        )
        .unwrap();

        // Modal is either closed (open succeeded) or shows ErrorPopup (open failed).
        // Either way, GithubPicker is gone.
        assert!(
            !matches!(state.list_modal, Some(Modal::GithubPicker { .. })),
            "GithubPicker must be gone after Enter"
        );
    }

    #[test]
    fn picker_esc_closes_without_opening_url() {
        use jackin_console::{
            github_mounts::GithubChoice, tui::components::github_picker::GithubPickerState,
        };
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let mut config = AppConfig::default();
        let mut state = ManagerState::from_config(&config, tmp.path());
        state.list_modal = Some(Modal::GithubPicker {
            state: GithubPickerState::new(vec![GithubChoice {
                src: "/tmp/a".into(),
                branch: "main".into(),
                url: "https://github.com/owner/repo/tree/main".into(),
            }]),
        });

        handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Esc),
        )
        .unwrap();

        assert!(state.list_modal.is_none());
    }
}
