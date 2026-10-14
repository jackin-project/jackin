// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::model::ManagerListRow;
use super::update::{WorkspaceRowLayout, selectable_rows, workspace_row_at, workspace_row_index};

#[derive(Debug, Clone, Copy, Default)]
pub struct WorkspaceSelection;

impl WorkspaceSelection {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    #[must_use]
    pub fn projection(layout: WorkspaceRowLayout<'_>) -> Vec<ManagerListRow> {
        selectable_rows(layout)
    }

    #[must_use]
    pub fn index_of(rows: &[ManagerListRow], row: ManagerListRow) -> Option<usize> {
        workspace_row_index(rows, row)
    }

    #[must_use]
    pub fn row_at(rows: &[ManagerListRow], index: usize) -> Option<ManagerListRow> {
        workspace_row_at(rows, index)
    }

    #[must_use]
    pub fn move_index(selected: usize, row_count: usize, delta: isize) -> usize {
        crate::tui::focus::collection_move_index(selected, row_count, delta)
    }

    #[must_use]
    pub fn move_to(target: usize, row_count: usize) -> usize {
        Self::move_index(0, row_count, isize::try_from(target).unwrap_or(isize::MAX))
    }
}

#[cfg(test)]
mod tests;
