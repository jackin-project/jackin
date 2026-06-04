//! Mouse text-selection state and rendering for the capsule multiplexer.
//!
//! `SelectionState` lives in the multiplexer's optional selection field.
//! The helper functions here extract text and paint the inverse-video
//! highlight overlay that the compositor writes on top of pane bodies.

use crate::tui::layout::{Rect, local_mouse_position};
use crate::tui::render::RowSnapshot;

/// Active mouse text selection on a pane. Held until the operator
/// releases the mouse button or the pane resizes.
#[derive(Clone, Copy)]
pub(crate) struct SelectionState {
    pub(crate) session_id: u64,
    /// Pane's inner content rectangle at selection-start time. Stays
    /// stable through the drag (a resize / reflow cancels the
    /// selection in the same places `DragState` is cancelled).
    pub(crate) inner: Rect,
    /// 0-based grid coordinates relative to the pane's inner area,
    /// captured at press time. Stays put during the drag.
    pub(crate) anchor_row: u16,
    pub(crate) anchor_col: u16,
    /// Latest grid coordinate the operator's cursor reached. Updated
    /// on every motion event.
    pub(crate) end_row: u16,
    pub(crate) end_col: u16,
}

/// Build the initial visible selection state for a click inside a
/// pane's inner content rect.
pub(crate) fn selection_start_for_inner_rect(
    session_id: u64,
    inner: Rect,
    row: u16,
    col: u16,
) -> Option<SelectionState> {
    let (anchor_row, anchor_col) = local_mouse_position(inner, row, col)?;
    Some(SelectionState {
        session_id,
        inner,
        anchor_row,
        anchor_col,
        end_row: anchor_row,
        end_col: anchor_col,
    })
}

/// Extract the selected text from the pane's grid screen.
///
/// Uses `canonical_selection` ordering and matches the bounds used by
/// `paint_selection_highlight` so copied text and highlighted cells agree.
pub(crate) fn selection_text(rows: &[RowSnapshot], sel: &SelectionState) -> String {
    let (start_row, start_col, end_row, end_col) = canonical_selection(sel);
    // Must match `paint_selection_highlight`'s bound — without this
    // the painted highlight and the copied text disagree mid-resize.
    let cols_for_full_row = sel.inner.cols.saturating_sub(1);
    let Some(max_snapshot_row) = rows
        .len()
        .checked_sub(1)
        .and_then(|row| u16::try_from(row).ok())
    else {
        return String::new();
    };
    let max_row = max_snapshot_row.min(end_row);
    if start_row > max_row {
        return String::new();
    }
    let mut out = String::new();
    for r in start_row..=max_row {
        let from_col = if r == start_row { start_col } else { 0 };
        let to_col = if r == end_row {
            end_col
        } else {
            cols_for_full_row
        };
        let row_text = rows
            .get(usize::from(r))
            .map(|row| row.text_range(from_col, to_col))
            .unwrap_or_default();
        out.push_str(row_text.trim_end());
        if r != max_row {
            out.push('\n');
        }
    }
    out
}

/// Normalise a selection into `(start_row, start_col, end_row, end_col)`
/// in top-left → bottom-right order, regardless of which direction the
/// operator dragged.
pub(crate) fn canonical_selection(sel: &SelectionState) -> (u16, u16, u16, u16) {
    if (sel.anchor_row, sel.anchor_col) <= (sel.end_row, sel.end_col) {
        (sel.anchor_row, sel.anchor_col, sel.end_row, sel.end_col)
    } else {
        (sel.end_row, sel.end_col, sel.anchor_row, sel.anchor_col)
    }
}

/// True only after the pointer moved away from the anchor cell.
pub(crate) fn selection_was_dragged(sel: &SelectionState) -> bool {
    sel.anchor_row != sel.end_row || sel.anchor_col != sel.end_col
}

/// Clamp a pointer motion event to the selected pane's inner rect and
/// update the visible selection end-cell.
pub(crate) fn move_selection_end(sel: &mut SelectionState, row: u16, col: u16) {
    let inner = sel.inner;
    let clamped_row = row.clamp(inner.row, inner.row + inner.rows.saturating_sub(1));
    let clamped_col = col.clamp(inner.col, inner.col + inner.cols.saturating_sub(1));
    sel.end_row = clamped_row - inner.row;
    sel.end_col = clamped_col - inner.col;
}

#[cfg(test)]
mod tests;
