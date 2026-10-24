// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::workspace_list_hover_row_at_position;

use crossterm::event::MouseEvent;

use super::super::model::ManagerListRow;
use crate::tui::layout::MIN_DRAGGABLE_WIDTH;
use ratatui::layout::Rect;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceListMousePlan {
    StartDrag(crate::tui::split::DragState),
    UpdateSplit(u16),
    EndDrag,
    SelectRow(ManagerListRow),
    Continue,
}

#[must_use]
pub fn workspace_list_mouse_plan(
    mouse: MouseEvent,
    term_size: Rect,
    split_pct: u16,
    drag_state: Option<crate::tui::split::DragState>,
    list_modal_open: bool,
    visual_rows: &[Option<ManagerListRow>],
    selectable: impl FnMut(ManagerListRow) -> bool,
) -> WorkspaceListMousePlan {
    if matches!(
        mouse.kind,
        crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left)
    ) {
        return WorkspaceListMousePlan::EndDrag;
    }
    if list_modal_open {
        return WorkspaceListMousePlan::Continue;
    }
    match mouse.kind {
        crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
            let seam_x = crate::tui::layout::split_seam_column(split_pct, term_size.width);
            if crate::tui::layout::near_seam(mouse.column, seam_x) {
                return if term_size.width >= MIN_DRAGGABLE_WIDTH {
                    WorkspaceListMousePlan::StartDrag(crate::tui::split::DragState {
                        anchor_pct: split_pct,
                        anchor_x: mouse.column,
                    })
                } else {
                    WorkspaceListMousePlan::Continue
                };
            }
            workspace_list_hover_row_at_position(
                visual_rows,
                mouse.column,
                mouse.row,
                term_size,
                seam_x,
                selectable,
            )
            .map_or(WorkspaceListMousePlan::Continue, |row| {
                WorkspaceListMousePlan::SelectRow(row)
            })
        }
        crossterm::event::MouseEventKind::Drag(crossterm::event::MouseButton::Left)
            if term_size.width >= MIN_DRAGGABLE_WIDTH =>
        {
            drag_state.map_or(WorkspaceListMousePlan::Continue, |anchor| {
                WorkspaceListMousePlan::UpdateSplit(crate::tui::split::clamp_split(
                    crate::tui::layout::split_pct_from_drag(
                        anchor.anchor_pct,
                        anchor.anchor_x,
                        mouse.column,
                        term_size.width,
                    ),
                ))
            })
        }
        _ => WorkspaceListMousePlan::Continue,
    }
}

#[must_use]
pub fn workspace_list_clickable_at_position(
    column: u16,
    row: u16,
    term_size: Rect,
    split_pct: u16,
    list_modal_open: bool,
    visual_rows: &[Option<ManagerListRow>],
    selectable: impl FnMut(ManagerListRow) -> bool,
) -> bool {
    if list_modal_open {
        return false;
    }
    let seam_x = crate::tui::layout::split_seam_column(split_pct, term_size.width);
    if crate::tui::layout::near_seam(column, seam_x) {
        return false;
    }
    workspace_list_hover_row_at_position(visual_rows, column, row, term_size, seam_x, selectable)
        .is_some()
}
