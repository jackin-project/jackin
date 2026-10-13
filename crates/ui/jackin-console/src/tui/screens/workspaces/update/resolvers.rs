// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! List key and scope resolvers.

use super::{
    WorkspaceInstanceAction, WorkspaceInstanceLookupEntry, WorkspaceInstanceLookupScope,
    WorkspaceInstanceScopePlan, WorkspaceListDeletePlan, WorkspaceListEditPlan,
    WorkspaceListEnterPlan, WorkspaceListKeyPlan, WorkspaceListNewSessionOpenPlan,
    WorkspaceListNewSessionPlan, WorkspaceListSelectedInstancePlan, WorkspaceListSettingsPlan,
    instance_action_accepts_status,
};

use crossterm::event::KeyCode;

use super::super::model::ManagerListRow;
use crate::mount_info_cache::MountInfoCache;

use crate::tui::components::github_picker::{GithubOpenPlan, github_open_plan};

#[must_use]
pub const fn workspace_list_prewarm_plan(row: ManagerListRow) -> Option<usize> {
    match row {
        ManagerListRow::SavedWorkspace(idx) => Some(idx),
        ManagerListRow::CurrentDirectory
        | ManagerListRow::NewWorkspace
        | ManagerListRow::WorkspaceInstance(_, _)
        | ManagerListRow::CurrentDirectoryInstance(_) => None,
    }
}

#[must_use]
pub const fn workspace_list_enter_plan(row: ManagerListRow) -> WorkspaceListEnterPlan {
    match row {
        ManagerListRow::CurrentDirectory => WorkspaceListEnterPlan::LaunchCurrentDir,
        ManagerListRow::NewWorkspace => WorkspaceListEnterPlan::CreateNewWorkspace,
        ManagerListRow::SavedWorkspace(idx) => WorkspaceListEnterPlan::LaunchSavedWorkspace(idx),
        ManagerListRow::WorkspaceInstance(_, _) | ManagerListRow::CurrentDirectoryInstance(_) => {
            WorkspaceListEnterPlan::InstanceAction
        }
    }
}

#[must_use]
pub fn workspace_list_key_plan(key: KeyCode, list_scroll_focused: bool) -> WorkspaceListKeyPlan {
    use crate::tui::keymap::{
        WORKSPACE_LIST_KEYMAP, WorkspaceListAction as A, bridged_keymap_action,
    };

    let Some(action) = bridged_keymap_action(
        &WORKSPACE_LIST_KEYMAP,
        termrock::input::KeyEvent::new(
            termrock::input::KeyCode::from(key),
            termrock::input::KeyModifiers::NONE,
        ),
    ) else {
        return WorkspaceListKeyPlan::Continue;
    };
    match action {
        A::Exit => WorkspaceListKeyPlan::Exit,
        A::TreeLeft => WorkspaceListKeyPlan::HorizontalTreeOrScroll { delta: -8 },
        A::TreeRight => WorkspaceListKeyPlan::HorizontalTreeOrScroll { delta: 8 },
        A::ScrollLeft => WorkspaceListKeyPlan::ScrollHorizontal { delta: -8 },
        A::ScrollRight => WorkspaceListKeyPlan::ScrollHorizontal { delta: 8 },
        A::NavigateUp => {
            if list_scroll_focused {
                WorkspaceListKeyPlan::ScrollFocusedVertical { delta: -3 }
            } else {
                WorkspaceListKeyPlan::MoveSelection { delta: -1 }
            }
        }
        A::NavigateDown => {
            if list_scroll_focused {
                WorkspaceListKeyPlan::ScrollFocusedVertical { delta: 3 }
            } else {
                WorkspaceListKeyPlan::MoveSelection { delta: 1 }
            }
        }
        A::Enter => WorkspaceListKeyPlan::Enter,
        A::Edit => WorkspaceListKeyPlan::Edit,
        A::NewSession => WorkspaceListKeyPlan::NewSession,
        A::Delete => WorkspaceListKeyPlan::Delete,
        A::OpenGithub => WorkspaceListKeyPlan::OpenGithub,
        A::InstanceReconnect => {
            WorkspaceListKeyPlan::InstanceAction(WorkspaceInstanceAction::Reconnect)
        }
        A::InstanceNewSession => {
            WorkspaceListKeyPlan::InstanceAction(WorkspaceInstanceAction::NewSession)
        }
        A::InstanceShell => WorkspaceListKeyPlan::InstanceAction(WorkspaceInstanceAction::Shell),
        A::InstanceInspect => {
            WorkspaceListKeyPlan::InstanceAction(WorkspaceInstanceAction::Inspect)
        }
        A::InstanceStop => WorkspaceListKeyPlan::InstanceAction(WorkspaceInstanceAction::Stop),
        A::ConfirmPurge => WorkspaceListKeyPlan::ConfirmPurge,
        A::Settings => WorkspaceListKeyPlan::Settings,
        A::Prewarm => WorkspaceListKeyPlan::Prewarm,
        // Tab/preview entry is resolved upstream in `workspace_list_top_level_key_plan`;
        // Ctrl-Q never reaches here (intercepted by `should_open_quit_confirm`).
        A::EnterPreview | A::Quit => WorkspaceListKeyPlan::Continue,
    }
}

#[must_use]
pub const fn selected_instance_scope_plan(row: ManagerListRow) -> WorkspaceInstanceScopePlan {
    match row {
        ManagerListRow::CurrentDirectory | ManagerListRow::CurrentDirectoryInstance(_) => {
            WorkspaceInstanceScopePlan::CurrentDirectory
        }
        ManagerListRow::SavedWorkspace(idx) => WorkspaceInstanceScopePlan::SavedWorkspace(idx),
        ManagerListRow::WorkspaceInstance(ws_idx, _) => {
            WorkspaceInstanceScopePlan::WorkspaceInstance(ws_idx)
        }
        ManagerListRow::NewWorkspace => WorkspaceInstanceScopePlan::None,
    }
}

#[must_use]
pub const fn selected_instance_plan(row: ManagerListRow) -> WorkspaceListSelectedInstancePlan {
    match row {
        ManagerListRow::CurrentDirectoryInstance(instance_idx) => {
            WorkspaceListSelectedInstancePlan::Direct {
                workspace_idx: None,
                instance_idx,
            }
        }
        ManagerListRow::WorkspaceInstance(workspace_idx, instance_idx) => {
            WorkspaceListSelectedInstancePlan::Direct {
                workspace_idx: Some(workspace_idx),
                instance_idx,
            }
        }
        ManagerListRow::CurrentDirectory | ManagerListRow::SavedWorkspace(_) => {
            WorkspaceListSelectedInstancePlan::Scope
        }
        ManagerListRow::NewWorkspace => WorkspaceListSelectedInstancePlan::None,
    }
}

#[must_use]
pub fn selected_instance_container_for_action<'a>(
    row: ManagerListRow,
    action: WorkspaceInstanceAction,
    mut direct_instance: impl FnMut(Option<usize>, usize) -> Option<WorkspaceInstanceLookupEntry<'a>>,
    mut scope: impl FnMut(WorkspaceInstanceScopePlan) -> Option<WorkspaceInstanceLookupScope<'a>>,
    instances: impl IntoIterator<Item = WorkspaceInstanceLookupEntry<'a>>,
) -> Option<&'a str> {
    match selected_instance_plan(row) {
        WorkspaceListSelectedInstancePlan::Direct {
            workspace_idx,
            instance_idx,
        } => {
            let entry = direct_instance(workspace_idx, instance_idx)?;
            instance_action_accepts_status(action, entry.status).then_some(entry.container)
        }
        WorkspaceListSelectedInstancePlan::Scope => {
            let scope = scope(selected_instance_scope_plan(row))?;
            instances.into_iter().find_map(|entry| {
                (instance_lookup_entry_matches_scope(entry, scope)
                    && instance_action_accepts_status(action, entry.status))
                .then_some(entry.container)
            })
        }
        WorkspaceListSelectedInstancePlan::None => None,
    }
}

#[must_use]
pub fn instance_lookup_entry_matches_scope(
    entry: WorkspaceInstanceLookupEntry<'_>,
    scope: WorkspaceInstanceLookupScope<'_>,
) -> bool {
    entry.workspace_name == scope.workspace_name
        && entry.workspace_label == scope.workspace_label
        && entry.workdir == scope.workdir
}

#[must_use]
pub const fn workspace_list_new_session_plan(row: ManagerListRow) -> WorkspaceListNewSessionPlan {
    match row {
        ManagerListRow::WorkspaceInstance(workspace_idx, instance_idx) => {
            WorkspaceListNewSessionPlan::ExistingWorkspaceInstance {
                workspace_idx,
                instance_idx,
            }
        }
        ManagerListRow::CurrentDirectory
        | ManagerListRow::CurrentDirectoryInstance(_)
        | ManagerListRow::SavedWorkspace(_)
        | ManagerListRow::NewWorkspace => WorkspaceListNewSessionPlan::CreateWorkspace,
    }
}

#[must_use]
pub fn workspace_list_new_session_open_plan(
    plan: WorkspaceListNewSessionPlan,
    workspace_instance_container: impl FnOnce(usize, usize) -> Option<String>,
) -> WorkspaceListNewSessionOpenPlan {
    match plan {
        WorkspaceListNewSessionPlan::ExistingWorkspaceInstance {
            workspace_idx,
            instance_idx,
        } => workspace_instance_container(workspace_idx, instance_idx).map_or(
            WorkspaceListNewSessionOpenPlan::OpenInstanceUnavailableError,
            |container| WorkspaceListNewSessionOpenPlan::OpenPicker { container },
        ),
        WorkspaceListNewSessionPlan::CreateWorkspace => {
            WorkspaceListNewSessionOpenPlan::OpenCreateWorkspace
        }
    }
}

#[must_use]
pub const fn workspace_list_edit_plan(row: ManagerListRow) -> WorkspaceListEditPlan {
    match row {
        ManagerListRow::SavedWorkspace(workspace_idx) => {
            WorkspaceListEditPlan::OpenEditor { workspace_idx }
        }
        ManagerListRow::CurrentDirectory
        | ManagerListRow::CurrentDirectoryInstance(_)
        | ManagerListRow::WorkspaceInstance(_, _)
        | ManagerListRow::NewWorkspace => WorkspaceListEditPlan::Noop,
    }
}

#[must_use]
pub const fn workspace_list_delete_plan(row: ManagerListRow) -> WorkspaceListDeletePlan {
    match row {
        ManagerListRow::SavedWorkspace(workspace_idx) => {
            WorkspaceListDeletePlan::ConfirmDelete { workspace_idx }
        }
        ManagerListRow::CurrentDirectory
        | ManagerListRow::CurrentDirectoryInstance(_)
        | ManagerListRow::WorkspaceInstance(_, _)
        | ManagerListRow::NewWorkspace => WorkspaceListDeletePlan::Noop,
    }
}

#[must_use]
pub const fn workspace_list_settings_plan(row: ManagerListRow) -> WorkspaceListSettingsPlan {
    match row {
        ManagerListRow::CurrentDirectory
        | ManagerListRow::SavedWorkspace(_)
        | ManagerListRow::NewWorkspace => WorkspaceListSettingsPlan::OpenSettings,
        ManagerListRow::CurrentDirectoryInstance(_) | ManagerListRow::WorkspaceInstance(_, _) => {
            WorkspaceListSettingsPlan::Noop
        }
    }
}

#[must_use]
pub fn workspace_list_github_open_plan(
    selected_workspace_name: Option<&str>,
    config: &jackin_config::AppConfig,
    mount_info_cache: &MountInfoCache,
) -> GithubOpenPlan {
    let Some(name) = selected_workspace_name else {
        return GithubOpenPlan::Continue;
    };
    let Some(workspace) = config.workspaces.get(name) else {
        return GithubOpenPlan::Continue;
    };
    github_open_plan(crate::github_mounts::resolve_for_workspace_from_cache(
        workspace,
        mount_info_cache,
    ))
}

#[must_use]
pub const fn workspace_list_current_directory_selected(row: ManagerListRow) -> bool {
    matches!(row, ManagerListRow::CurrentDirectory)
}

#[must_use]
pub const fn workspace_list_new_workspace_selected(row: ManagerListRow) -> bool {
    matches!(row, ManagerListRow::NewWorkspace)
}
