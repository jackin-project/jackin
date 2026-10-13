// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! List selection index plans.

use super::{WorkspaceCollapseSelectionPlan, workspace_last_selectable_index, workspace_row_index};

use super::super::model::ManagerListRow;

#[must_use]
pub const fn initial_workspace_selected_index(
    saved_count: usize,
    matching_saved_index: Option<usize>,
) -> usize {
    let selected_row = match matching_saved_index {
        Some(idx) => ManagerListRow::SavedWorkspace(idx),
        None => ManagerListRow::CurrentDirectory,
    };
    match selected_row.to_screen_index(saved_count) {
        Some(idx) => idx,
        None => 0,
    }
}

#[must_use]
pub const fn saved_workspace_selected_index(saved_count: usize, saved_index: usize) -> usize {
    match ManagerListRow::SavedWorkspace(saved_index).to_screen_index(saved_count) {
        Some(idx) => idx,
        None => 0,
    }
}

#[must_use]
pub const fn collapse_current_dir_selection_plan(
    row: ManagerListRow,
) -> WorkspaceCollapseSelectionPlan {
    match row {
        ManagerListRow::CurrentDirectoryInstance(_) => WorkspaceCollapseSelectionPlan::Parent,
        ManagerListRow::CurrentDirectory
        | ManagerListRow::SavedWorkspace(_)
        | ManagerListRow::WorkspaceInstance(_, _)
        | ManagerListRow::NewWorkspace => WorkspaceCollapseSelectionPlan::Clamp,
    }
}

#[must_use]
pub const fn collapsed_current_dir_selected_index(selected_row: ManagerListRow) -> Option<usize> {
    match collapse_current_dir_selection_plan(selected_row) {
        WorkspaceCollapseSelectionPlan::Parent => Some(0),
        WorkspaceCollapseSelectionPlan::Clamp => None,
    }
}

#[must_use]
pub const fn collapse_workspace_selection_plan(
    row: ManagerListRow,
    workspace_idx: usize,
) -> WorkspaceCollapseSelectionPlan {
    match row {
        ManagerListRow::WorkspaceInstance(row_workspace_idx, _)
            if row_workspace_idx == workspace_idx =>
        {
            WorkspaceCollapseSelectionPlan::Parent
        }
        ManagerListRow::CurrentDirectory
        | ManagerListRow::CurrentDirectoryInstance(_)
        | ManagerListRow::SavedWorkspace(_)
        | ManagerListRow::WorkspaceInstance(_, _)
        | ManagerListRow::NewWorkspace => WorkspaceCollapseSelectionPlan::Clamp,
    }
}

#[must_use]
pub fn collapsed_workspace_selected_index(
    rows: &[ManagerListRow],
    selected: usize,
    selected_row: ManagerListRow,
    workspace_idx: usize,
) -> Option<usize> {
    match collapse_workspace_selection_plan(selected_row, workspace_idx) {
        WorkspaceCollapseSelectionPlan::Parent => {
            workspace_row_index(rows, ManagerListRow::SavedWorkspace(workspace_idx))
        }
        WorkspaceCollapseSelectionPlan::Clamp => {
            Some(selected.min(workspace_last_selectable_index(rows.len())))
        }
    }
}

#[must_use]
pub const fn workspace_list_saved_workspace_index(row: ManagerListRow) -> Option<usize> {
    match row {
        ManagerListRow::SavedWorkspace(idx) => Some(idx),
        ManagerListRow::CurrentDirectory
        | ManagerListRow::CurrentDirectoryInstance(_)
        | ManagerListRow::NewWorkspace
        | ManagerListRow::WorkspaceInstance(_, _) => None,
    }
}

#[must_use]
pub const fn workspace_list_settings_available(row: ManagerListRow) -> bool {
    !matches!(
        row,
        ManagerListRow::WorkspaceInstance(_, _) | ManagerListRow::CurrentDirectoryInstance(_)
    )
}
