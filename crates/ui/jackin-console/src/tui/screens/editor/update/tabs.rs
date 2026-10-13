// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Editor tab move and hover plans.

use super::editor_mount_index_at_visual_row;

use super::super::model::{EditorHoverTarget, EditorTab};

use jackin_config::MountConfig;

#[must_use]
pub const fn previous_editor_tab(tab: EditorTab) -> EditorTab {
    match tab {
        EditorTab::General => EditorTab::Auth,
        EditorTab::Mounts => EditorTab::General,
        EditorTab::Roles => EditorTab::Mounts,
        EditorTab::Secrets => EditorTab::Roles,
        EditorTab::Auth => EditorTab::Secrets,
    }
}

#[must_use]
pub const fn next_editor_tab(tab: EditorTab) -> EditorTab {
    match tab {
        EditorTab::General => EditorTab::Mounts,
        EditorTab::Mounts => EditorTab::Roles,
        EditorTab::Roles => EditorTab::Secrets,
        EditorTab::Secrets => EditorTab::Auth,
        EditorTab::Auth => EditorTab::General,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorTabMovePlan {
    pub active_tab: EditorTab,
    pub tab_bar_focused: bool,
    pub active_row: usize,
    pub tab_scroll_x: u16,
    pub tab_scroll_y: u16,
    pub clear_secret_view_state: bool,
}

#[must_use]
pub const fn editor_tab_bar_focus_plan(focused: bool) -> bool {
    focused
}

#[must_use]
pub fn editor_tab_at_position(row: u16, col: u16) -> Option<EditorTab> {
    let labels: Vec<&str> = EditorTab::ALL.iter().map(|tab| tab.label()).collect();
    let idx = crate::tui::layout::tab_cell_at_position(row, col, &labels)?;
    EditorTab::ALL.get(idx).copied()
}

#[must_use]
pub fn editor_tab_hover_plan(row: u16, col: u16) -> Option<usize> {
    let labels: Vec<&str> = EditorTab::ALL.iter().map(|tab| tab.label()).collect();
    crate::tui::layout::tab_hover_index_at_position(row, col, &labels)
}

#[must_use]
pub fn editor_tab_hover_target_plan(
    modal_open: bool,
    row: u16,
    col: u16,
) -> Option<EditorHoverTarget> {
    (!modal_open)
        .then(|| editor_tab_hover_plan(row, col).map(EditorHoverTarget::Tab))
        .flatten()
}

#[must_use]
pub fn editor_mount_index_at_position(
    active_tab: EditorTab,
    modal_open: bool,
    area: ratatui::layout::Rect,
    col: u16,
    row: u16,
    scroll_y: u16,
    mounts: &[MountConfig],
) -> Option<usize> {
    if active_tab != EditorTab::Mounts || modal_open {
        return None;
    }
    crate::tui::layout::bordered_content_hit_at_position(area, col, row, scroll_y, |visual_row| {
        editor_mount_index_at_visual_row(mounts, visual_row)
    })
}

#[must_use]
pub fn editor_mount_hover_target_at_position(
    active_tab: EditorTab,
    modal_open: bool,
    area: ratatui::layout::Rect,
    col: u16,
    row: u16,
    scroll_y: u16,
    mounts: &[MountConfig],
) -> Option<EditorHoverTarget> {
    editor_mount_index_at_position(active_tab, modal_open, area, col, row, scroll_y, mounts)
        .map(EditorHoverTarget::MountRow)
}

#[must_use]
pub const fn editor_tab_move_plan(
    active_tab: EditorTab,
    delta: isize,
    focus_tab_bar: bool,
) -> EditorTabMovePlan {
    let next = if delta.is_negative() {
        previous_editor_tab(active_tab)
    } else {
        next_editor_tab(active_tab)
    };
    EditorTabMovePlan {
        active_tab: next,
        tab_bar_focused: focus_tab_bar,
        active_row: 0,
        tab_scroll_x: 0,
        tab_scroll_y: 0,
        clear_secret_view_state: matches!(active_tab, EditorTab::Secrets),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorTabSelectPlan {
    pub active_tab: EditorTab,
    pub tab_bar_focused: bool,
    pub active_row: usize,
    pub workspace_mounts_scroll_focused: bool,
    pub clear_secret_view_state: bool,
}

#[must_use]
pub const fn editor_tab_select_plan(
    previous_tab: EditorTab,
    selected_tab: EditorTab,
) -> EditorTabSelectPlan {
    EditorTabSelectPlan {
        active_tab: selected_tab,
        tab_bar_focused: true,
        active_row: 0,
        workspace_mounts_scroll_focused: false,
        clear_secret_view_state: matches!(previous_tab, EditorTab::Secrets)
            && !matches!(selected_tab, EditorTab::Secrets),
    }
}
