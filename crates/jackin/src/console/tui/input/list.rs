//! List-stage dispatch: workspace-picker key handling and the
//! list-level modal (`GithubPicker`).

use crossterm::event::{KeyCode, KeyEvent};

use super::InputOutcome;
use crate::console::ConsoleInstanceAction;
use crate::console::tui::effect::ManagerEffect;
use crate::console::tui::message::{ManagerMessage, update_manager};
use crate::console::tui::state::{
    AgentChoiceState, EditorState, ManagerState, Modal, SettingsState,
};
use crate::paths::JackinPaths;
use jackin_config::AppConfig;
use jackin_console::tui::components::error_popup::{
    instance_unavailable_error_message, instance_unavailable_error_title, no_instance_error_title,
    no_instance_state_for_workspace_message, no_purgeable_instance_for_workspace_message,
    no_recoverable_instance_for_workspace_message, no_recoverable_instance_selected_message,
    no_running_instance_for_workspace_message, no_running_instance_to_stop_message,
};
use jackin_console::tui::components::github_picker::{GithubOpenPlan, github_open_plan};
use jackin_console::tui::components::provider_picker::ProviderPickerOutcome;
use jackin_console::tui::layout::list_body_area;
use jackin_console::tui::screens::workspaces::update::{
    PreviewPaneKeyPlan, WorkspaceInstanceScopePlan, WorkspaceInstanceStatus,
    WorkspaceListEnterPlan, WorkspaceListHorizontalPlan, WorkspaceListNewSessionPlan,
    WorkspaceListSelectedInstancePlan, instance_action_accepts_status,
    is_preview_pane_entry_target, preview_pane_key_plan, preview_pane_selected_index,
    selected_instance_plan, selected_instance_scope_plan, should_enter_preview_pane,
    workspace_list_enter_plan, workspace_list_horizontal_plan, workspace_list_new_session_plan,
    workspace_list_saved_workspace_index, workspace_list_settings_available,
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
        // Left/Right arrows: tree expand/collapse when the selected row owns
        // that direction, otherwise horizontal scroll if the focused list
        // overflows. h/l remain alternate horizontal-scroll keys.
        KeyCode::Left => {
            handle_list_left_right(state, config, cwd, -8);
            Ok(InputOutcome::Continue)
        }
        KeyCode::Right => {
            handle_list_left_right(state, config, cwd, 8);
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
            if state.list_scroll_focus().is_some() {
                dispatch_manager(state, ManagerMessage::ScrollFocusedListBlockVertical(-3));
                clamp_list_scroll_after_key(state, config, cwd);
            } else {
                dispatch_manager(state, ManagerMessage::MoveListSelection(-1));
            }
            Ok(InputOutcome::Continue)
        }
        KeyCode::Down | KeyCode::Char('j' | 'J') => {
            if state.list_scroll_focus().is_some() {
                dispatch_manager(state, ManagerMessage::ScrollFocusedListBlockVertical(3));
                clamp_list_scroll_after_key(state, config, cwd);
            } else {
                dispatch_manager(state, ManagerMessage::MoveListSelection(1));
            }
            Ok(InputOutcome::Continue)
        }
        KeyCode::Enter => match workspace_list_enter_plan(state.selected_row()) {
            WorkspaceListEnterPlan::LaunchCurrentDir => Ok(InputOutcome::LaunchCurrentDir),
            WorkspaceListEnterPlan::CreateNewWorkspace => {
                state.request_effect(ManagerEffect::OpenCreatePreludeFileBrowser);
                Ok(InputOutcome::Continue)
            }
            WorkspaceListEnterPlan::LaunchSavedWorkspace(i) => Ok(state
                .workspaces
                .get(i)
                .map_or(InputOutcome::Continue, |summary| {
                    InputOutcome::LaunchNamed(summary.name.clone())
                })),
            WorkspaceListEnterPlan::InstanceAction => Ok(instance_action_outcome(
                state,
                ConsoleInstanceAction::Reconnect,
                no_recoverable_instance_selected_message(),
            )),
        },
        KeyCode::Char('e' | 'E') => {
            if let Some(i) = workspace_list_saved_workspace_index(state.selected_row())
                && let Some(summary) = state.workspaces.get(i)
            {
                let name = summary.name.clone();
                if let Some(ws) = config.workspaces.get(&name) {
                    dispatch_manager(
                        state,
                        ManagerMessage::EnterEditor(EditorState::new_edit(name, ws.clone())),
                    );
                }
            }
            Ok(InputOutcome::Continue)
        }
        KeyCode::Char('n' | 'N') => {
            match workspace_list_new_session_plan(state.selected_row()) {
                WorkspaceListNewSessionPlan::ExistingWorkspaceInstance {
                    workspace_idx,
                    instance_idx,
                } => {
                    let instances = state.workspace_active_instances(workspace_idx);
                    let Some(entry) = instances.get(instance_idx) else {
                        dispatch_manager(
                            state,
                            ManagerMessage::OpenListErrorPopup {
                                title: instance_unavailable_error_title().into(),
                                message: instance_unavailable_error_message().into(),
                            },
                        );
                        return Ok(InputOutcome::Continue);
                    };
                    let container = entry.container_base.clone();
                    let picker = AgentChoiceState::with_choices(jackin_core::Agent::ALL.to_vec());
                    // The host config does not prove what env the already-running
                    // Capsule daemon captured. Offer provider choices only from
                    // daemon-owned flows that know `ZAI_API_KEY` exists there.
                    let providers = Vec::new();
                    state.inline_new_session_picker = Some((container, picker, providers));
                }
                WorkspaceListNewSessionPlan::CreateWorkspace => {
                    state.request_effect(ManagerEffect::OpenCreatePreludeFileBrowser);
                }
            }
            Ok(InputOutcome::Continue)
        }
        KeyCode::Char('d' | 'D') => {
            if let Some(i) = workspace_list_saved_workspace_index(state.selected_row())
                && let Some(ws) = state.workspaces.get(i)
            {
                let name = ws.name.clone();
                dispatch_manager(state, ManagerMessage::EnterConfirmDelete { name });
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
            if workspace_list_settings_available(state.selected_row()) {
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

fn handle_list_left_right(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
    horizontal_delta: i16,
) {
    let selected = state.selected_row();
    match workspace_list_horizontal_plan(
        selected,
        horizontal_delta,
        state.current_dir_expanded,
        state.has_current_dir_active_instances(),
        |idx| state.is_workspace_expanded(idx),
        |idx| state.has_active_instances(idx),
    ) {
        WorkspaceListHorizontalPlan::CollapseTree => {
            dispatch_manager(state, ManagerMessage::CollapseSelectedTree);
        }
        WorkspaceListHorizontalPlan::ExpandTree => {
            dispatch_manager(state, ManagerMessage::ExpandSelectedTree);
        }
        WorkspaceListHorizontalPlan::Scroll(delta) => {
            dispatch_manager(state, ManagerMessage::ScrollListHorizontal(delta));
        }
    }
    clamp_list_scroll_after_key(state, config, cwd);
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
            let Some(cursor) = preview_pane_selected_index(panes.len(), Some(cursor)) else {
                return InputOutcome::Continue;
            };
            let (_, session_id) = panes[cursor];
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
    match selected_instance_plan(state.selected_row()) {
        WorkspaceListSelectedInstancePlan::Direct {
            workspace_idx,
            instance_idx,
        } => {
            let entry = selected_direct_instance(state, workspace_idx, instance_idx)?;
            accepts_instance_status(action, entry.status).then(|| entry.container_base.clone())
        }
        WorkspaceListSelectedInstancePlan::Scope => {
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
        WorkspaceListSelectedInstancePlan::None => None,
    }
}

fn selected_direct_instance<'a>(
    state: &'a ManagerState<'_>,
    workspace_idx: Option<usize>,
    instance_idx: usize,
) -> Option<&'a crate::instance::InstanceIndexEntry> {
    match workspace_idx {
        Some(ws_idx) => state
            .workspace_active_instances(ws_idx)
            .get(instance_idx)
            .copied(),
        None => state
            .current_dir_active_instances()
            .get(instance_idx)
            .copied(),
    }
}

fn selected_instance_scope<'a>(
    state: &'a ManagerState<'_>,
) -> Option<(Option<&'a str>, &'a str, &'a str)> {
    match selected_instance_scope_plan(state.selected_row()) {
        WorkspaceInstanceScopePlan::CurrentDirectory => {
            let current_dir = state.current_dir.as_str();
            Some((None, current_dir, current_dir))
        }
        WorkspaceInstanceScopePlan::SavedWorkspace(i) => state.workspaces.get(i).map(|summary| {
            (
                Some(summary.name.as_str()),
                summary.name.as_str(),
                summary.workdir.as_str(),
            )
        }),
        WorkspaceInstanceScopePlan::WorkspaceInstance(ws_idx) => {
            state.workspaces.get(ws_idx).map(|ws| {
                (
                    Some(ws.name.as_str()),
                    ws.name.as_str(),
                    ws.workdir.as_str(),
                )
            })
        }
        WorkspaceInstanceScopePlan::None => None,
    }
}

fn accepts_instance_status(
    action: ConsoleInstanceAction,
    status: crate::instance::InstanceStatus,
) -> bool {
    instance_action_accepts_status(action.workspace_action_fact(), instance_status_fact(status))
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
    match github_open_plan(choices) {
        GithubOpenPlan::Continue => InputOutcome::Continue,
        GithubOpenPlan::OpenUrl(url) => {
            state.request_effect(ManagerEffect::OpenUrl(url));
            InputOutcome::Continue
        }
        GithubOpenPlan::Pick(picker_state) => {
            dispatch_manager(
                state,
                ManagerMessage::OpenListGithubPicker {
                    state: picker_state,
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
        .map(|modal| modal.rect(state.cached_term_size));
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
            if matches!(key.code, KeyCode::Enter)
                && let Some((row, payload)) = info.keyboard_copy_payload()
            {
                state.request_effect(ManagerEffect::CopyContainerInfoValue { row, payload });
                return InputOutcome::Continue;
            }
            let outcome = if let Some(rect) = container_info_rect {
                info.handle_key_in_rect(key, rect)
            } else {
                info.handle_key(key)
            };
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
/// sidebar. Commit runs `inline_provider_followup_plan`; the running-container
/// path always supplies an empty provider list (the daemon, not host config,
/// owns the captured env), so in practice this dispatches `NewSessionWithAgent`
/// directly and the provider picker never opens here. Cancel/Esc dismisses.
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
            // Running-container path passes an empty list → no provider picker.
            let plan = inline_provider_followup_plan(container, agent, providers.clone());
            dispatch_manager(state, ManagerMessage::DismissInlineSessionPicker);
            match plan {
                InlineProviderFollowupPlan::StartSession { context, agent } => {
                    InputOutcome::InstanceAction {
                        container: context,
                        action: ConsoleInstanceAction::NewSessionWithAgent(agent),
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
