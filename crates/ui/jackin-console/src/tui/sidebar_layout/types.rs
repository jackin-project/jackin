// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Sidebar layout metrics and selection types.

use ratatui::layout::Rect;

use crate::tui::screens::workspaces::model::ManagerListRow;

/// Fixed height of the compact running-instances badge (borders + 1 text line).
pub const COMPACT_INSTANCES_HEIGHT: u16 = 3;

/// Root-derived heights and visibility flags for sidebar layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SidebarLayoutMetrics {
    pub instance_count: usize,
    pub workspace_mount_height: u16,
    pub global_mount_height: Option<u16>,
    pub role_global_mount_height: Option<u16>,
    pub env_height: Option<u16>,
    pub show_roles: bool,
    pub agent_count: usize,
}

/// Rect for each rendered block. `None` panels are skipped in both render
/// and hit-test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SidebarLayout {
    pub instances: Option<Rect>,
    pub general: Rect,
    pub mounts: Rect,
    pub global: Option<Rect>,
    pub role_global: Option<Rect>,
    pub env: Option<Rect>,
    pub roles: Option<Rect>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SidebarScrollArea {
    pub area: Rect,
    pub content_width: usize,
    pub content_height: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SidebarScrollAreas {
    pub workspace: SidebarScrollArea,
    pub global: SidebarScrollArea,
    pub role_global: Option<SidebarScrollArea>,
    pub roles: Option<SidebarScrollArea>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarScrollFocus {
    Workspace,
    Global,
    RoleGlobal,
    Roles,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectedSidebarTarget {
    CurrentDirectory,
    SavedWorkspace(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlobalMountRowsSelection<Role> {
    None,
    CurrentDirectory,
    SavedWorkspace { picker_role: Option<Role> },
}

#[must_use]
pub const fn selected_sidebar_target(row: ManagerListRow) -> Option<SelectedSidebarTarget> {
    match row {
        ManagerListRow::CurrentDirectory => Some(SelectedSidebarTarget::CurrentDirectory),
        ManagerListRow::SavedWorkspace(idx) => Some(SelectedSidebarTarget::SavedWorkspace(idx)),
        ManagerListRow::NewWorkspace
        | ManagerListRow::WorkspaceInstance(_, _)
        | ManagerListRow::CurrentDirectoryInstance(_) => None,
    }
}

#[must_use]
pub fn global_mount_rows_selection<Role>(
    row: ManagerListRow,
    saved_workspace_exists: impl FnOnce(usize) -> bool,
    picker_role: Option<Role>,
) -> GlobalMountRowsSelection<Role> {
    match row {
        ManagerListRow::CurrentDirectory | ManagerListRow::CurrentDirectoryInstance(_) => {
            GlobalMountRowsSelection::CurrentDirectory
        }
        ManagerListRow::SavedWorkspace(idx) if saved_workspace_exists(idx) => {
            GlobalMountRowsSelection::SavedWorkspace { picker_role }
        }
        ManagerListRow::SavedWorkspace(_)
        | ManagerListRow::NewWorkspace
        | ManagerListRow::WorkspaceInstance(_, _) => GlobalMountRowsSelection::None,
    }
}

#[must_use]
pub fn inline_picker_role<Role>(
    selected_role_picker_role: Option<Role>,
    agent_picker_role: Option<Role>,
) -> Option<Role> {
    selected_role_picker_role.or(agent_picker_role)
}

#[must_use]
pub const fn inline_picker_active(role_picker_open: bool, agent_picker_open: bool) -> bool {
    role_picker_open || agent_picker_open
}
