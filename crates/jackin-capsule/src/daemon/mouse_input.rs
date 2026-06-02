//! Mouse, pointer, hover, and text-selection methods for the Multiplexer.

use crate::tui::components::branch_context_bar::branch_context_bar_hit;
use crate::tui::terminal::osc22_pointer_shape;
use crate::tui::view::encode_osc52_clipboard_write;

use super::*;

impl Multiplexer {
    pub(super) fn set_pointer_shape(&mut self, shape: PointerShape) {
        if !self.pointer_shapes_supported || self.pointer_shape == shape {
            return;
        }
        self.pointer_shape = shape;
        self.send_output(osc22_pointer_shape(shape));
    }

    pub(super) fn update_pointer_shape_for_mouse(&mut self, row: u16, col: u16, button: u8) {
        if !self.pointer_shapes_supported {
            return;
        }
        let shape = self.pointer_shape_at(row, col, button);
        self.set_pointer_shape(shape);
    }

    pub(super) fn update_hover_for_mouse(&mut self, row: u16, col: u16) -> Option<Vec<u8>> {
        let next = self.hover_target_at(row, col);
        if self.hover_target == next {
            return None;
        }
        self.hover_target = next;
        match hover_frame_plan(self.dialog_open()) {
            HoverFramePlan::DialogOverlay(reason) => Some(self.compose_full_frame(reason)),
            HoverFramePlan::ChromeHover => Some(self.compose_chrome_hover_frame()),
        }
    }

    /// Resolve the chrome target a hit at `(row, col)` (0-based)
    /// would land on, walking dialog → tab strip → menu → branch bar
    /// in priority order. Both `hover_target_at` and `pointer_shape_at`
    /// consume this so the priority ordering lives once.
    pub(super) fn chrome_hit_target_at(&self, row: u16, col: u16) -> Option<HoverTarget> {
        let row_1based = row + 1;
        let col_1based = col + 1;
        let dialog_copy_target = self.dialog_top().is_some_and(|dialog| {
            let github = self.github_context_view();
            dialog.clickable_at(
                row_1based,
                col_1based,
                self.term_rows,
                self.term_cols,
                Some(&github),
            )
        });
        let tab = (row_1based == 1)
            .then(|| self.status_bar.tab_at_col(col_1based))
            .flatten();
        let branch_hit = branch_context_bar_hit(
            row_1based,
            col_1based,
            self.term_rows,
            self.term_cols,
            self.context_bar_branch(),
            self.pull_request_context.as_deref(),
            self.pull_request_context_loading(),
            self.status_bar.instance_id_label(),
        );
        chrome_hover_target_for_state(ChromeHitState {
            dialog_copy_target,
            dialog_open: self.dialog_top().is_some(),
            tab,
            menu_hit: self.status_bar.hint_at(row_1based, col_1based),
            branch_hit,
        })
    }

    pub(super) fn hover_target_at(&self, row: u16, col: u16) -> Option<HoverTarget> {
        hover_target_for_state(HoverState {
            dragging: self.drag.is_some(),
            selecting: self.selection.is_some(),
            chrome_target: self.chrome_hit_target_at(row, col),
        })
    }

    pub(super) fn pointer_shape_at(&self, row: u16, col: u16, button: u8) -> PointerShape {
        pointer_shape_for_state(PointerShapeState {
            dragging: self.drag.is_some(),
            selecting: self.selection.is_some(),
            chrome_target: self.chrome_hit_target_at(row, col),
            dialog_open: self.dialog_top().is_some(),
            drag_start_orient: self.detect_drag_start(row, col).map(|drag| drag.orient),
            selection_start_available: self.detect_selection_start(row, col).is_some(),
            no_button_motion: button == SGR_NO_BUTTON_MOTION,
        })
    }

    /// Re-encode an SGR mouse event in the focused pane's local
    /// coordinate space and forward to its PTY. `press = true` emits
    /// the `M` final, `false` emits `m` (release). Forwarding is
    /// gated by the focused pane's requested mouse mode so shells and
    /// pre-mount agents never see raw mouse bytes leak out as
    /// command-line garbage, and press-only panes do not receive
    /// motion events from the multiplexer's always-on outer tracking.
    pub(super) fn forward_mouse_to_focused_pane_with_kind(
        &mut self,
        col: u16,
        row: u16,
        button: u8,
        press: bool,
    ) -> bool {
        let Some(focused) = self.active_focused_id() else {
            return false;
        };
        let Some(session) = self.sessions.get(&focused) else {
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
        session.send_input(&buf);
        true
    }

    /// Test whether the click at `(row, col)` lands inside the inner
    /// content area of a pane whose program never opted into a
    /// mouse protocol. If so, this is the start of a text selection
    /// (zellij-style "drag in shell pane → copy to clipboard").
    pub(super) fn detect_selection_start(&self, row: u16, col: u16) -> Option<SelectionState> {
        if row < STATUS_BAR_ROWS {
            return None;
        }
        let content_rect = content_rect(self.content_rows, self.term_cols);
        let (id, outer) = if let Some(zoom_id) = self.active_zoomed_id() {
            (zoom_id, content_rect)
        } else {
            let tab = self.tabs.get(self.active_tab)?;
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
        let session = self.sessions.get(&id)?;
        if session.mouse_enabled() {
            // Pane's program wants the mouse — defer to PTY forward.
            return None;
        }
        selection_start_for_inner_rect(id, inner, row, col)
    }

    /// Update the active selection's end-cell to the new motion
    /// position. Clamps to the inner pane rect so a drag that leaves
    /// the pane still produces a reasonable highlight.
    pub(super) fn selection_motion(&mut self, row: u16, col: u16) -> Option<Vec<u8>> {
        let sel = self.selection.as_mut()?;
        move_selection_end(sel, row, col);
        Some(self.compose_full_frame(selection_change_redraw_reason()))
    }

    /// Commit the active selection: extract the selected text from
    /// the source session's `vt100` grid, emit OSC 52 to the
    /// attached client (which the outer terminal turns into a
    /// real clipboard write), and clear the highlight.
    pub(super) fn finalize_selection(&mut self) -> Option<Vec<u8>> {
        let sel = self.selection.take()?;
        // Suppress single-cell selections: a click-to-focus with no
        // drag motion lands anchor==end and would otherwise OSC 52
        // whatever character sat under the cursor — a silent host-
        // clipboard overwrite on every focus click.
        if selection_was_dragged(&sel) && let Some(session) = self.sessions.get_mut(&sel.session_id) {
            let rows = session.render_snapshot(sel.inner.rows, sel.inner.cols);
            let text = selection_text(&rows, &sel);
            if !text.is_empty() && self.attached_out.is_some() {
                let bytes = encode_osc52_clipboard_write(&text);
                self.send_output(bytes);
            }
        }
        Some(self.compose_full_frame(selection_change_redraw_reason()))
    }
}

impl Multiplexer {
    pub(super) fn detect_drag_start(&self, row: u16, col: u16) -> Option<DragState> {
        if row < STATUS_BAR_ROWS || self.active_zoomed_id().is_some() {
            return None;
        }
        let content_rect = content_rect(self.content_rows, self.term_cols);
        let tab = self.tabs.get(self.active_tab)?;
        let (path, orient, rect) = tab.tree.border_at(content_rect, row, col)?;
        Some(DragState {
            tab_idx: self.active_tab,
            path,
            orient,
            rect,
        })
    }

    pub(super) fn drag_motion(&mut self, row: u16, col: u16) -> Option<Vec<u8>> {
        let drag = self.drag.clone()?;
        let new_ratio = drag_resize_ratio(drag.orient, drag.rect, row, col);
        let tab = self.tabs.get_mut(drag.tab_idx)?;
        if !tab.tree.set_ratio_at(&drag.path, new_ratio) {
            return None;
        }
        self.resize_panes();
        Some(self.compose_full_frame(drag_resize_redraw_reason()))
    }
}
