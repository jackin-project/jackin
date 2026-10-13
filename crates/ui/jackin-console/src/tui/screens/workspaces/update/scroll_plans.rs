// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Scroll and selection plans.

use super::{
    WorkspaceListScrollFocusPlan, WorkspaceListScrollTargetPlan, WorkspaceListSelectionPlan,
};

#[expect(
    clippy::fn_params_excessive_bools,
    reason = "Six orthogonal workspace-list scroll-focus inputs (in_left_pane, \
              has_scroll_areas, ...) — each is an independent UI signal the \
              scroll-focus planner reads to pick the correct focus target. \
              Named-arg reads match the per-input scroll-focus routing idiom."
)]
#[must_use]
pub const fn workspace_list_scroll_focus_plan(
    in_left_pane: bool,
    has_scroll_areas: bool,
    in_workspace_mounts: bool,
    in_global_mounts: bool,
    in_role_global_mounts: bool,
    in_roles: bool,
) -> WorkspaceListScrollFocusPlan {
    if in_left_pane {
        return WorkspaceListScrollFocusPlan {
            list_names_focused: true,
            scroll_focus: None,
        };
    }
    let scroll_focus = if !has_scroll_areas {
        None
    } else if in_workspace_mounts {
        Some(crate::tui::focus::MountScrollFocus::Workspace)
    } else if in_global_mounts {
        Some(crate::tui::focus::MountScrollFocus::Global)
    } else if in_role_global_mounts {
        Some(crate::tui::focus::MountScrollFocus::RoleGlobal)
    } else if in_roles {
        Some(crate::tui::focus::MountScrollFocus::Roles)
    } else {
        None
    };
    WorkspaceListScrollFocusPlan {
        list_names_focused: false,
        scroll_focus,
    }
}

#[must_use]
pub const fn workspace_list_horizontal_scroll_target_plan(
    list_names_focused: bool,
    scroll_focus: Option<crate::tui::focus::MountScrollFocus>,
) -> WorkspaceListScrollTargetPlan {
    if list_names_focused {
        WorkspaceListScrollTargetPlan::ListNames
    } else if let Some(focus) = scroll_focus {
        WorkspaceListScrollTargetPlan::FocusedBlock(focus)
    } else {
        WorkspaceListScrollTargetPlan::None
    }
}

#[must_use]
pub const fn workspace_list_vertical_scroll_target_plan(
    scroll_focus: Option<crate::tui::focus::MountScrollFocus>,
) -> WorkspaceListScrollTargetPlan {
    if let Some(focus) = scroll_focus {
        WorkspaceListScrollTargetPlan::FocusedBlock(focus)
    } else {
        WorkspaceListScrollTargetPlan::None
    }
}

#[must_use]
pub fn workspace_list_move_selection_plan(
    selected: usize,
    row_count: usize,
    delta: isize,
) -> WorkspaceListSelectionPlan {
    let next = super::super::selection::WorkspaceSelection::move_index(selected, row_count, delta);
    WorkspaceListSelectionPlan {
        selected: next,
        changed: next != selected,
        clear_inline_role_picker: true,
        clear_inline_agent_picker: true,
        clear_inline_new_session_picker: true,
        clear_inline_account_picker: false,
        clear_launch_account_picker: false,
    }
}

#[must_use]
pub fn workspace_list_select_row_plan(
    current_selected: usize,
    selected: usize,
    row_count: usize,
) -> WorkspaceListSelectionPlan {
    let next = super::super::selection::WorkspaceSelection::move_to(selected, row_count);
    let changed = next != current_selected;
    WorkspaceListSelectionPlan {
        selected: next,
        changed,
        clear_inline_role_picker: true,
        clear_inline_agent_picker: changed,
        clear_inline_new_session_picker: changed,
        clear_inline_account_picker: changed,
        clear_launch_account_picker: changed,
    }
}
