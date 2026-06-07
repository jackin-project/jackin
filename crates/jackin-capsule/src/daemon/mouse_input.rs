//! Mouse, pointer, hover, and text-selection methods for the Multiplexer.

use jackin_tui::components::HoverTracker;
use ratatui::layout::Rect;

use crate::tui::components::branch_context_bar::{
    BranchContextBarHit, ColRange, branch_context_bar_layout,
};
use crate::tui::terminal::osc22_pointer_shape;
use crate::tui::view::encode_osc52_clipboard_write;

use super::{
    ChromeHitState, DragState, HoverFramePlan, HoverState, HoverTarget, Multiplexer, PointerShape,
    PointerShapeState, SGR_NO_BUTTON_MOTION, STATUS_BAR_ROWS, SelectionState,
    chrome_hover_target_for_state, content_rect, drag_resize_ratio, drag_resize_redraw_reason,
    encode_mouse_for_protocol, hover_frame_plan, hover_target_for_state, local_mouse_position,
    mouse_event_encoding_for_mode, move_selection_end, pointer_shape_for_state,
    selection_change_redraw_reason, selection_start_for_inner_rect, selection_text,
    selection_was_dragged, status_change_redraw_reason,
};

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
        // The shared Debug info dialog brightens the hovered copyable row, so a
        // move between two copyable rows must redraw even though hover_target
        // stays DialogCopyTarget. Track the per-row hover separately.
        let (term_rows, term_cols) = (self.term_rows, self.term_cols);
        let row_hover_changed = self.dialog_top_mut().is_some_and(|dialog| {
            dialog.set_container_info_hover(row + 1, col + 1, term_rows, term_cols)
        });
        if self.hover_target == next && !row_hover_changed {
            return None;
        }
        self.hover_target = next;
        match hover_frame_plan(self.dialog_open()) {
            HoverFramePlan::DialogOverlay(reason) => Some(self.compose_full_redraw(reason)),
            HoverFramePlan::ChromeHover => {
                Some(self.compose_full_redraw(status_change_redraw_reason()))
            }
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
        if self.dialog_top().is_some() {
            return chrome_hover_target_for_state(ChromeHitState {
                dialog_copy_target,
                dialog_open: true,
                tab: None,
                menu_hit: false,
                branch_hit: None,
            });
        }

        let mut tracker = HoverTracker::new();
        self.register_chrome_hover_targets(&mut tracker);
        let target = tracker.hovered(col, row).copied();
        chrome_hover_target_for_state(ChromeHitState {
            dialog_copy_target,
            dialog_open: false,
            tab: target.and_then(|target| match target {
                HoverTarget::Tab(idx) => Some(idx),
                _ => None,
            }),
            menu_hit: target == Some(HoverTarget::Menu),
            branch_hit: target.and_then(|target| match target {
                HoverTarget::BranchContext => Some(BranchContextBarHit::Context),
                HoverTarget::Container => Some(BranchContextBarHit::Container),
                HoverTarget::DebugChip => Some(BranchContextBarHit::DebugChip),
                _ => None,
            }),
        })
    }

    fn register_chrome_hover_targets(&self, tracker: &mut HoverTracker<HoverTarget>) {
        for (idx, (start, end)) in self.status_bar.tab_regions.iter().copied().enumerate() {
            register_row0_range_1based(tracker, start, end, HoverTarget::Tab(idx));
        }
        if let Some((start, end)) = self.status_bar.hint_region {
            register_row0_range_1based(tracker, start, end, HoverTarget::Menu);
        }

        let Some(layout) = branch_context_bar_layout(
            self.term_rows,
            self.term_cols,
            self.context_bar_branch(),
            self.pull_request_context.as_deref(),
            self.pull_request_context_loading(),
            self.status_bar.instance_id_label(),
        ) else {
            return;
        };
        let row0 = self.term_rows.saturating_sub(1);
        register_col_range_1based(
            tracker,
            row0,
            layout.left_region,
            HoverTarget::BranchContext,
        );
        register_col_range_1based(
            tracker,
            row0,
            layout.container_region,
            HoverTarget::Container,
        );
        register_col_range_1based(
            tracker,
            row0,
            layout.debug_chip_region,
            HoverTarget::DebugChip,
        );
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
        crate::cdebug!(
            "selection start: session={id} press=({row},{col}) inner=({},{},{}x{})",
            inner.row,
            inner.col,
            inner.rows,
            inner.cols
        );
        selection_start_for_inner_rect(id, inner, row, col)
    }

    /// Update the active selection's end-cell to the new motion
    /// position. Clamps to the inner pane rect so a drag that leaves
    /// the pane still produces a reasonable highlight. Dragging above or below
    /// the pane nudges the selected session's scrollback view so long
    /// transcript selections can continue past the visible viewport.
    pub(super) fn selection_motion(&mut self, row: u16, col: u16) -> Option<Vec<u8>> {
        let (session_id, inner) = {
            let sel = self.selection.as_ref()?;
            (sel.session_id, sel.inner)
        };
        let scroll_delta = if row < inner.row {
            Some(1)
        } else if row >= inner.row.saturating_add(inner.rows) {
            Some(-1)
        } else {
            None
        };
        if let Some(delta) = scroll_delta
            && let Some(session) = self.sessions.get_mut(&session_id)
        {
            session.scroll_by(delta);
        }
        let sel = self.selection.as_mut()?;
        move_selection_end(sel, row, col);
        crate::cdebug!(
            "selection motion: motion=({row},{col}) anchor=({},{}) end=({},{}) inner=({},{},{}x{})",
            sel.anchor_row,
            sel.anchor_col,
            sel.end_row,
            sel.end_col,
            sel.inner.row,
            sel.inner.col,
            sel.inner.rows,
            sel.inner.cols
        );
        Some(self.compose_full_redraw(selection_change_redraw_reason()))
    }

    /// Commit the active selection: extract the selected text from the source
    /// session's grid and emit OSC 52 to the attached client (which the outer
    /// terminal turns into a real clipboard write). Dragged selections remain
    /// highlighted after copy until the next click or typed input clears them.
    pub(super) fn finalize_selection(&mut self) -> Option<Vec<u8>> {
        let sel = self.selection?;
        // Suppress single-cell selections: a click-to-focus with no
        // drag motion lands anchor==end and would otherwise OSC 52
        // whatever character sat under the cursor — a silent host-
        // clipboard overwrite on every focus click.
        if selection_was_dragged(&sel) {
            let mut copied = false;
            if let Some(session) = self.sessions.get_mut(&sel.session_id) {
                let rows = session.render_snapshot(sel.inner.rows, sel.inner.cols);
                let text = selection_text(&rows, &sel);
                if !text.is_empty() && self.attached_out.is_some() {
                    let bytes = encode_osc52_clipboard_write(&text);
                    self.send_output(bytes);
                    copied = true;
                }
            }
            self.selection_copied = copied;
        } else {
            self.selection = None;
            self.selection_copied = false;
        }
        Some(self.compose_full_redraw(selection_change_redraw_reason()))
    }
}

fn register_row0_range_1based(
    tracker: &mut HoverTracker<HoverTarget>,
    start: u16,
    end: u16,
    key: HoverTarget,
) {
    if let Some(range) = ColRange::new(start, end) {
        register_col_range_1based(tracker, 0, Some(range), key);
    }
}

fn register_col_range_1based(
    tracker: &mut HoverTracker<HoverTarget>,
    row0: u16,
    range: Option<ColRange>,
    key: HoverTarget,
) {
    let Some(range) = range else {
        return;
    };
    let x = range.start.saturating_sub(1);
    let width = range.end.saturating_sub(range.start);
    if width > 0 {
        tracker.register(Rect::new(x, row0, width, 1), key);
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
        Some(self.compose_full_redraw(drag_resize_redraw_reason()))
    }
}
