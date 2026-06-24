//! Mouse, pointer, hover, and text-selection methods for the Multiplexer.

use jackin_tui::components::HoverTracker;
use ratatui::layout::Rect;

use crate::tui::components::branch_context_bar::{
    BranchContextBarHit, ColRange, branch_context_bar_layout, debug_run_id_label,
};
use crate::tui::terminal::osc22_pointer_shape;
use crate::tui::view::encode_osc52_clipboard_write;

use super::{
    ChromeHitState, DragState, HoverFramePlan, HoverState, HoverTarget, Instant, Multiplexer,
    PointerShape, PointerShapeState, SGR_NO_BUTTON_MOTION, STATUS_BAR_ROWS, SelectionState,
    chrome_hover_target_for_state, content_rect, drag_resize_ratio, drag_resize_redraw_reason,
    encode_mouse_for_protocol, hover_frame_plan, hover_target_for_state, local_mouse_position,
    mouse_event_encoding_for_mode, move_selection_end, pointer_shape_for_state,
    selection_change_redraw_reason, selection_start_for_inner_rect, selection_text,
    selection_was_dragged, status_change_redraw_reason, wheel_scrollback_redraw_reason,
};
use crate::tui::selection::word_bounds_in_row;

/// Two presses on the same pane cell within this window form a double-click.
/// 500 ms matches the common desktop default.
const DOUBLE_CLICK_WINDOW: std::time::Duration = std::time::Duration::from_millis(500);

/// A primary press on a pane cell, in content coordinates, stamped for
/// double-click classification.
#[derive(Clone, Copy, Debug)]
pub(super) struct PanePress {
    pub(super) session_id: u64,
    pub(super) content_row: usize,
    pub(super) col: u16,
    pub(super) at: Instant,
}

impl Multiplexer {
    pub(super) fn set_pointer_shape(&mut self, shape: PointerShape) {
        if !self.pointer_shapes_supported || self.pointer_shape == shape {
            return;
        }
        self.pointer_shape = shape;
        self.send_out_of_band(osc22_pointer_shape(shape));
    }

    pub(super) fn update_pointer_shape_for_mouse(&mut self, row: u16, col: u16, button: u8) {
        if !self.pointer_shapes_supported {
            return;
        }
        let shape = self.pointer_shape_at(row, col, button);
        self.set_pointer_shape(shape);
    }

    pub(super) fn update_hover_for_mouse(&mut self, row: u16, col: u16) {
        let next = self.hover_target_at(row, col);
        // The shared Debug info dialog brightens the hovered copyable row, so a
        // move between two copyable rows must redraw even though hover_target
        // stays DialogCopyTarget. Track the per-row hover separately.
        let (term_rows, term_cols) = (self.term_rows, self.term_cols);
        let row_hover_changed = self.dialog_top_mut().is_some_and(|dialog| {
            let row = row + 1;
            let col = col + 1;
            dialog.set_container_info_hover(row, col, term_rows, term_cols)
                || dialog.set_usage_tab_hover(row, col, term_rows, term_cols)
        });
        if self.hover_target == next && !row_hover_changed {
            return;
        }
        self.hover_target = next;
        match hover_frame_plan(self.dialog_open()) {
            HoverFramePlan::DialogOverlay(reason) => self.invalidate(reason),
            HoverFramePlan::ChromeHover => self.invalidate(status_change_redraw_reason()),
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
                HoverTarget::UsageStatus => Some(BranchContextBarHit::UsageStatus),
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
            self.focused_usage_status_label().as_deref(),
            self.pull_request_context.as_deref(),
            self.pull_request_context_loading(),
            debug_run_id_label().as_deref(),
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
        register_col_range_1based(tracker, row0, layout.usage_region, HoverTarget::UsageStatus);
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
        // Each dropped event names its gate: this is the dispatch link of the
        // chunk→parse→dispatch→PTY-write debug chain, and a quiet drop here
        // left "clicked in the pane, nothing happened" with no way to localize the failure.
        let drop_trace = |gate: &str| {
            crate::cdebug!(
                "mouse forward dropped at {gate}: row={row} col={col} button={button} press={press}"
            );
        };
        let Some(focused) = self.active_focused_id() else {
            drop_trace("no-focused-pane");
            return false;
        };
        let Some(session) = self.sessions.get(&focused) else {
            drop_trace("session-gone");
            return false;
        };
        let Some(encoding) = mouse_event_encoding_for_mode(
            session.mouse_protocol_mode(),
            session.mouse_protocol_encoding(),
            button,
            press,
        ) else {
            drop_trace("mouse-mode-gate");
            return false;
        };
        let Some(inner) = self.active_focused_inner_rect() else {
            drop_trace("no-inner-rect");
            return false;
        };
        let Some((local_row, local_col)) = local_mouse_position(inner, row, col) else {
            drop_trace("outside-pane");
            return false;
        };
        let Some(buf) =
            encode_mouse_for_protocol(button, local_col + 1, local_row + 1, press, encoding)
        else {
            drop_trace("encoding");
            return false;
        };
        session.send_input(&buf);
        true
    }

    /// Click-to-jump on the focused pane's scrollback scrollbar. Hits only
    /// the right-border track of the focused pane (a click on an unfocused
    /// pane's border stays a focus gesture), and only when that pane retains
    /// scrollback and is not an alt-screen app — the same gates that decide
    /// whether the scrollbar is painted at all. Shared splitter borders never
    /// reach here: the caller checks `detect_drag_start` first, so drag-resize
    /// keeps priority on borders two panes share.
    pub(super) fn scrollbar_jump_at(&mut self, row: u16, col: u16) -> bool {
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
        let Some(session) = self.sessions.get_mut(&focused) else {
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
        let top_offset = jackin_tui::components::scrollbar_offset_for_track_position(
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
        crate::cdebug!(
            "scrollbar jump: session={focused} row={row} col={col} filled={filled} top_offset={top_offset} tail_offset={tail_offset} moved={moved}"
        );
        if moved {
            self.invalidate(wheel_scrollback_redraw_reason());
        }
        moved
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
        let scrollback_filled = session.scrollback_filled();
        let scrollback_offset = session.scrollback_offset();
        crate::cdebug!(
            "selection start: session={id} press=({row},{col}) inner=({},{},{}x{})",
            inner.row,
            inner.col,
            inner.rows,
            inner.cols
        );
        selection_start_for_inner_rect(id, inner, row, col, scrollback_filled, scrollback_offset)
    }

    /// Update the active selection's end-cell to the new motion
    /// position. Clamps to the inner pane rect so a drag that leaves
    /// the pane still produces a reasonable highlight. Dragging above or below
    /// the pane nudges the selected session's scrollback view so long
    /// transcript selections can continue past the visible viewport.
    pub(super) fn selection_motion(&mut self, row: u16, col: u16) {
        let Some((session_id, inner)) = self
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
            if let Some(session) = self.sessions.get_mut(&session_id) {
                if let Some(delta) = scroll_delta {
                    session.scroll_by(delta);
                }
                (session.scrollback_filled(), session.scrollback_offset())
            } else {
                return;
            };
        let Some(sel) = self.selection.as_mut() else {
            return;
        };
        move_selection_end(sel, row, col, scrollback_filled, scrollback_offset);
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
        // The selection changed shape, so the clipboard no longer matches
        // it; release must copy again (extends a word-click selection too).
        self.selection_copied = false;
        self.invalidate(selection_change_redraw_reason());
    }

    /// Promote a press-time selection candidate only after the pointer really
    /// moves away from the anchor cell. Plain clicks remain normal focus/click
    /// gestures and never flash selection chrome or arm clipboard copy.
    pub(super) fn pending_selection_motion(&mut self, row: u16, col: u16) {
        // The press turned into a drag — it must not pair as the first half
        // of a double-click with the click that later clears its copy.
        self.last_pane_press = None;
        self.selection = self.pending_selection.take();
        self.selection_motion(row, col);
        if !self.selection.as_ref().is_some_and(selection_was_dragged) {
            self.selection = None;
        }
    }

    /// Commit the active selection: extract the selected text from the source
    /// session's grid and emit OSC 52 to the attached client (which the outer
    /// terminal turns into a real clipboard write). Dragged selections remain
    /// highlighted after copy until the next click or typed input clears them.
    pub(super) fn finalize_selection(&mut self) {
        let Some(sel) = self.selection else {
            return;
        };
        // A word-click selection was already copied at press time; the
        // release that follows must not write the clipboard again.
        if self.selection_copied {
            return;
        }
        // Suppress single-cell selections: a click-to-focus with no
        // drag motion lands anchor==end and would otherwise OSC 52
        // whatever character sat under the cursor — a silent host-
        // clipboard overwrite on every focus click.
        if selection_was_dragged(&sel) {
            self.copy_selection_to_clipboard(&sel);
        } else {
            self.selection = None;
            self.selection_copied = false;
            self.selection_copy_feedback_deadline = None;
        }
        self.invalidate(selection_change_redraw_reason());
    }

    /// Snapshot the selection's session and copy through
    /// `copy_selection_rows`. Used by drag-release finalize, which holds no
    /// snapshot of its own.
    fn copy_selection_to_clipboard(&mut self, sel: &SelectionState) {
        let rows = self
            .sessions
            .get(&sel.session_id)
            .map(|session| session.render_content_snapshot(sel.inner.cols))
            .unwrap_or_default();
        self.copy_selection_rows(sel, &rows);
    }

    /// OSC 52 the selection's text to the attached client and arm the
    /// "copied" toast — the shared copy body for drag-release finalize and
    /// double-click word selection (which resolves word bounds from the
    /// same rows it copies; the snapshot is a full-grid copy worth taking
    /// once).
    fn copy_selection_rows(
        &mut self,
        sel: &SelectionState,
        rows: &[crate::tui::render::RowSnapshot],
    ) {
        let text = selection_text(rows, sel);
        let copied = !text.is_empty() && self.client.is_attached();
        if copied {
            let bytes = encode_osc52_clipboard_write(&text);
            self.send_out_of_band(bytes);
        } else {
            // No toast and no clipboard write: name the quiet reasons
            // (empty rows from a vanished session, empty extracted text,
            // detached client) in a `--debug` trace.
            crate::cdebug!(
                "selection copy skipped: session={} rows={} text_len={} attached={}",
                sel.session_id,
                rows.len(),
                text.len(),
                self.client.is_attached(),
            );
        }
        self.selection_copied = copied;
        self.selection_copy_feedback_deadline =
            copied.then_some(Instant::now() + crate::tui::update::DIALOG_COPY_FEEDBACK_DURATION);
    }

    /// Classify a primary press on a pane cell as single or double click.
    /// A double-click selects the word under the cursor and copies it
    /// immediately; the highlight stays until the next click or keystroke,
    /// same as a dragged selection. Returns `true` when the press was
    /// consumed as a word selection.
    pub(super) fn register_pane_press(&mut self, candidate: &SelectionState) -> bool {
        let press = PanePress {
            session_id: candidate.session_id,
            content_row: candidate.anchor_row,
            col: candidate.anchor_col,
            at: Instant::now(),
        };
        let is_double = self
            .last_pane_press
            .is_some_and(|previous| is_double_click(&previous, &press));
        if !is_double {
            self.last_pane_press = Some(press);
            return false;
        }
        // A third quick press starts a fresh cycle instead of re-selecting.
        self.last_pane_press = None;
        self.select_word_at(candidate)
    }

    /// Select the word under `candidate`'s anchor cell and copy it. The
    /// word's display-column bounds come from `word_bounds_in_row` over the
    /// session's content snapshot.
    fn select_word_at(&mut self, candidate: &SelectionState) -> bool {
        let Some(session) = self.sessions.get(&candidate.session_id) else {
            crate::cdebug!("word select skipped: session={} gone", candidate.session_id);
            return false;
        };
        let rows = session.render_content_snapshot(candidate.inner.cols);
        let Some((start_col, end_col)) = rows
            .get(candidate.anchor_row)
            .and_then(|row| word_bounds_in_row(row, candidate.anchor_col))
        else {
            crate::cdebug!(
                "word select skipped: no word at session={} content_row={} col={}",
                candidate.session_id,
                candidate.anchor_row,
                candidate.anchor_col,
            );
            return false;
        };
        let mut sel = *candidate;
        sel.anchor_col = start_col;
        sel.end_col = end_col;
        crate::cdebug!(
            "word select: session={} content_row={} cols={start_col}..={end_col}",
            sel.session_id,
            sel.anchor_row
        );
        self.selection = Some(sel);
        self.pending_selection = None;
        self.copy_selection_rows(&sel, &rows);
        self.invalidate(selection_change_redraw_reason());
        true
    }
}

/// Two presses form a double-click when they land on the same content cell
/// of the same session within [`DOUBLE_CLICK_WINDOW`]. Pure so the timing
/// window has direct tests without a clock injection seam.
pub(super) fn is_double_click(previous: &PanePress, press: &PanePress) -> bool {
    previous.session_id == press.session_id
        && previous.content_row == press.content_row
        && previous.col == press.col
        && press.at.duration_since(previous.at) <= DOUBLE_CLICK_WINDOW
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

    pub(super) fn drag_motion(&mut self, row: u16, col: u16) {
        let Some(drag) = self.drag.clone() else {
            return;
        };
        let new_ratio = drag_resize_ratio(drag.orient, drag.rect, row, col);
        let Some(tab) = self.tabs.get_mut(drag.tab_idx) else {
            return;
        };
        if !tab.tree.set_ratio_at(&drag.path, new_ratio) {
            return;
        }
        self.resize_panes();
        self.invalidate(drag_resize_redraw_reason());
    }
}
