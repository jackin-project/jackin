// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Two-level workspaces-list selection (workspace cursor + instance
//! sub-rows) re-hosted as a product wrapper over upstream flat
//! `CollectionState` — upstream has no two-level selection model (spec
//! carve-out, plan 009).
//!
//! The wrapper owns the flat `ManagerListRow` projection and the
//! row ↔ flat-index mapping; movement rides `CollectionState` keyed by
//! flat index (the plan-sanctioned keying) with `.wrap(false)` — the
//! pre-cutover list saturates at both ends. Movement constructs the
//! collection per call: `ManagerState.selected` stays the stored cursor,
//! and upstream `CollectionState` exposes no `set_active` to re-seed a
//! persistent instance after click-selects.

use super::model::ManagerListRow;
use super::update::{WorkspaceRowLayout, selectable_rows, workspace_row_at, workspace_row_index};

/// Stateless driver for the workspaces list's two-level selection.
#[derive(Debug, Clone, Copy, Default)]
pub struct WorkspaceSelection;

impl WorkspaceSelection {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Flat selectable-row projection: the two-level row space
    /// (workspaces, their instance sub-rows, the synthetic directory and
    /// new-workspace rows) flattened in paint order.
    #[must_use]
    pub fn projection(layout: WorkspaceRowLayout<'_>) -> Vec<ManagerListRow> {
        selectable_rows(layout)
    }

    /// Flat index of a logical row within the projection.
    #[must_use]
    pub fn index_of(rows: &[ManagerListRow], row: ManagerListRow) -> Option<usize> {
        workspace_row_index(rows, row)
    }

    /// Logical row at a flat projection index.
    #[must_use]
    pub fn row_at(rows: &[ManagerListRow], index: usize) -> Option<ManagerListRow> {
        workspace_row_at(rows, index)
    }

    /// Cursor move over the flat projection, saturating at both ends.
    #[must_use]
    pub fn move_index(selected: usize, row_count: usize, delta: isize) -> usize {
        crate::tui::focus::collection_move_index(selected, row_count, delta)
    }

    /// Absolute cursor set, clamped into the projection.
    #[must_use]
    pub fn move_to(target: usize, row_count: usize) -> usize {
        Self::move_index(0, row_count, isize::try_from(target).unwrap_or(isize::MAX))
    }
}

#[cfg(test)]
mod tests;
