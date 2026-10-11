// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `ManagerState` workspace-list row queries.

use super::record_manager_recovery;

use crate::tui::screens::workspaces::model::hovered_list_row;
use crate::tui::screens::workspaces::update::{
    workspace_last_selectable_index, workspace_list_current_directory_selected,
    workspace_list_new_workspace_selected, workspace_row_at_visual_index, workspace_selected_row,
    workspace_visual_selected_index,
};

use super::super::{ManagerListRow, ManagerState};

impl ManagerState<'_> {
    /// Flat ordered list of selectable rows accounting for tree expansion.
    /// Instance rows appear immediately after their parent workspace row.
    pub(crate) fn selectable_rows_vec(&self) -> Vec<ManagerListRow> {
        let workspace_instance_counts = self.workspace_instance_counts();
        crate::tui::screens::workspaces::selection::WorkspaceSelection::projection(
            crate::tui::screens::workspaces::update::WorkspaceRowLayout {
                current_dir_expanded: self.current_dir_expanded,
                current_dir_instance_count: self.current_dir_visible_instances().len(),
                workspace_instance_counts: &workspace_instance_counts,
                expanded_workspaces: &self.expanded_workspaces,
            },
        )
    }

    /// Visual row list for rendering — same as `selectable_rows_vec` plus a
    /// `None` spacer before `NewWorkspace` when saved workspaces exist.
    pub fn visual_rows_vec(&self) -> Vec<Option<ManagerListRow>> {
        let workspace_instance_counts = self.workspace_instance_counts();
        crate::tui::screens::workspaces::update::visual_rows(
            crate::tui::screens::workspaces::update::WorkspaceRowLayout {
                current_dir_expanded: self.current_dir_expanded,
                current_dir_instance_count: self.current_dir_visible_instances().len(),
                workspace_instance_counts: &workspace_instance_counts,
                expanded_workspaces: &self.expanded_workspaces,
            },
        )
    }

    #[must_use]
    pub const fn hovered_list_row(&self) -> Option<ManagerListRow> {
        hovered_list_row(self.hover_target)
    }

    pub(crate) fn workspace_instance_counts(&self) -> Vec<usize> {
        self.workspaces
            .iter()
            .enumerate()
            .map(|(i, _)| self.workspace_visible_instances(i).len())
            .collect()
    }

    /// Returns the position of `row` in `selectable_rows_vec`, or `None`.
    #[must_use]
    pub fn index_of_row(&self, row: ManagerListRow) -> Option<usize> {
        crate::tui::screens::workspaces::selection::WorkspaceSelection::index_of(
            &self.selectable_rows_vec(),
            row,
        )
    }

    // ── Core navigation ───────────────────────────────────────────

    /// Total number of selectable rows (includes instance rows when expanded).
    #[must_use]
    pub fn row_count(&self) -> usize {
        self.selectable_rows_vec().len()
    }

    /// Index of the "+ New workspace" sentinel row in the selectable list.
    #[must_use]
    pub fn new_workspace_row_index(&self) -> usize {
        workspace_last_selectable_index(self.selectable_rows_vec().len())
    }

    /// Decode a selectable-list index into a [`ManagerListRow`].
    #[must_use]
    pub fn row_at(&self, idx: usize) -> Option<ManagerListRow> {
        crate::tui::screens::workspaces::selection::WorkspaceSelection::row_at(
            &self.selectable_rows_vec(),
            idx,
        )
    }

    /// Decode a visual-list index (may include the non-selectable spacer)
    /// into a [`ManagerListRow`]. Returns `None` for the spacer row.
    #[must_use]
    pub fn row_at_visual_index(&self, idx: usize) -> Option<ManagerListRow> {
        workspace_row_at_visual_index(&self.visual_rows_vec(), idx)
    }

    /// Visual-list index of the currently selected row (for ratatui
    /// highlight). Differs from `selected` when instance rows are visible.
    #[must_use]
    pub fn visual_selected(&self) -> usize {
        let selected = self.selected_row();
        let visual_rows = self.visual_rows_vec();
        workspace_visual_selected_index(&visual_rows, selected).unwrap_or_else(|| {
            record_manager_recovery();
            0 // CurrentDirectory is always row 0 and is never removed
        })
    }

    /// What the operator currently has highlighted.
    #[must_use]
    pub fn selected_row(&self) -> ManagerListRow {
        workspace_selected_row(&self.selectable_rows_vec(), self.selected)
    }

    /// Convenience: `true` when the selection is on the synthetic
    /// "Current directory" row.
    #[must_use]
    pub fn is_current_dir_selected(&self) -> bool {
        workspace_list_current_directory_selected(self.selected_row())
    }

    /// Convenience: `true` when the selection is on the "+ New workspace"
    /// sentinel.
    #[must_use]
    pub fn is_new_workspace_selected(&self) -> bool {
        workspace_list_new_workspace_selected(self.selected_row())
    }

    /// Whether the workspace tree node at `ws_idx` is expanded.
    #[must_use]
    pub fn is_workspace_expanded(&self, ws_idx: usize) -> bool {
        self.expanded_workspaces.contains(&ws_idx)
    }
}
