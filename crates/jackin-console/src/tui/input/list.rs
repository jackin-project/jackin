// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! List-stage dispatch: workspace-picker key handling and the
//! list-level modal (`GithubPicker`).

use crossterm::event::{KeyCode, KeyEvent};

use super::InputOutcome;
use crate::tui::components::error_popup::{
    instance_unavailable_error_message, instance_unavailable_error_title, no_instance_error_title,
    no_purgeable_instance_for_workspace_message, no_recoverable_instance_selected_message,
};
use crate::tui::components::github_picker::GithubOpenPlan;
use crate::tui::components::provider_picker::ProviderPickerOutcome;
use crate::tui::layout::list_body_area;
use crate::tui::message::ConsoleInstanceAction;
use crate::tui::screens::workspaces::update::{
    PreviewPaneActionPlan, SelectedInstanceActionPlan, SelectedInstancePurgeConfirmPlan,
    WorkspaceInstanceAction, WorkspaceInstanceLookupEntry, WorkspaceInstanceLookupScope,
    WorkspaceInstanceScopePlan, WorkspaceInstanceStatus, WorkspaceListDeletePlan,
    WorkspaceListEditPlan, WorkspaceListEnterPlan, WorkspaceListHorizontalPlan,
    WorkspaceListKeyPlan, WorkspaceListNewSessionOpenPlan, WorkspaceListSettingsPlan,
    WorkspaceListTopLevelKeyPlan, preview_pane_action_plan, selected_instance_action_plan,
    selected_instance_container_for_action, selected_instance_purge_confirm_plan,
    workspace_instance_empty_message, workspace_list_delete_plan, workspace_list_edit_plan,
    workspace_list_enter_plan, workspace_list_github_open_plan, workspace_list_horizontal_plan,
    workspace_list_new_session_open_plan, workspace_list_new_session_plan,
    workspace_list_prewarm_plan, workspace_list_settings_plan, workspace_list_top_level_key_plan,
};
use crate::tui::screens::workspaces::view::instance_purge_confirm_label;
use crate::tui::state::update::{ManagerMessage, update_manager};
use crate::tui::state::{
    AgentChoiceState, EditorState, ManagerEffect, ManagerState, Modal, SettingsState,
};
use crate::tui::update::{
    DismissibleModalPlan, InlinePickerDismissal, InlinePickerPlan, InlinePickerShellPlan,
    InlineProviderFollowupPlan, ListGithubPickerPlan, ListModalKeyTarget, ListRolePickerPlan,
    apply_inline_new_session_picker_plan, apply_inline_picker_dismissal_plan,
    apply_inline_provider_picker_plan, dismissible_modal_plan, inline_picker_dismissal_plan,
    inline_picker_plan, inline_picker_shell_plan, inline_provider_followup_plan,
    list_github_picker_plan, list_role_picker_plan,
};
use jackin_config::AppConfig;
use jackin_core::JackinPaths;

type ConcreteInstanceAction = ConsoleInstanceAction<jackin_core::Agent>;

pub fn handle_list_key(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    _paths: &JackinPaths,
    cwd: &std::path::Path,
    key: KeyEvent,
) -> anyhow::Result<InputOutcome> {
    let selected_row = state.selected_row();
    let selected_preview_pane_count =
        selected_instance_container(state, ConcreteInstanceAction::Reconnect)
            .map(|container| state.flattened_preview_panes(&container).len());
    let plan = workspace_list_top_level_key_plan(
        key.code,
        state.preview_focused,
        selected_row,
        selected_preview_pane_count,
        state.list_scroll_focus().is_some(),
    );
    let plan = match plan {
        WorkspaceListTopLevelKeyPlan::PreviewFocused => {
            return Ok(handle_preview_focused_key(state, key));
        }
        WorkspaceListTopLevelKeyPlan::EnterPreview => {
            dispatch_manager(state, ManagerMessage::EnterPreview);
            return Ok(InputOutcome::Continue);
        }
        WorkspaceListTopLevelKeyPlan::ListKey(plan) => plan,
    };
    match plan {
        WorkspaceListKeyPlan::Exit => Ok(InputOutcome::ExitJackin),
        // Left/Right arrows: tree expand/collapse when the selected row owns
        // that direction, otherwise horizontal scroll if the focused list
        // overflows. h/l remain alternate horizontal-scroll keys.
        WorkspaceListKeyPlan::HorizontalTreeOrScroll { delta } => {
            handle_list_left_right(state, config, cwd, delta);
            Ok(InputOutcome::Continue)
        }
        WorkspaceListKeyPlan::ScrollHorizontal { delta } => {
            dispatch_manager(state, ManagerMessage::ScrollListHorizontal(delta));
            clamp_list_scroll_after_key(state, config, cwd);
            Ok(InputOutcome::Continue)
        }
        WorkspaceListKeyPlan::MoveSelection { delta } => {
            dispatch_manager(state, ManagerMessage::MoveListSelection(delta));
            Ok(InputOutcome::Continue)
        }
        WorkspaceListKeyPlan::ScrollFocusedVertical { delta } => {
            dispatch_manager(state, ManagerMessage::ScrollFocusedListBlockVertical(delta));
            clamp_list_scroll_after_key(state, config, cwd);
            Ok(InputOutcome::Continue)
        }
        WorkspaceListKeyPlan::Enter => match workspace_list_enter_plan(state.selected_row()) {
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
                ConcreteInstanceAction::Reconnect,
                no_recoverable_instance_selected_message(),
            )),
        },
        WorkspaceListKeyPlan::Edit => {
            dispatch_workspace_list_edit(
                state,
                config,
                workspace_list_edit_plan(state.selected_row()),
            );
            Ok(InputOutcome::Continue)
        }
        WorkspaceListKeyPlan::NewSession => {
            match workspace_list_new_session_open_plan(
                workspace_list_new_session_plan(state.selected_row()),
                |workspace_idx, instance_idx| {
                    // Tree rows index the visible list (live + failed). A new
                    // session can only attach to a live container, so resolve
                    // by visible index but yield a container only when running.
                    state
                        .workspace_visible_instances(workspace_idx)
                        .get(instance_idx)
                        .filter(|entry| {
                            matches!(
                                entry.status,
                                jackin_core::InstanceStatus::Active
                                    | jackin_core::InstanceStatus::Running
                            )
                        })
                        .map(|entry| entry.container_base.clone())
                },
            ) {
                WorkspaceListNewSessionOpenPlan::OpenPicker { container } => {
                    let picker = AgentChoiceState::with_choices(jackin_core::Agent::ALL.to_vec());
                    // The host config does not prove what env the already-running
                    // Capsule daemon captured. Offer provider choices only from
                    // daemon-owned flows that know `ZAI_API_KEY` exists there.
                    let providers = Vec::new();
                    apply_inline_new_session_picker_plan(state, container, picker, providers);
                }
                WorkspaceListNewSessionOpenPlan::OpenCreateWorkspace => {
                    state.request_effect(ManagerEffect::OpenCreatePreludeFileBrowser);
                }
                WorkspaceListNewSessionOpenPlan::OpenInstanceUnavailableError => {
                    dispatch_manager(
                        state,
                        ManagerMessage::OpenListErrorPopup {
                            title: instance_unavailable_error_title().into(),
                            message: instance_unavailable_error_message().into(),
                        },
                    );
                }
            }
            Ok(InputOutcome::Continue)
        }
        WorkspaceListKeyPlan::Delete => {
            dispatch_workspace_list_delete(state, workspace_list_delete_plan(state.selected_row()));
            Ok(InputOutcome::Continue)
        }
        WorkspaceListKeyPlan::Prewarm => Ok(workspace_list_prewarm_plan(state.selected_row())
            .and_then(|i| state.workspaces.get(i))
            .map_or(InputOutcome::Continue, |summary| {
                InputOutcome::PrewarmNamed(summary.name.clone())
            })),
        WorkspaceListKeyPlan::OpenGithub => Ok(handle_list_open_in_github(state, config)),
        WorkspaceListKeyPlan::InstanceAction(action) => {
            let (action, message) = console_instance_action_and_empty_message(action);
            Ok(instance_action_outcome(state, action, message))
        }
        WorkspaceListKeyPlan::ConfirmPurge => Ok(confirm_purge_outcome(state)),
        WorkspaceListKeyPlan::Settings => {
            dispatch_workspace_list_settings(
                state,
                config,
                workspace_list_settings_plan(state.selected_row()),
            );
            Ok(InputOutcome::Continue)
        }
        WorkspaceListKeyPlan::Continue => Ok(InputOutcome::Continue),
    }
}

fn dispatch_workspace_list_edit(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    plan: WorkspaceListEditPlan,
) {
    let WorkspaceListEditPlan::OpenEditor { workspace_idx } = plan else {
        return;
    };
    let Some(summary) = state.workspaces.get(workspace_idx) else {
        return;
    };
    let name = summary.name.clone();
    if let Some(ws) = config.workspaces.get(&name) {
        dispatch_manager(
            state,
            ManagerMessage::EnterEditor(EditorState::new_edit(name, ws.clone())),
        );
    }
}

fn dispatch_workspace_list_delete(state: &mut ManagerState<'_>, plan: WorkspaceListDeletePlan) {
    let WorkspaceListDeletePlan::ConfirmDelete { workspace_idx } = plan else {
        return;
    };
    if let Some(ws) = state.workspaces.get(workspace_idx) {
        let name = ws.name.clone();
        dispatch_manager(state, ManagerMessage::EnterConfirmDelete { name });
    }
}

fn dispatch_workspace_list_settings(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    plan: WorkspaceListSettingsPlan,
) {
    if matches!(plan, WorkspaceListSettingsPlan::OpenSettings) {
        dispatch_manager(
            state,
            ManagerMessage::EnterSettings(SettingsState::from_config(config)),
        );
    }
}

fn console_instance_action_and_empty_message(
    action: WorkspaceInstanceAction,
) -> (ConcreteInstanceAction, &'static str) {
    let action_message = workspace_instance_empty_message(action);
    let action = match action {
        WorkspaceInstanceAction::Reconnect => ConcreteInstanceAction::Reconnect,
        WorkspaceInstanceAction::NewSession => ConcreteInstanceAction::NewSession,
        WorkspaceInstanceAction::Shell => ConcreteInstanceAction::Shell,
        WorkspaceInstanceAction::Inspect => ConcreteInstanceAction::Inspect,
        WorkspaceInstanceAction::Stop => ConcreteInstanceAction::Stop,
        WorkspaceInstanceAction::Purge => ConcreteInstanceAction::Purge,
    };
    (action, action_message)
}

fn clamp_list_scroll_after_key(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
) {
    let area = state.cached_term_size;
    let body = list_body_area(area);
    crate::tui::layout::list::clamp_list_scroll_for_area(body, state, config, cwd);
}

fn dispatch_manager(state: &mut ManagerState<'_>, message: ManagerMessage) {
    let _dirty = update_manager(state, message);
}

fn instance_action_outcome(
    state: &mut ManagerState<'_>,
    action: ConcreteInstanceAction,
    empty_message: &str,
) -> InputOutcome {
    match selected_instance_action_plan(selected_instance_container(state, action)) {
        SelectedInstanceActionPlan::Start { container } => {
            InputOutcome::InstanceAction { container, action }
        }
        SelectedInstanceActionPlan::OpenError => {
            dispatch_manager(
                state,
                ManagerMessage::OpenListErrorPopup {
                    title: no_instance_error_title().into(),
                    message: empty_message.into(),
                },
            );
            InputOutcome::Continue
        }
    }
}

/// Resolve the container for Purge, then stage a Y/N confirmation
/// modal. Purge now also calls `eject_role` before deleting preserved
/// state (so a mis-keyed `P` on a running instance destroys role +
/// `DinD` + volume + network plus on-disk state in one stroke), so the
/// confirmation step is non-optional. Mirrors the workspace-delete
/// pattern at `handle_list_key` line 158.
fn confirm_purge_outcome(state: &mut ManagerState<'_>) -> InputOutcome {
    match selected_instance_purge_confirm_plan(
        selected_instance_container(state, ConcreteInstanceAction::Purge),
        |container| {
            state
                .instances
                .iter()
                .find(|entry| entry.container_base == container)
                .map_or_else(
                    || instance_purge_confirm_label(container, None),
                    |entry| {
                        instance_purge_confirm_label(&entry.container_base, Some(&entry.role_key))
                    },
                )
        },
    ) {
        SelectedInstancePurgeConfirmPlan::OpenConfirm { container, label } => {
            dispatch_manager(
                state,
                ManagerMessage::EnterConfirmInstancePurge { container, label },
            );
            InputOutcome::Continue
        }
        SelectedInstancePurgeConfirmPlan::OpenError => {
            dispatch_manager(
                state,
                ManagerMessage::OpenListErrorPopup {
                    title: no_instance_error_title().into(),
                    message: no_purgeable_instance_for_workspace_message().into(),
                },
            );
            InputOutcome::Continue
        }
    }
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
        state.has_current_dir_visible_instances(),
        |idx| state.is_workspace_expanded(idx),
        |idx| state.has_visible_instances(idx),
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
    let Some(container) = selected_instance_container(state, ConcreteInstanceAction::Reconnect)
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
    match preview_pane_action_plan(key.code, Some(cursor), panes.iter().map(|(_, id)| *id)) {
        PreviewPaneActionPlan::ExitPreview => {
            dispatch_manager(state, ManagerMessage::ExitPreview);
            InputOutcome::Continue
        }
        PreviewPaneActionPlan::Move { delta } => {
            dispatch_manager(state, ManagerMessage::MovePreviewPane { container, delta });
            InputOutcome::Continue
        }
        PreviewPaneActionPlan::ReconnectSelected { session_id } => {
            dispatch_manager(state, ManagerMessage::ExitPreview);
            InputOutcome::InstanceAction {
                container,
                action: ConcreteInstanceAction::ReconnectFocus(session_id),
            }
        }
        PreviewPaneActionPlan::Continue => InputOutcome::Continue,
    }
}

fn selected_instance_container(
    state: &ManagerState<'_>,
    action: ConcreteInstanceAction,
) -> Option<String> {
    selected_instance_container_for_action(
        state.selected_row(),
        action.workspace_action_fact(),
        |workspace_idx, instance_idx| {
            selected_direct_instance(state, workspace_idx, instance_idx).map(instance_lookup_entry)
        },
        |scope| selected_instance_scope(state, scope),
        state.instances.iter().map(instance_lookup_entry),
    )
    .map(ToOwned::to_owned)
}

fn selected_direct_instance<'a>(
    state: &'a ManagerState<'_>,
    workspace_idx: Option<usize>,
    instance_idx: usize,
) -> Option<&'a jackin_core::InstanceIndexEntry> {
    match workspace_idx {
        Some(ws_idx) => state
            .workspace_visible_instances(ws_idx)
            .get(instance_idx)
            .copied(),
        None => state
            .current_dir_visible_instances()
            .get(instance_idx)
            .copied(),
    }
}

fn selected_instance_scope<'a>(
    state: &'a ManagerState<'_>,
    scope: WorkspaceInstanceScopePlan,
) -> Option<WorkspaceInstanceLookupScope<'a>> {
    match scope {
        WorkspaceInstanceScopePlan::CurrentDirectory => {
            let current_dir = state.current_dir.as_str();
            Some(WorkspaceInstanceLookupScope {
                workspace_name: None,
                workspace_label: current_dir,
                workdir: current_dir,
            })
        }
        WorkspaceInstanceScopePlan::SavedWorkspace(i) => {
            state
                .workspaces
                .get(i)
                .map(|summary| WorkspaceInstanceLookupScope {
                    workspace_name: Some(summary.name.as_str()),
                    workspace_label: summary.name.as_str(),
                    workdir: summary.workdir.as_str(),
                })
        }
        WorkspaceInstanceScopePlan::WorkspaceInstance(ws_idx) => {
            state
                .workspaces
                .get(ws_idx)
                .map(|ws| WorkspaceInstanceLookupScope {
                    workspace_name: Some(ws.name.as_str()),
                    workspace_label: ws.name.as_str(),
                    workdir: ws.workdir.as_str(),
                })
        }
        WorkspaceInstanceScopePlan::None => None,
    }
}

fn instance_lookup_entry(
    entry: &jackin_core::InstanceIndexEntry,
) -> WorkspaceInstanceLookupEntry<'_> {
    WorkspaceInstanceLookupEntry {
        container: entry.container_base.as_str(),
        workspace_name: entry.workspace_name.as_deref(),
        workspace_label: entry.workspace_label.as_str(),
        workdir: entry.workdir.as_str(),
        status: instance_status_fact(entry.status),
    }
}

const fn instance_status_fact(status: jackin_core::InstanceStatus) -> WorkspaceInstanceStatus {
    use jackin_core::InstanceStatus as S;
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
    let selected_workspace_name = state
        .selected_workspace_summary()
        .map(|summary| summary.name.as_str());
    match workspace_list_github_open_plan(selected_workspace_name, config, &state.mount_info_cache)
    {
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
pub fn handle_list_modal(state: &mut ManagerState<'_>, key: KeyEvent) -> InputOutcome {
    // Pre-compute the Debug-info dialog rect (immutable borrow) so the scroll
    // can be clamped to the content after the key is handled — without this the
    // offset inflates past the end and the opposite key feels dead while it
    // unwinds.
    let container_info_rect = state
        .list_modal
        .as_ref()
        .and_then(|modal| modal.container_info_rect(state.cached_term_size));
    let Some(modal) = state.list_modal.as_mut() else {
        return InputOutcome::Continue;
    };
    let target = modal.list_key_target();
    match (target, modal) {
        (ListModalKeyTarget::GithubPicker, Modal::GithubPicker { state: picker }) => {
            match list_github_picker_plan(picker.handle_key(key)) {
                ListGithubPickerPlan::OpenUrl(url) => {
                    dispatch_manager(state, ManagerMessage::DismissListModal);
                    state.request_effect(ManagerEffect::OpenUrl(url));
                    InputOutcome::Continue
                }
                ListGithubPickerPlan::Dismiss => {
                    dispatch_manager(state, ManagerMessage::DismissListModal);
                    InputOutcome::Continue
                }
                ListGithubPickerPlan::Continue => InputOutcome::Continue,
            }
        }
        (ListModalKeyTarget::RolePicker, Modal::RolePicker { state: picker }) => {
            match list_role_picker_plan(picker.handle_key(key)) {
                ListRolePickerPlan::Launch(role) => {
                    dispatch_manager(state, ManagerMessage::DismissListModal);
                    InputOutcome::LaunchWithAgent(role)
                }
                ListRolePickerPlan::Dismiss => {
                    dispatch_manager(state, ManagerMessage::DismissListModal);
                    InputOutcome::Continue
                }
                ListRolePickerPlan::Continue => InputOutcome::Continue,
            }
        }
        (ListModalKeyTarget::ErrorPopup, Modal::ErrorPopup { state: popup }) => {
            match dismissible_modal_plan(popup.handle_key(key)) {
                DismissibleModalPlan::Dismiss => {
                    dispatch_manager(state, ManagerMessage::DismissListModal);
                    InputOutcome::Continue
                }
                DismissibleModalPlan::Continue => InputOutcome::Continue,
            }
        }
        (ListModalKeyTarget::ContainerInfo, Modal::ContainerInfo { state: info }) => {
            if matches!(key.code, KeyCode::Enter)
                && let Some((row, payload)) = info.keyboard_copy_payload()
            {
                state.request_effect(ManagerEffect::CopyContainerInfoValue { row, payload });
                return InputOutcome::Continue;
            }
            let outcome = if let Some(rect) = container_info_rect {
                info.set_viewport(rect);
                info.handle_key(key)
            } else {
                info.handle_key(key)
            };
            if let Some(rect) = container_info_rect {
                info.clamp_scroll(rect);
            }
            match dismissible_modal_plan(outcome) {
                DismissibleModalPlan::Dismiss => {
                    dispatch_manager(state, ManagerMessage::DismissListModal);
                    InputOutcome::Continue
                }
                DismissibleModalPlan::Continue => InputOutcome::Continue,
            }
        }
        (ListModalKeyTarget::Dismiss, _) => {
            dispatch_manager(state, ManagerMessage::DismissListModal);
            InputOutcome::Continue
        }
        _ => InputOutcome::Continue,
    }
}

pub fn handle_inline_role_picker(state: &mut ManagerState<'_>, key: KeyEvent) -> InputOutcome {
    let Some(picker) = state.inline_role_picker.as_mut() else {
        return InputOutcome::Continue;
    };
    match inline_picker_shell_plan(key, true) {
        InlinePickerShellPlan::ScrollHorizontal(delta) => {
            dispatch_manager(state, ManagerMessage::ScrollListHorizontal(delta));
            InputOutcome::Continue
        }
        InlinePickerShellPlan::Exit => InputOutcome::ExitJackin,
        InlinePickerShellPlan::Delegate => match inline_picker_plan(picker.handle_key(key)) {
            InlinePickerPlan::Commit(role) => {
                dispatch_manager(state, ManagerMessage::DismissInlineRolePicker);
                InputOutcome::LaunchWithAgent(role)
            }
            InlinePickerPlan::Dismiss => {
                dispatch_manager(state, ManagerMessage::DismissInlineRolePicker);
                InputOutcome::Continue
            }
            InlinePickerPlan::Continue => InputOutcome::Continue,
        },
    }
}

pub fn handle_inline_agent_picker(state: &mut ManagerState<'_>, key: KeyEvent) -> InputOutcome {
    let Some((_, picker)) = state.inline_agent_picker.as_mut() else {
        return InputOutcome::Continue;
    };
    match inline_picker_shell_plan(key, false) {
        InlinePickerShellPlan::ScrollHorizontal(delta) => {
            dispatch_manager(state, ManagerMessage::ScrollListHorizontal(delta));
            InputOutcome::Continue
        }
        InlinePickerShellPlan::Exit => InputOutcome::ExitJackin,
        InlinePickerShellPlan::Delegate => match inline_picker_plan(picker.handle_key(key)) {
            InlinePickerPlan::Commit(agent) => {
                dispatch_manager(state, ManagerMessage::DismissInlineAgentPicker);
                InputOutcome::LaunchWithRuntimeAgent(agent)
            }
            InlinePickerPlan::Dismiss => {
                dispatch_manager(state, ManagerMessage::DismissInlineAgentPicker);
                InputOutcome::Continue
            }
            InlinePickerPlan::Continue => InputOutcome::Continue,
        },
    }
}

/// Handle key events while the new-session agent picker is open in the left
/// sidebar. Commit runs `inline_provider_followup_plan`; the running-container
/// path always supplies an empty provider list (the daemon, not host config,
/// owns the captured env), so in practice this dispatches `NewSessionWithAgent`
/// directly and the provider picker never opens here. Cancel/Esc dismisses.
pub fn handle_new_session_picker(state: &mut ManagerState<'_>, key: KeyEvent) -> InputOutcome {
    let Some((container, picker, providers)) = state.inline_new_session_picker.as_mut() else {
        return InputOutcome::Continue;
    };
    match inline_picker_plan(picker.handle_key(key)) {
        InlinePickerPlan::Commit(agent) => {
            let container = container.clone();
            // Running-container path passes an empty list → no provider picker.
            let plan = inline_provider_followup_plan(container, agent, providers.clone());
            dispatch_manager(state, ManagerMessage::DismissInlineSessionPicker);
            match plan {
                InlineProviderFollowupPlan::StartSession { context, agent } => {
                    InputOutcome::InstanceAction {
                        container: context,
                        action: ConcreteInstanceAction::NewSessionWithAgent(agent),
                    }
                }
                InlineProviderFollowupPlan::OpenProviderPicker(picker) => {
                    apply_inline_provider_picker_plan(state, picker);
                    InputOutcome::Continue
                }
            }
        }
        InlinePickerPlan::Dismiss => {
            dispatch_manager(state, ManagerMessage::DismissInlineSessionPicker);
            InputOutcome::Continue
        }
        InlinePickerPlan::Continue => InputOutcome::Continue,
    }
}

/// Handle key events while the inline provider picker is open (shown after
/// agent selection when multiple providers are available). Enter commits;
/// Esc cancels; Up/Down navigate.
pub fn handle_inline_provider_picker(state: &mut ManagerState<'_>, key: KeyEvent) -> InputOutcome {
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

pub fn handle_launch_provider_picker(state: &mut ManagerState<'_>, key: KeyEvent) -> InputOutcome {
    let Some(picker) = state.launch_provider_picker.as_mut() else {
        return InputOutcome::Continue;
    };
    match picker.handle_key(key.into()) {
        ProviderPickerOutcome::Commit {
            context,
            agent,
            provider,
        } => {
            apply_inline_picker_dismissal_plan(
                state,
                inline_picker_dismissal_plan(InlinePickerDismissal::LaunchProvider),
            );
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
