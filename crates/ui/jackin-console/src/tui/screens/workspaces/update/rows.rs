// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Workspace row layout queries.

use std::collections::BTreeSet;

use super::super::model::ManagerListRow;

#[derive(Debug, Clone, Copy)]
pub struct WorkspaceRowLayout<'a> {
    pub current_dir_expanded: bool,
    pub current_dir_instance_count: usize,
    pub workspace_instance_counts: &'a [usize],
    pub expanded_workspaces: &'a BTreeSet<usize>,
}

#[must_use]
pub fn selectable_rows(layout: WorkspaceRowLayout<'_>) -> Vec<ManagerListRow> {
    let mut rows = vec![ManagerListRow::CurrentDirectory];
    if layout.current_dir_expanded {
        rows.extend(
            (0..layout.current_dir_instance_count).map(ManagerListRow::CurrentDirectoryInstance),
        );
    }
    for (i, count) in layout.workspace_instance_counts.iter().copied().enumerate() {
        rows.push(ManagerListRow::SavedWorkspace(i));
        if layout.expanded_workspaces.contains(&i) {
            rows.extend((0..count).map(|j| ManagerListRow::WorkspaceInstance(i, j)));
        }
    }
    rows.push(ManagerListRow::NewWorkspace);
    rows
}

#[must_use]
pub fn visual_rows(layout: WorkspaceRowLayout<'_>) -> Vec<Option<ManagerListRow>> {
    let mut rows = selectable_rows(layout)
        .into_iter()
        .map(Some)
        .collect::<Vec<_>>();
    if !layout.workspace_instance_counts.is_empty() {
        let insert_at = rows.len().saturating_sub(1);
        rows.insert(insert_at, None);
    }
    rows
}

#[must_use]
pub fn workspace_visual_selected_index(
    visual_rows: &[Option<ManagerListRow>],
    selected: ManagerListRow,
) -> Option<usize> {
    visual_rows
        .iter()
        .position(|row| row.as_ref() == Some(&selected))
}

#[must_use]
pub fn workspace_row_index(rows: &[ManagerListRow], row: ManagerListRow) -> Option<usize> {
    rows.iter().position(|candidate| *candidate == row)
}

#[must_use]
pub fn workspace_row_at(rows: &[ManagerListRow], idx: usize) -> Option<ManagerListRow> {
    rows.get(idx).copied()
}

#[must_use]
pub fn workspace_selected_row(rows: &[ManagerListRow], selected: usize) -> ManagerListRow {
    workspace_row_at(rows, selected).unwrap_or(ManagerListRow::CurrentDirectory)
}

#[must_use]
pub fn workspace_row_at_visual_index(
    visual_rows: &[Option<ManagerListRow>],
    idx: usize,
) -> Option<ManagerListRow> {
    visual_rows.get(idx).copied().flatten()
}

#[must_use]
pub const fn workspace_last_selectable_index(row_count: usize) -> usize {
    row_count.saturating_sub(1)
}
