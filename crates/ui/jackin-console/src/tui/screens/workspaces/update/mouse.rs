// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Workspace list mouse plans.

use super::workspace_list_hover_row_at_position;

use crossterm::event::MouseEvent;

use super::super::model::ManagerListRow;
use ratatui::layout::Rect;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceListMousePlan {
    StartDrag(crate::tui::split::DragState),
    UpdateSplit(u16),
    EndDrag,
    SelectRow(ManagerListRow),
    Continue,
}

// Mouse parity matrix row 14 carve-out: seam drag (hit slack,
// anchor-relative pct delta, 20-80% clamp, narrow-terminal gate) stays
// consumer code — upstream `ResizablePanelGroup` is exact-handle,
// absolute-position, ungated. Its adoption decision belongs to plan 011.
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
    if list_modal_open {
        return WorkspaceListMousePlan::Continue;
    }
    match mouse.kind {
        crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
            let seam_x = crate::tui::layout::split_seam_column(split_pct, term_size.width);
            if crate::tui::layout::near_seam(mouse.column, seam_x) {
                return WorkspaceListMousePlan::StartDrag(crate::tui::split::DragState {
                    anchor_pct: split_pct,
                    anchor_x: mouse.column,
                });
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
        crossterm::event::MouseEventKind::Drag(crossterm::event::MouseButton::Left) => drag_state
            .map_or(WorkspaceListMousePlan::Continue, |anchor| {
                WorkspaceListMousePlan::UpdateSplit(crate::tui::split::clamp_split(
                    crate::tui::layout::split_pct_from_drag(
                        anchor.anchor_pct,
                        anchor.anchor_x,
                        mouse.column,
                        term_size.width,
                    ),
                ))
            }),
        crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left) => {
            WorkspaceListMousePlan::EndDrag
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
