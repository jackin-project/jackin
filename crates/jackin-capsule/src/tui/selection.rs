//! Mouse text-selection state and rendering for the capsule multiplexer.
//!
//! `SelectionState` lives in the multiplexer's optional selection field.
//! The helper functions here extract text and paint the inverse-video
//! highlight overlay that the compositor writes on top of pane bodies.

use crate::tui::layout::{Rect, local_mouse_position};
use crate::tui::render::RowSnapshot;

/// Active mouse text selection on a pane.
///
/// Rows are absolute content coordinates: retained scrollback rows
/// oldest-first, followed by the current live screen rows. Columns remain pane
/// cell coordinates. This lets a copied selection persist across scrollback
/// viewport movement instead of being tied to a transient screen row.
#[derive(Clone, Copy)]
pub(crate) struct SelectionState {
    pub(crate) session_id: u64,
    /// Pane's inner content rectangle at selection-start time. Stays
    /// stable through the drag (a resize / reflow cancels the
    /// selection in the same places `DragState` is cancelled).
    pub(crate) inner: Rect,
    /// 0-based content row captured at press time. Stays put during the drag.
    pub(crate) anchor_row: usize,
    pub(crate) anchor_col: u16,
    /// Latest content row the operator's cursor reached. Updated on every
    /// motion event.
    pub(crate) end_row: usize,
    pub(crate) end_col: u16,
}

#[derive(Clone, Copy)]
pub(crate) struct VisibleSelection {
    pub(crate) inner: Rect,
    pub(crate) start_row: u16,
    pub(crate) start_col: u16,
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
    scrollback_filled: usize,
    scrollback_offset: usize,
) -> Option<SelectionState> {
    let (anchor_row, anchor_col) = local_mouse_position(inner, row, col)?;
    let anchor_row =
        visible_row_to_content_row(scrollback_filled, scrollback_offset, inner.rows, anchor_row);
    Some(SelectionState {
        session_id,
        inner,
        anchor_row,
        anchor_col,
        end_row: anchor_row,
        end_col: anchor_col,
    })
}

/// Extract the selected text from the pane's full content snapshot.
///
/// Uses `canonical_selection` ordering and matches the bounds used by
/// `paint_selection_highlight` so copied text and highlighted cells agree.
pub(crate) fn selection_text(rows: &[RowSnapshot], sel: &SelectionState) -> String {
    let (start_row, start_col, end_row, end_col) = canonical_selection(sel);
    // Must match `paint_selection_highlight`'s bound — without this
    // the painted highlight and the copied text disagree mid-resize.
    let cols_for_full_row = sel.inner.cols.saturating_sub(1);
    let Some(max_snapshot_row) = rows.len().checked_sub(1) else {
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
            .get(r)
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
pub(crate) fn canonical_selection(sel: &SelectionState) -> (usize, u16, usize, u16) {
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
/// update the content-coordinate selection end-cell.
pub(crate) fn move_selection_end(
    sel: &mut SelectionState,
    row: u16,
    col: u16,
    scrollback_filled: usize,
    scrollback_offset: usize,
) {
    let inner = sel.inner;
    let clamped_row = row.clamp(inner.row, inner.row + inner.rows.saturating_sub(1));
    let clamped_col = col.clamp(inner.col, inner.col + inner.cols.saturating_sub(1));
    sel.end_row = visible_row_to_content_row(
        scrollback_filled,
        scrollback_offset,
        inner.rows,
        clamped_row - inner.row,
    );
    sel.end_col = clamped_col - inner.col;
}

pub(crate) fn visible_selection(
    sel: &SelectionState,
    scrollback_filled: usize,
    scrollback_offset: usize,
) -> Option<VisibleSelection> {
    let (start_row, start_col, end_row, end_col) = canonical_selection(sel);
    let start_visible = content_row_to_visible_row(
        scrollback_filled,
        scrollback_offset,
        sel.inner.rows,
        start_row,
    );
    let end_visible = content_row_to_visible_row(
        scrollback_filled,
        scrollback_offset,
        sel.inner.rows,
        end_row,
    );
    match (start_visible, end_visible) {
        (Some(start_visible_row), Some(end_visible_row)) => Some(VisibleSelection {
            inner: sel.inner,
            start_row: start_visible_row,
            start_col,
            end_row: end_visible_row,
            end_col,
        }),
        (Some(start_visible_row), None) if end_row >= start_row => Some(VisibleSelection {
            inner: sel.inner,
            start_row: start_visible_row,
            start_col,
            end_row: sel.inner.rows.saturating_sub(1),
            end_col: sel.inner.cols.saturating_sub(1),
        }),
        (None, Some(end_visible_row)) if start_row <= end_row => Some(VisibleSelection {
            inner: sel.inner,
            start_row: 0,
            start_col: 0,
            end_row: end_visible_row,
            end_col,
        }),
        (None, None) => {
            let viewport_start = viewport_start_content_row(scrollback_filled, scrollback_offset);
            let viewport_end = viewport_start.saturating_add(sel.inner.rows as usize);
            if start_row < viewport_end && end_row >= viewport_start {
                Some(VisibleSelection {
                    inner: sel.inner,
                    start_row: 0,
                    start_col: 0,
                    end_row: sel.inner.rows.saturating_sub(1),
                    end_col: sel.inner.cols.saturating_sub(1),
                })
            } else {
                None
            }
        }
        _ => None,
    }
}

fn viewport_prefix_rows(
    scrollback_filled: usize,
    scrollback_offset: usize,
    viewport_rows: u16,
) -> usize {
    if scrollback_offset == 0 || scrollback_filled == 0 {
        0
    } else {
        scrollback_offset
            .min(scrollback_filled)
            .min(viewport_rows as usize)
    }
}

fn viewport_start_content_row(scrollback_filled: usize, scrollback_offset: usize) -> usize {
    scrollback_filled.saturating_sub(scrollback_offset.min(scrollback_filled))
}

fn visible_row_to_content_row(
    scrollback_filled: usize,
    scrollback_offset: usize,
    viewport_rows: u16,
    visible_row: u16,
) -> usize {
    let prefix = viewport_prefix_rows(scrollback_filled, scrollback_offset, viewport_rows);
    let visible_row = visible_row as usize;
    if visible_row < prefix {
        viewport_start_content_row(scrollback_filled, scrollback_offset).saturating_add(visible_row)
    } else {
        scrollback_filled.saturating_add(visible_row.saturating_sub(prefix))
    }
}

fn content_row_to_visible_row(
    scrollback_filled: usize,
    scrollback_offset: usize,
    viewport_rows: u16,
    content_row: usize,
) -> Option<u16> {
    let prefix = viewport_prefix_rows(scrollback_filled, scrollback_offset, viewport_rows);
    if content_row < scrollback_filled {
        let start = viewport_start_content_row(scrollback_filled, scrollback_offset);
        let end = start.saturating_add(prefix).min(scrollback_filled);
        if (start..end).contains(&content_row) {
            u16::try_from(content_row.saturating_sub(start)).ok()
        } else {
            None
        }
    } else {
        let live_row = content_row.saturating_sub(scrollback_filled);
        let visible_row = prefix.saturating_add(live_row);
        (visible_row < viewport_rows as usize)
            .then(|| u16::try_from(visible_row).ok())
            .flatten()
    }
}

#[cfg(test)]
mod tests;
