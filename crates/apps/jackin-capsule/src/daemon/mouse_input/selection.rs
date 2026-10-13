// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Mouse press forwarding and text-selection lifecycle.

use crate::tui::pane_snapshot::RowSnapshot;

use crate::tui::view::encode_osc52_clipboard_write;

use super::super::{
    Instant, Multiplexer, STATUS_BAR_ROWS, SelectionState, content_rect, encode_mouse_for_protocol,
    local_mouse_position, mouse_event_encoding_for_mode, move_selection_end,
    selection_change_redraw_reason, selection_start_for_inner_rect, selection_text,
    selection_was_dragged, wheel_scrollback_redraw_reason,
};
use super::{PanePress, is_double_click};
use crate::tui::selection::word_bounds_in_row;

impl Multiplexer {
    /// Re-encode an SGR mouse event in the focused pane's local
    /// coordinate space and forward to its PTY. `press = true` emits
    /// the `M` final, `false` emits `m` (release). Forwarding is
    /// gated by the focused pane's requested mouse mode so shells and
    /// pre-mount agents never see raw mouse bytes leak out as
    /// command-line garbage, and press-only panes do not receive
    /// motion events from the multiplexer's always-on outer tracking.
    pub(crate) fn forward_mouse_to_focused_pane_with_kind(
        &mut self,
        col: u16,
        row: u16,
        button: u8,
        press: bool,
    ) -> bool {
        let Some(focused) = self.active_focused_id() else {
            return false;
        };
        let Some(session) = self.session_supervisor.sessions.get(focused) else {
            return false;
        };
        let Some(encoding) = mouse_event_encoding_for_mode(
            session.mouse_protocol_mode(),
            session.mouse_protocol_encoding(),
            button,
            press,
        ) else {
            return false;
        };
        let Some(inner) = self.active_focused_inner_rect() else {
            return false;
        };
        let Some((local_row, local_col)) = local_mouse_position(inner, row, col) else {
            return false;
        };
        let Some(buf) =
            encode_mouse_for_protocol(button, local_col + 1, local_row + 1, press, encoding)
        else {
            return false;
        };
        session.send_input(&buf)
    }

    /// Click-to-jump on the focused pane's scrollback scrollbar. Hits only
    /// the right-border track of the focused pane (a click on an unfocused
    /// pane's border stays a focus gesture), and only when that pane retains
    /// scrollback and is not an alt-screen app — the same gates that decide
    /// whether the scrollbar is painted at all. Shared splitter borders never
    /// reach here: the caller checks `detect_drag_start` first, so drag-resize
    /// keeps priority on borders two panes share.
    pub(crate) fn scrollbar_jump_at(&mut self, row: u16, col: u16) -> bool {
        let Some(focused) = self.active_focused_id() else {
            return false;
        };
        let Some(pane) = self
            .visible_panes()
            .into_iter()
            .find(|pane| pane.id == focused)
        else {
            return false;
        };
        if pane.outer.cols == 0 || pane.outer.rows < 3 {
            return false;
        }
        let track_col = pane
            .outer
            .col
            .saturating_add(pane.outer.cols)
            .saturating_sub(1);
        let track_start = pane.outer.row + 1;
        let interior_rows = usize::from(pane.outer.rows - 2);
        if col != track_col || row < track_start || usize::from(row - track_start) >= interior_rows
        {
            return false;
        }
        let Some(session) = self.session_supervisor.sessions.get_mut(focused) else {
            return false;
        };
        if session.shadow_grid.alternate_screen() {
            return false;
        }
        let filled = session.scrollback_filled();
        if filled == 0 {
            return false;
        }
        // Same content-length convention as the painted scrollbar
        // (`apply_pane_scrollbar`): scrollback rows plus the visible interior.
        let content_len = filled.saturating_add(interior_rows);
        let top_offset = termrock::scroll::scrollbar_offset_for_track_position(
            content_len,
            interior_rows,
            interior_rows,
            usize::from(row - track_start),
        );
        // The shared component speaks top-relative offsets; the pane scroll
        // model is tail-relative (0 = live). Max top offset equals `filled`,
        // so the conversion is a plain inversion.
        let tail_offset = filled.saturating_sub(usize::from(top_offset));
        let moved = session.set_scrollback_offset(tail_offset);
        if moved {
            self.invalidate(wheel_scrollback_redraw_reason());
        }
        moved
    }

    /// Test whether the click at `(row, col)` lands inside the inner
    /// content area of a pane whose program never opted into a
    /// mouse protocol. If so, this is the start of a text selection
    /// (zellij-style "drag in shell pane → copy to clipboard").
    pub(crate) fn detect_selection_start(&self, row: u16, col: u16) -> Option<SelectionState> {
        if row < STATUS_BAR_ROWS {
            return None;
        }
        let content_rect = content_rect(self.render.content_rows, self.render.term_cols);
        let (id, outer) = if let Some(zoom_id) = self.active_zoomed_id() {
            (zoom_id, content_rect)
        } else {
            let tab = self
                .session_supervisor
                .tabs
                .get(self.session_supervisor.active_tab)?;
            tab.tree.leaves(content_rect).into_iter().find(|(_, r)| {
                row >= r.row && row < r.row + r.rows && col >= r.col && col < r.col + r.cols
            })?
        };
        let inner = outer.shrink(1);
        if row < inner.row
            || row >= inner.row + inner.rows
            || col < inner.col
            || col >= inner.col + inner.cols
        {
            return None;
        }
        let session = self.session_supervisor.sessions.get(id)?;
        if session.mouse_enabled() {
            // Pane's program wants the mouse — defer to PTY forward.
            return None;
        }
        let scrollback_filled = session.scrollback_filled();
        let scrollback_offset = session.scrollback_offset();
        selection_start_for_inner_rect(id, inner, row, col, scrollback_filled, scrollback_offset)
    }

    /// Update the active selection's end-cell to the new motion
    /// position. Clamps to the inner pane rect so a drag that leaves
    /// the pane still produces a reasonable highlight. Dragging above or below
    /// the pane nudges the selected session's scrollback view so long
    /// transcript selections can continue past the visible viewport.
    pub(crate) fn selection_motion(&mut self, row: u16, col: u16) {
        let Some((session_id, inner)) = self
            .clipboard
            .selection
            .as_ref()
            .map(|sel| (sel.session_id, sel.inner))
        else {
            return;
        };
        let scroll_delta = if row < inner.row {
            Some(1)
        } else if row >= inner.row.saturating_add(inner.rows) {
            Some(-1)
        } else {
            None
        };
        let (scrollback_filled, scrollback_offset) =
            if let Some(session) = self.session_supervisor.sessions.get_mut(session_id) {
                if let Some(delta) = scroll_delta {
                    session.scroll_by(delta);
                }
                (session.scrollback_filled(), session.scrollback_offset())
            } else {
                return;
            };
        let Some(sel) = self.clipboard.selection.as_mut() else {
            return;
        };
        move_selection_end(sel, row, col, scrollback_filled, scrollback_offset);
        // The selection changed shape, so the clipboard no longer matches
        // it; release must copy again (extends a word-click selection too).
        self.clipboard.selection_copied = false;
        self.invalidate(selection_change_redraw_reason());
    }

    /// Promote a press-time selection candidate only after the pointer really
    /// moves away from the anchor cell. Plain clicks remain normal focus/click
    /// gestures and never flash selection chrome or arm clipboard copy.
    pub(crate) fn pending_selection_motion(&mut self, row: u16, col: u16) {
        // The press turned into a drag — it must not pair as the first half
        // of a double-click with the click that later clears its copy.
        self.clipboard.last_pane_press = None;
        self.clipboard.selection = self.clipboard.pending_selection.take();
        self.selection_motion(row, col);
        if !self
            .clipboard
            .selection
            .as_ref()
            .is_some_and(selection_was_dragged)
        {
            self.clipboard.selection = None;
        }
    }

    /// Commit the active selection: extract the selected text from the source
    /// session's grid and emit OSC 52 to the attached client (which the outer
    /// terminal turns into a real clipboard write). Dragged selections remain
    /// highlighted after copy until the next click or typed input clears them.
    pub(crate) fn finalize_selection(&mut self) {
        let Some(sel) = self.clipboard.selection else {
            return;
        };
        // A word-click selection was already copied at press time; the
        // release that follows must not write the clipboard again.
        if self.clipboard.selection_copied {
            return;
        }
        // Suppress single-cell selections: a click-to-focus with no
        // drag motion lands anchor==end and would otherwise OSC 52
        // whatever character sat under the cursor — a silent host-
        // clipboard overwrite on every focus click.
        if selection_was_dragged(&sel) {
            self.copy_selection_to_clipboard(&sel);
        } else {
            self.clipboard.selection = None;
            self.clipboard.selection_copied = false;
            self.clipboard.selection_copy_feedback_deadline = None;
        }
        self.invalidate(selection_change_redraw_reason());
    }

    /// Snapshot the selection's session and copy through
    /// `copy_selection_rows`. Used by drag-release finalize, which holds no
    /// snapshot of its own.
    fn copy_selection_to_clipboard(&mut self, sel: &SelectionState) {
        let rows = self
            .session_supervisor
            .sessions
            .get(sel.session_id)
            .map(|session| session.render_content_snapshot(sel.inner.cols))
            .unwrap_or_default();
        self.copy_selection_rows(sel, &rows);
    }

    /// OSC 52 the selection's text to the attached client and arm the
    /// "copied" toast — the shared copy body for drag-release finalize and
    /// double-click word selection (which resolves word bounds from the
    /// same rows it copies; the snapshot is a full-grid copy worth taking
    /// once).
    fn copy_selection_rows(&mut self, sel: &SelectionState, rows: &[RowSnapshot]) {
        let text = selection_text(rows, sel);
        let copied = !text.is_empty() && self.client_registry.client.is_attached();
        if copied {
            let bytes = encode_osc52_clipboard_write(&text);
            self.send_out_of_band(bytes);
        }
        self.clipboard.selection_copied = copied;
        self.clipboard.selection_copy_feedback_deadline =
            copied.then_some(Instant::now() + crate::tui::update::DIALOG_COPY_FEEDBACK_DURATION);
    }

    /// Classify a primary press on a pane cell as single or double click.
    /// A double-click selects the word under the cursor and copies it
    /// immediately; the highlight stays until the next click or keystroke,
    /// same as a dragged selection. Returns `true` when the press was
    /// consumed as a word selection.
    pub(crate) fn register_pane_press(&mut self, candidate: &SelectionState) -> bool {
        let press = PanePress {
            session_id: candidate.session_id,
            content_row: candidate.anchor_row,
            col: candidate.anchor_col,
            at: Instant::now(),
        };
        let is_double = self
            .clipboard
            .last_pane_press
            .is_some_and(|previous| is_double_click(&previous, &press));
        if !is_double {
            self.clipboard.last_pane_press = Some(press);
            return false;
        }
        // A third quick press starts a fresh cycle instead of re-selecting.
        self.clipboard.last_pane_press = None;
        self.select_word_at(candidate)
    }

    /// Select the word under `candidate`'s anchor cell and copy it. The
    /// word's display-column bounds come from `word_bounds_in_row` over the
    /// session's content snapshot.
    fn select_word_at(&mut self, candidate: &SelectionState) -> bool {
        let Some(session) = self.session_supervisor.sessions.get(candidate.session_id) else {
            return false;
        };
        let rows = session.render_content_snapshot(candidate.inner.cols);
        let Some((start_col, end_col)) = rows
            .get(candidate.anchor_row)
            .and_then(|row| word_bounds_in_row(row, candidate.anchor_col))
        else {
            return false;
        };
        let mut sel = *candidate;
        sel.anchor_col = start_col;
        sel.end_col = end_col;
        self.clipboard.selection = Some(sel);
        self.clipboard.pending_selection = None;
        self.copy_selection_rows(&sel, &rows);
        self.invalidate(selection_change_redraw_reason());
        true
    }
}
