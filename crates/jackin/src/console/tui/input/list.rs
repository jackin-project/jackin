//! List-stage dispatch: workspace-picker key handling and the
//! list-level modal (`GithubPicker`).

use crossterm::event::{KeyCode, KeyEvent};

use super::InputOutcome;
use crate::config::AppConfig;
use crate::console::ConsoleInstanceAction;
use crate::console::tui::effect::ManagerEffect;
use crate::console::tui::instance_action::workspace_instance_action_fact;
use crate::console::tui::message::{ManagerMessage, update_manager};
use crate::console::tui::state::{
    EditorState, ManagerListRow, ManagerState, Modal, settings_state_from_config,
};
use crate::paths::JackinPaths;
use jackin_console::tui::components::error_popup::{
    instance_unavailable_error_message, instance_unavailable_error_title, no_instance_error_title,
    no_instance_state_for_workspace_message, no_purgeable_instance_for_workspace_message,
    no_recoverable_instance_for_workspace_message, no_recoverable_instance_selected_message,
    no_running_instance_for_workspace_message, no_running_instance_to_stop_message,
};
use jackin_console::tui::components::provider_picker::ProviderPickerOutcome;
use jackin_console::tui::layout::list_body_area;
use jackin_console::tui::screens::workspaces::update::{
    PreviewPaneKeyPlan, WorkspaceInstanceStatus, instance_action_accepts_status,
    is_preview_pane_entry_target, preview_pane_key_plan, should_enter_preview_pane,
};
use jackin_console::tui::screens::workspaces::view::instance_purge_confirm_label;
use jackin_console::tui::update::{
    InlinePickerShellPlan, InlineProviderFollowupPlan, inline_picker_shell_plan,
    inline_provider_followup_plan,
};
use jackin_tui::ModalOutcome;

#[allow(
    clippy::too_many_lines,
    clippy::items_after_statements,
    clippy::unnecessary_wraps
)]
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
    if is_preview_pane_entry_target(key.code, selected_row)
        && let Some(container) =
            selected_instance_container(state, ConsoleInstanceAction::Reconnect)
        && should_enter_preview_pane(
            key.code,
            selected_row,
            state.flattened_preview_panes(&container).len(),
        )
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
                state.request_effect(ManagerEffect::OpenCreatePreludeFileBrowser);
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
                no_recoverable_instance_selected_message(),
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
                            title: instance_unavailable_error_title().into(),
                            message: instance_unavailable_error_message().into(),
                        },
                    );
                }
            } else {
                state.request_effect(ManagerEffect::OpenCreatePreludeFileBrowser);
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
        KeyCode::Char('o' | 'O') => Ok(handle_list_open_in_github(state, config)),
        KeyCode::Char('r' | 'R') => Ok(instance_action_outcome(
            state,
            ConsoleInstanceAction::Reconnect,
            no_recoverable_instance_for_workspace_message(),
        )),
        KeyCode::Char('a' | 'A') => Ok(instance_action_outcome(
            state,
            ConsoleInstanceAction::NewSession,
            no_running_instance_for_workspace_message(),
        )),
        KeyCode::Char('x' | 'X') => Ok(instance_action_outcome(
            state,
            ConsoleInstanceAction::Shell,
            no_running_instance_for_workspace_message(),
        )),
        KeyCode::Char('i' | 'I') => Ok(instance_action_outcome(
            state,
            ConsoleInstanceAction::Inspect,
            no_instance_state_for_workspace_message(),
        )),
        KeyCode::Char('p' | 'P') => Ok(confirm_purge_outcome(state)),
        KeyCode::Char('t' | 'T') => Ok(instance_action_outcome(
            state,
            ConsoleInstanceAction::Stop,
            no_running_instance_to_stop_message(),
        )),
        KeyCode::Char('s' | 'S') => {
            if !matches!(
                state.selected_row(),
                ManagerListRow::WorkspaceInstance(_, _)
                    | ManagerListRow::CurrentDirectoryInstance(_)
            ) {
                dispatch_manager(
                    state,
                    ManagerMessage::EnterSettings(settings_state_from_config(config)),
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
    let body = list_body_area(area);
    crate::console::tui::layout::list::clamp_list_scroll_for_area(body, state, config, cwd);
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
                title: no_instance_error_title().into(),
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
                title: no_instance_error_title().into(),
                message: no_purgeable_instance_for_workspace_message().into(),
            },
        );
        return InputOutcome::Continue;
    };
    let label = state
        .instances
        .iter()
        .find(|entry| entry.container_base == container)
        .map_or_else(
            || instance_purge_confirm_label(&container, None),
            |entry| instance_purge_confirm_label(&entry.container_base, Some(&entry.role_key)),
        );
    dispatch_manager(
        state,
        ManagerMessage::EnterConfirmInstancePurge { container, label },
    );
    InputOutcome::Continue
}

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
    let cursor = state
        .preview_pane_cursor
        .get(&container)
        .copied()
        .unwrap_or(0);
    match preview_pane_key_plan(key.code, panes.len()) {
        PreviewPaneKeyPlan::ExitPreview => {
            dispatch_manager(state, ManagerMessage::ExitPreview);
            InputOutcome::Continue
        }
        PreviewPaneKeyPlan::Move { delta } => {
            dispatch_manager(state, ManagerMessage::MovePreviewPane { container, delta });
            InputOutcome::Continue
        }
        PreviewPaneKeyPlan::ReconnectSelected => {
            let (_, session_id) = panes[cursor.min(panes.len() - 1)];
            dispatch_manager(state, ManagerMessage::ExitPreview);
            InputOutcome::InstanceAction {
                container,
                action: ConsoleInstanceAction::ReconnectFocus(session_id),
            }
        }
        PreviewPaneKeyPlan::Continue => InputOutcome::Continue,
    }
}

fn selected_instance_container(
    state: &ManagerState<'_>,
    action: ConsoleInstanceAction,
) -> Option<String> {
    if let ManagerListRow::WorkspaceInstance(ws_idx, inst_idx) = state.selected_row() {
        let instances = state.workspace_active_instances(ws_idx);
        let entry = instances.get(inst_idx)?;
        return if accepts_instance_status(action, entry.status) {
            Some(entry.container_base.clone())
        } else {
            None
        };
    }
    if let ManagerListRow::CurrentDirectoryInstance(inst_idx) = state.selected_row() {
        let instances = state.current_dir_active_instances();
        let entry = instances.get(inst_idx)?;
        return if accepts_instance_status(action, entry.status) {
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
        (entry.matches(query) && accepts_instance_status(action, entry.status))
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

const fn accepts_instance_status(
    action: ConsoleInstanceAction,
    status: crate::instance::InstanceStatus,
) -> bool {
    instance_action_accepts_status(
        workspace_instance_action_fact(action),
        instance_status_fact(status),
    )
}

const fn instance_status_fact(status: crate::instance::InstanceStatus) -> WorkspaceInstanceStatus {
    use crate::instance::InstanceStatus as S;
    match status {
        S::Active => WorkspaceInstanceStatus::Active,
        S::Running => WorkspaceInstanceStatus::Running,
        S::CleanExited => WorkspaceInstanceStatus::CleanExited,
        S::Crashed => WorkspaceInstanceStatus::Crashed,
        S::PreservedDirty => WorkspaceInstanceStatus::PreservedDirty,
        S::PreservedUnpushed => WorkspaceInstanceStatus::PreservedUnpushed,
        S::RestoreAvailable => WorkspaceInstanceStatus::RestoreAvailable,
        S::Superseded => WorkspaceInstanceStatus::Superseded,
        S::Purged => WorkspaceInstanceStatus::Purged,
        S::FailedSetup => WorkspaceInstanceStatus::FailedSetup,
    }
}

/// Dispatch the `o` key on the workspace list view.
fn handle_list_open_in_github(state: &mut ManagerState<'_>, config: &AppConfig) -> InputOutcome {
    // Silent no-op when there is no workspace or no GitHub URLs — the hint is
    // already suppressed in those cases so the operator never sees the key.
    let Some(summary) = state.selected_workspace_summary() else {
        return InputOutcome::Continue;
    };
    let Some(ws) = config.workspaces.get(&summary.name) else {
        return InputOutcome::Continue;
    };
    let choices = jackin_console::github_mounts::resolve_for_workspace_from_cache(
        ws,
        &state.mount_info_cache,
    );
    match choices.len() {
        0 => InputOutcome::Continue,
        1 => {
            state.request_effect(ManagerEffect::OpenUrl(choices[0].url.clone()));
            InputOutcome::Continue
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
            InputOutcome::Continue
        }
    }
}

/// Dispatch a key into whatever modal currently sits on `state.list_modal`.
pub(super) fn handle_list_modal(state: &mut ManagerState<'_>, key: KeyEvent) -> InputOutcome {
    // Pre-compute the Debug-info dialog rect (immutable borrow) so the scroll
    // can be clamped to the content after the key is handled — without this the
    // offset inflates past the end and the opposite key feels dead while it
    // unwinds.
    let container_info_rect = state
        .list_modal
        .as_ref()
        .filter(|modal| matches!(modal, Modal::ContainerInfo { .. }))
        .map(|modal| {
            crate::console::tui::components::modal_layout::modal_outer_rect(
                modal,
                state.cached_term_size,
            )
        });
    let Some(modal) = state.list_modal.as_mut() else {
        return InputOutcome::Continue;
    };
    match modal {
        Modal::GithubPicker { state: picker } => match picker.handle_key(key) {
            ModalOutcome::Commit(url) => {
                dispatch_manager(state, ManagerMessage::DismissListModal);
                state.request_effect(ManagerEffect::OpenUrl(url));
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
        Modal::ContainerInfo { state: info } => {
            let outcome = info.handle_key(key);
            if let Some(rect) = container_info_rect {
                info.clamp_scroll(rect);
            }
            match outcome {
                ModalOutcome::Commit(()) | ModalOutcome::Cancel => {
                    dispatch_manager(state, ManagerMessage::DismissListModal);
                    InputOutcome::Continue
                }
                ModalOutcome::Continue => InputOutcome::Continue,
            }
        }
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
    match inline_picker_shell_plan(key, true) {
        InlinePickerShellPlan::ScrollHorizontal(delta) => {
            dispatch_manager(state, ManagerMessage::ScrollListHorizontal(delta));
            InputOutcome::Continue
        }
        InlinePickerShellPlan::Exit => InputOutcome::ExitJackin,
        InlinePickerShellPlan::Delegate => match picker.handle_key(key) {
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
    match inline_picker_shell_plan(key, false) {
        InlinePickerShellPlan::ScrollHorizontal(delta) => {
            dispatch_manager(state, ManagerMessage::ScrollListHorizontal(delta));
            InputOutcome::Continue
        }
        InlinePickerShellPlan::Exit => InputOutcome::ExitJackin,
        InlinePickerShellPlan::Delegate => match picker.handle_key(key) {
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
            let plan = inline_provider_followup_plan(
                container,
                agent,
                providers.clone(),
                agent == crate::agent::Agent::Claude,
            );
            dispatch_manager(state, ManagerMessage::DismissInlineSessionPicker);
            match plan {
                InlineProviderFollowupPlan::StartSession { context, agent } => {
                    InputOutcome::InstanceAction {
                        container: context,
                        action: crate::console::ConsoleInstanceAction::NewSessionWithAgent(agent),
                    }
                }
                InlineProviderFollowupPlan::OpenProviderPicker(picker) => {
                    state.inline_provider_picker = Some(picker);
                    InputOutcome::Continue
                }
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
    match picker.handle_key(key.into()) {
        ProviderPickerOutcome::Commit {
            context,
            agent,
            provider,
        } => {
            dispatch_manager(state, ManagerMessage::DismissInlineProviderPicker);
            InputOutcome::NewSessionWithProvider {
                container: context,
                agent,
                provider,
            }
        }
        ProviderPickerOutcome::Cancel => {
            dispatch_manager(state, ManagerMessage::DismissInlineProviderPicker);
            InputOutcome::Continue
        }
        ProviderPickerOutcome::Continue => InputOutcome::Continue,
    }
}

pub(super) fn handle_launch_provider_picker(
    state: &mut ManagerState<'_>,
    key: KeyEvent,
) -> InputOutcome {
    let Some(picker) = state.launch_provider_picker.as_mut() else {
        return InputOutcome::Continue;
    };
    match picker.handle_key(key.into()) {
        ProviderPickerOutcome::Commit {
            context,
            agent,
            provider,
        } => {
            state.launch_provider_picker = None;
            InputOutcome::LaunchWithProvider {
                selector: context,
                agent,
                provider,
            }
        }
        ProviderPickerOutcome::Cancel => {
            dispatch_manager(state, ManagerMessage::DismissLaunchProviderPicker);
            InputOutcome::Continue
        }
        ProviderPickerOutcome::Continue => InputOutcome::Continue,
    }
}

#[cfg(test)]
mod tests;
