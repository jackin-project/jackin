// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `ManagerState` sessions, preview, and expansion.

use super::record_manager_recovery;

use crate::tui::screens::workspaces::update::{
    collapsed_current_dir_selected_index, collapsed_workspace_selected_index,
    preview_pane_selected_index, workspace_list_saved_workspace_index,
};

use super::super::{ManagerState, WorkspaceSummary};

impl ManagerState<'_> {
    /// Recorded sessions for `container_base`, or an empty slice when none
    /// are cached (no sessions or manifest not yet loaded).
    #[must_use]
    pub fn sessions_for_instance(&self, container_base: &str) -> &[jackin_core::SessionRecord] {
        self.instance_sessions
            .get(container_base)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    /// Returns `true` when the last `refresh_instances` pass failed to read
    /// the instance manifest for `container_base`.
    #[must_use]
    pub fn has_session_load_error(&self, container_base: &str) -> bool {
        self.instance_session_errors.contains(container_base)
    }

    /// Live tab/pane snapshot the daemon reported in the last
    /// `refresh_instances` tick, or `None` when the bind-mounted socket
    /// is absent or the fetch failed. `render_instance_details_pane`
    /// prefers this over the on-disk manifest sessions when present.
    #[must_use]
    pub fn snapshot_for_instance(
        &self,
        container_base: &str,
    ) -> Option<&jackin_protocol::InstanceSnapshot> {
        self.instance_snapshots.get(container_base)
    }

    /// Flatten the per-instance snapshot's tab/pane tree into a
    /// linear list the preview's ↑/↓ navigation can index into.
    /// Each entry is `(tab_idx, session_id)`. Empty when no
    /// snapshot exists for the container.
    #[must_use]
    pub fn flattened_preview_panes(&self, container_base: &str) -> Vec<(usize, u64)> {
        let Some(snapshot) = self.instance_snapshots.get(container_base) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (tab_idx, tab) in snapshot.tabs.iter().enumerate() {
            for pane in &tab.panes {
                out.push((tab_idx, pane.session_id));
            }
        }
        out
    }

    /// Currently-selected pane in the preview, clamped against the
    /// flattened list. Returns `None` when the snapshot is missing
    /// or the list is empty.
    #[must_use]
    pub fn preview_selected_pane(&self, container_base: &str) -> Option<(usize, u64)> {
        let panes = self.flattened_preview_panes(container_base);
        if panes.is_empty() {
            return None;
        }
        let cursor = preview_pane_selected_index(
            panes.len(),
            self.preview_pane_cursor.get(container_base).copied(),
        )?;
        panes.get(cursor).copied()
    }

    /// The [`WorkspaceSummary`] currently highlighted, or `None` when the
    /// selection is on Current Directory, New Workspace, or a `WorkspaceInstance`.
    #[must_use]
    pub fn selected_workspace_summary(&self) -> Option<&WorkspaceSummary> {
        workspace_list_saved_workspace_index(self.selected_row())
            .and_then(|i| self.workspaces.get(i))
    }

    // ── Tree expand / collapse ────────────────────────────────────

    /// Expand the workspace tree node at `ws_idx`. No-op when already
    /// expanded or when there are no visible instances.
    pub fn expand_workspace(&mut self, ws_idx: usize) {
        if self.has_visible_instances(ws_idx) {
            self.expanded_workspaces.insert(ws_idx);
        }
    }

    /// Expand the synthetic "Current directory" row. No-op when
    /// already expanded or when no instances point at the cwd.
    pub fn expand_current_dir(&mut self) {
        if self.has_current_dir_visible_instances() {
            self.current_dir_expanded = true;
        }
    }

    /// Collapse the synthetic "Current directory" row. When the
    /// cursor is on one of its instance children, jumps the cursor
    /// up to the parent row first.
    pub fn collapse_current_dir(&mut self) {
        if !self.current_dir_expanded {
            return;
        }
        let selected = collapsed_current_dir_selected_index(self.selected_row());
        self.current_dir_expanded = false;
        if let Some(selected) = selected {
            self.selected = selected;
        }
    }

    /// Collapse the workspace tree node at `ws_idx`. When the cursor is
    /// on a child instance row, jumps up to the workspace row.
    pub fn collapse_workspace(&mut self, ws_idx: usize) {
        if !self.expanded_workspaces.contains(&ws_idx) {
            return;
        }
        let selected_row = self.selected_row();
        self.expanded_workspaces.remove(&ws_idx);
        let rows = self.selectable_rows_vec();
        self.selected =
            collapsed_workspace_selected_index(&rows, self.selected, selected_row, ws_idx)
                .unwrap_or_else(|| {
                    record_manager_recovery();
                    0 // CurrentDirectory is always row 0 and is never removed
                });
    }
}
