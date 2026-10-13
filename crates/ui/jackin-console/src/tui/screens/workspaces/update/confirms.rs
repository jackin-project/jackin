// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Confirm builders and horizontal plans.

use super::{
    InstancePurgeConfirmPlan, WorkspaceDeleteConfirmPlan, WorkspaceListHorizontalPlan,
    WorkspaceTreeDisclosurePlan,
};

use crossterm::event::KeyCode;

use super::super::model::ManagerListRow;

#[must_use]
pub fn workspace_delete_confirm_state(name: &str) -> crate::tui::components::ConfirmState {
    crate::tui::components::ConfirmState::new(format!("Delete \"{name}\"?"))
}

#[must_use]
pub fn instance_purge_confirm_state(label: &str) -> crate::tui::components::ConfirmState {
    crate::tui::components::ConfirmState::new(format!(
        "Purge \"{label}\"?\nRemoves the role container, DinD sidecar, volume, network, and local recovery state."
    ))
}

#[must_use]
pub fn workspace_delete_confirm_plan(name: String) -> WorkspaceDeleteConfirmPlan {
    WorkspaceDeleteConfirmPlan {
        state: workspace_delete_confirm_state(&name),
        name,
    }
}

#[must_use]
pub fn instance_purge_confirm_plan(container: String, label: String) -> InstancePurgeConfirmPlan {
    InstancePurgeConfirmPlan {
        state: instance_purge_confirm_state(&label),
        container,
        label,
    }
}

#[must_use]
pub const fn collapse_selected_tree_plan(row: ManagerListRow) -> WorkspaceTreeDisclosurePlan {
    match row {
        ManagerListRow::SavedWorkspace(i) | ManagerListRow::WorkspaceInstance(i, _) => {
            WorkspaceTreeDisclosurePlan::CollapseWorkspace(i)
        }
        ManagerListRow::CurrentDirectory | ManagerListRow::CurrentDirectoryInstance(_) => {
            WorkspaceTreeDisclosurePlan::CollapseCurrentDir
        }
        ManagerListRow::NewWorkspace => WorkspaceTreeDisclosurePlan::None,
    }
}

#[must_use]
pub const fn expand_selected_tree_plan(row: ManagerListRow) -> WorkspaceTreeDisclosurePlan {
    match row {
        ManagerListRow::SavedWorkspace(i) => WorkspaceTreeDisclosurePlan::ExpandWorkspace(i),
        ManagerListRow::CurrentDirectory => WorkspaceTreeDisclosurePlan::ExpandCurrentDir,
        ManagerListRow::CurrentDirectoryInstance(_)
        | ManagerListRow::WorkspaceInstance(_, _)
        | ManagerListRow::NewWorkspace => WorkspaceTreeDisclosurePlan::None,
    }
}

#[must_use]
pub fn workspace_row_owns_left(
    row: ManagerListRow,
    current_dir_expanded: bool,
    current_dir_has_instances: bool,
    mut workspace_expanded: impl FnMut(usize) -> bool,
) -> bool {
    match row {
        ManagerListRow::CurrentDirectory => current_dir_expanded && current_dir_has_instances,
        ManagerListRow::CurrentDirectoryInstance(_) => current_dir_expanded,
        ManagerListRow::SavedWorkspace(i) | ManagerListRow::WorkspaceInstance(i, _) => {
            workspace_expanded(i)
        }
        ManagerListRow::NewWorkspace => false,
    }
}

#[must_use]
pub fn workspace_row_owns_right(
    row: ManagerListRow,
    current_dir_expanded: bool,
    current_dir_has_instances: bool,
    mut workspace_expanded: impl FnMut(usize) -> bool,
    mut workspace_has_instances: impl FnMut(usize) -> bool,
) -> bool {
    match row {
        ManagerListRow::CurrentDirectory => !current_dir_expanded && current_dir_has_instances,
        ManagerListRow::SavedWorkspace(i) => !workspace_expanded(i) && workspace_has_instances(i),
        ManagerListRow::CurrentDirectoryInstance(_)
        | ManagerListRow::WorkspaceInstance(_, _)
        | ManagerListRow::NewWorkspace => false,
    }
}

#[must_use]
pub fn workspace_list_horizontal_plan(
    row: ManagerListRow,
    horizontal_delta: i16,
    current_dir_expanded: bool,
    current_dir_has_instances: bool,
    workspace_expanded: impl FnMut(usize) -> bool,
    workspace_has_instances: impl FnMut(usize) -> bool,
) -> WorkspaceListHorizontalPlan {
    if horizontal_delta < 0 {
        if workspace_row_owns_left(
            row,
            current_dir_expanded,
            current_dir_has_instances,
            workspace_expanded,
        ) {
            WorkspaceListHorizontalPlan::CollapseTree
        } else {
            WorkspaceListHorizontalPlan::Scroll(horizontal_delta)
        }
    } else if horizontal_delta > 0
        && workspace_row_owns_right(
            row,
            current_dir_expanded,
            current_dir_has_instances,
            workspace_expanded,
            workspace_has_instances,
        )
    {
        WorkspaceListHorizontalPlan::ExpandTree
    } else {
        WorkspaceListHorizontalPlan::Scroll(horizontal_delta)
    }
}

#[must_use]
pub const fn workspace_unclamped_scroll_plan(current_scroll: u16, delta: i16) -> u16 {
    crate::tui::update::unclamped_scroll_plan(current_scroll, delta)
}

#[must_use]
pub const fn is_preview_pane_entry_target(key: KeyCode, row: ManagerListRow) -> bool {
    matches!(key, KeyCode::Tab | KeyCode::Right)
        && matches!(
            row,
            ManagerListRow::WorkspaceInstance(_, _) | ManagerListRow::CurrentDirectoryInstance(_)
        )
}

#[must_use]
pub const fn should_enter_preview_pane(
    key: KeyCode,
    row: ManagerListRow,
    pane_count: usize,
) -> bool {
    is_preview_pane_entry_target(key, row) && pane_count > 0
}
