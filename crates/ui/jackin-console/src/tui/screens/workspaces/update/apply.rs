// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Hover, selection, and scroll application.

use super::workspace_unclamped_scroll_plan;

use super::super::model::{ManagerHoverTarget, ManagerListRow};
use ratatui::layout::Rect;

#[must_use]
pub fn workspace_list_hover_row_at_position(
    visual_rows: &[Option<ManagerListRow>],
    col: u16,
    row: u16,
    term_size: Rect,
    seam_x: u16,
    mut selectable: impl FnMut(ManagerListRow) -> bool,
) -> Option<ManagerListRow> {
    if crate::tui::layout::near_seam(col, seam_x) {
        return None;
    }
    let content_top = crate::tui::layout::LIST_HEADER_HEIGHT.saturating_add(1);
    let body_end = term_size
        .height
        .saturating_sub(crate::tui::layout::LIST_FOOTER_HEIGHT);
    let content_bottom = body_end.saturating_sub(1);
    if content_top >= content_bottom {
        return None;
    }

    let mut regions = Vec::new();
    for (visual_idx, row_value) in visual_rows.iter().enumerate() {
        let Some(row_value) = row_value else {
            continue;
        };
        if !selectable(*row_value) {
            continue;
        }
        let Ok(offset) = u16::try_from(visual_idx) else {
            break;
        };
        let y = content_top.saturating_add(offset);
        if y >= content_bottom {
            break;
        }
        regions.push(termrock::interaction::HitRegion {
            area: Rect {
                x: 1,
                y,
                width: seam_x.saturating_sub(1),
                height: 1,
            },
            id: *row_value,
        });
    }
    let position = ratatui::layout::Position::new(col, row);
    regions
        .iter()
        .find(|region| region.area.contains(position))
        .map(|region| region.id)
}

#[must_use]
pub fn selected_index(selected: usize, row_count: usize) -> usize {
    crate::tui::focus::selected_index(selected, row_count)
}

#[expect(
    clippy::struct_excessive_bools,
    reason = "Five orthogonal inline-picker clear flags on the list-selection \
              plan (role / agent / new_session / provider / launch_account) — \
              each tracks an independent clear-mutation the plan applies to the \
              state. Named-field reads match the per-trait-method dispatch this \
              plan parallelizes."
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceListSelectionPlan {
    pub selected: usize,
    pub changed: bool,
    pub clear_inline_role_picker: bool,
    pub clear_inline_agent_picker: bool,
    pub clear_inline_new_session_picker: bool,
    pub clear_inline_account_picker: bool,
    pub clear_launch_account_picker: bool,
}

pub trait WorkspaceListSelectionState {
    fn clear_inline_role_picker(&mut self);
    fn clear_inline_agent_picker(&mut self);
    fn clear_inline_new_session_picker(&mut self);
    fn clear_inline_account_picker(&mut self);
    fn clear_launch_account_picker(&mut self);
    fn reset_list_scroll(&mut self);
    fn set_selected(&mut self, selected: usize);
}

pub fn apply_workspace_list_selection_plan(
    state: &mut impl WorkspaceListSelectionState,
    plan: WorkspaceListSelectionPlan,
) {
    if plan.clear_inline_role_picker {
        state.clear_inline_role_picker();
    }
    if plan.clear_inline_agent_picker {
        state.clear_inline_agent_picker();
    }
    if plan.clear_inline_new_session_picker {
        state.clear_inline_new_session_picker();
    }
    if plan.clear_inline_account_picker {
        state.clear_inline_account_picker();
    }
    if plan.clear_launch_account_picker {
        state.clear_launch_account_picker();
    }
    if plan.changed {
        state.reset_list_scroll();
        state.set_selected(plan.selected);
    }
}

pub trait WorkspaceListHoverState {
    fn set_workspace_list_hover_target(&mut self, target: Option<ManagerHoverTarget>);
}

pub fn apply_workspace_list_hover_target(
    state: &mut impl WorkspaceListHoverState,
    target: Option<ManagerHoverTarget>,
) {
    state.set_workspace_list_hover_target(target);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceListScrollFocusPlan {
    pub list_names_focused: bool,
    pub scroll_focus: Option<crate::tui::focus::MountScrollFocus>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceListScrollTargetPlan {
    ListNames,
    FocusedBlock(crate::tui::focus::MountScrollFocus),
    None,
}

pub trait WorkspaceListScrollState {
    fn list_names_scroll_x(&self) -> u16;
    fn set_list_names_scroll_x(&mut self, value: u16);
    fn block_scroll_x(&self, focus: crate::tui::focus::MountScrollFocus) -> u16;
    fn set_block_scroll_x(&mut self, focus: crate::tui::focus::MountScrollFocus, value: u16);
    fn block_scroll_y(&self, focus: crate::tui::focus::MountScrollFocus) -> u16;
    fn set_block_scroll_y(&mut self, focus: crate::tui::focus::MountScrollFocus, value: u16);
}

pub fn apply_workspace_list_horizontal_scroll_plan(
    state: &mut impl WorkspaceListScrollState,
    plan: WorkspaceListScrollTargetPlan,
    delta: i16,
) {
    match plan {
        WorkspaceListScrollTargetPlan::ListNames => {
            state.set_list_names_scroll_x(workspace_unclamped_scroll_plan(
                state.list_names_scroll_x(),
                delta,
            ));
        }
        WorkspaceListScrollTargetPlan::FocusedBlock(focus) => {
            state.set_block_scroll_x(
                focus,
                workspace_unclamped_scroll_plan(state.block_scroll_x(focus), delta),
            );
        }
        WorkspaceListScrollTargetPlan::None => {}
    }
}

pub fn apply_workspace_list_vertical_scroll_plan(
    state: &mut impl WorkspaceListScrollState,
    plan: WorkspaceListScrollTargetPlan,
    delta: i16,
) {
    if let WorkspaceListScrollTargetPlan::FocusedBlock(focus) = plan {
        state.set_block_scroll_y(
            focus,
            workspace_unclamped_scroll_plan(state.block_scroll_y(focus), delta),
        );
    }
}
