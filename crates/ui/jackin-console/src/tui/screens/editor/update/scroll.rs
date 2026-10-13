// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Editor scroll and selection plans.

use super::super::model::EditorTab;

use jackin_config::MountConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorFieldSelectionPlan {
    pub active_row: usize,
    pub tab_scroll_y: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorMountRowSelectPlan {
    pub active_row: usize,
    pub workspace_mounts_scroll_focused: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorScrollFocusPlan {
    pub workspace_mounts_scroll_focused: bool,
    pub tab_content_scroll_focused: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorHorizontalScrollPlan {
    pub scroll_x: u16,
    pub workspace_mounts_scroll_focused: bool,
    pub tab_content_scroll_focused: bool,
}

#[must_use]
pub fn editor_tab_horizontal_scroll_plan(
    current_scroll_x: u16,
    delta: i16,
    term_width: u16,
    content_width: usize,
) -> EditorHorizontalScrollPlan {
    EditorHorizontalScrollPlan {
        scroll_x: crate::tui::update::term_width_scroll_plan(
            current_scroll_x,
            delta,
            term_width,
            content_width,
        ),
        workspace_mounts_scroll_focused: false,
        tab_content_scroll_focused: true,
    }
}

#[must_use]
pub fn editor_workspace_mounts_horizontal_scroll_plan(
    current_scroll_x: u16,
    delta: i16,
    term_width: u16,
    content_width: usize,
) -> EditorHorizontalScrollPlan {
    EditorHorizontalScrollPlan {
        scroll_x: crate::tui::update::term_width_scroll_plan(
            current_scroll_x,
            delta,
            term_width,
            content_width,
        ),
        workspace_mounts_scroll_focused: true,
        tab_content_scroll_focused: false,
    }
}

#[must_use]
pub const fn editor_scroll_focus_plan(
    active_tab: EditorTab,
    modal_open: bool,
    in_workspace_mounts: bool,
    in_tab_content: bool,
) -> EditorScrollFocusPlan {
    if modal_open {
        return EditorScrollFocusPlan {
            workspace_mounts_scroll_focused: false,
            tab_content_scroll_focused: false,
        };
    }
    if matches!(active_tab, EditorTab::Mounts) {
        EditorScrollFocusPlan {
            workspace_mounts_scroll_focused: in_workspace_mounts,
            tab_content_scroll_focused: false,
        }
    } else {
        EditorScrollFocusPlan {
            workspace_mounts_scroll_focused: false,
            tab_content_scroll_focused: in_tab_content,
        }
    }
}

#[must_use]
pub const fn editor_mount_row_select_plan(row: usize) -> EditorMountRowSelectPlan {
    EditorMountRowSelectPlan {
        active_row: row,
        workspace_mounts_scroll_focused: true,
    }
}

#[must_use]
pub fn editor_mount_index_at_visual_row(mounts: &[MountConfig], row: usize) -> Option<usize> {
    if row == 0 {
        return None;
    }

    let mut visual = 1usize;
    for (index, mount) in mounts.iter().enumerate() {
        if row == visual {
            return Some(index);
        }
        visual += 1;
        if mount.src != mount.dst {
            if row == visual {
                return Some(index);
            }
            visual += 1;
        }
    }

    if !mounts.is_empty() {
        if row == visual {
            return None;
        }
        visual += 1;
    }

    (row == visual).then_some(mounts.len())
}
