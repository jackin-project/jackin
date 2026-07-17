// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! TUI mouse, pointer, hover, and text-selection methods for the daemon-owned `Multiplexer`.

use jackin_protocol::attach::ServerFrame;
use ratatui::layout::Rect;
use termrock::interaction::HitRegion;

use crate::tui::components::branch_context_bar::{
    BranchContextBarHit, ColRange, branch_context_bar_layout, debug_run_id_label,
};
use crate::tui::pane_snapshot::RowSnapshot;
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
pub(crate) struct PanePress {
    pub(super) session_id: u64,
    pub(super) content_row: usize,
    pub(super) col: u16,
    pub(super) at: Instant,
}

enum HostOpenTarget {
    Allowed(String),
    Rejected,
}

impl Multiplexer {
    pub(super) fn set_pointer_shape(&mut self, shape: PointerShape) {
        if !self.client_registry.pointer_shapes_supported
            || self.client_registry.pointer_shape == shape
        {
            return;
        }
        self.client_registry.pointer_shape = shape;
        self.send_out_of_band(osc22_pointer_shape(shape));
    }

    pub(super) fn update_pointer_shape_for_mouse(&mut self, row: u16, col: u16, button: u8) {
        if !self.client_registry.pointer_shapes_supported {
            return;
        }
        let shape = self.pointer_shape_at(row, col, button);
        self.set_pointer_shape(shape);
    }

    pub(super) fn update_hover_for_mouse(&mut self, row: u16, col: u16, button: u8) {
        let next = self.hover_target_at(row, col);
        let next_link = self.link_hover_url_at(row, col, button);
        // The shared Debug info dialog brightens the hovered copyable row, so a
        // move between two copyable rows must redraw even though hover_target
        // stays DialogCopyTarget. Track the per-row hover separately.
        let (term_rows, term_cols) = self.render.terminal_size();
        let row_hover_changed = self.dialog_top_mut().is_some_and(|dialog| {
            let row = row + 1;
            let col = col + 1;
            dialog.set_container_info_hover(row, col, term_rows, term_cols)
                || dialog.set_usage_tab_hover(row, col, term_rows, term_cols)
        });
        if self.render.hover_target == next
            && self.render.link_hover_url == next_link
            && !row_hover_changed
        {
            return;
        }
        self.render.hover_target = next;
        self.render.link_hover_url = next_link;
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
                self.render.term_rows,
                self.render.term_cols,
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

        let mut regions = Vec::new();
        self.register_chrome_hover_targets(&mut regions);
        let position = ratatui::layout::Position::new(col, row);
        let target = regions
            .iter()
            .find(|region| region.area.contains(position))
            .map(|region| region.id);
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

    fn register_chrome_hover_targets(&self, regions: &mut Vec<HitRegion<HoverTarget>>) {
        for (idx, (start, end)) in self
            .status
            .status_bar
            .tab_regions
            .iter()
            .copied()
            .enumerate()
        {
            register_row0_range_1based(regions, start, end, HoverTarget::Tab(idx));
        }
        if let Some((start, end)) = self.status.status_bar.hint_region {
            register_row0_range_1based(regions, start, end, HoverTarget::Menu);
        }

        let Some(layout) = branch_context_bar_layout(
            self.render.term_rows,
            self.render.term_cols,
            self.context_bar_branch(),
            self.focused_usage_status_label().as_deref(),
            self.pr_watch.pull_request_context.as_deref(),
            self.pull_request_context_loading(),
            debug_run_id_label().as_deref(),
            self.status.status_bar.instance_id_label(),
        ) else {
            return;
        };
        let row0 = self.render.term_rows.saturating_sub(1);
        register_col_range_1based(
            regions,
            row0,
            layout.left_region,
            HoverTarget::BranchContext,
        );
        register_col_range_1based(regions, row0, layout.usage_region, HoverTarget::UsageStatus);
        register_col_range_1based(
            regions,
            row0,
            layout.container_region,
            HoverTarget::Container,
        );
        register_col_range_1based(
            regions,
            row0,
            layout.debug_chip_region,
            HoverTarget::DebugChip,
        );
    }

    pub(super) fn hover_target_at(&self, row: u16, col: u16) -> Option<HoverTarget> {
        hover_target_for_state(HoverState {
            dragging: self.render.drag.is_some(),
            selecting: self.clipboard.selection.is_some(),
            chrome_target: self.chrome_hit_target_at(row, col),
        })
    }

    pub(super) fn pointer_shape_at(&self, row: u16, col: u16, button: u8) -> PointerShape {
        pointer_shape_for_state(PointerShapeState {
            dragging: self.render.drag.is_some(),
            selecting: self.clipboard.selection.is_some(),
            chrome_target: self.chrome_hit_target_at(row, col),
            dialog_open: self.dialog_top().is_some(),
            drag_start_orient: self.detect_drag_start(row, col).map(|drag| drag.orient),
            selection_start_available: self.detect_selection_start(row, col).is_some(),
            link_target_available: self.link_hover_url_at(row, col, button).is_some(),
            no_button_motion: button == SGR_NO_BUTTON_MOTION,
        })
    }

    fn link_hover_url_at(&self, row: u16, col: u16, button: u8) -> Option<String> {
        if self.dialog_top().is_some()
            || !host_url_opening_allowed()
            || !is_host_url_hover_button(button)
        {
            return None;
        }
        self.resolve_http_url_at_mouse_cell(row, col)
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
        let delivered = session.send_input(&buf);
        if delivered {
            let _counter_result =
                jackin_telemetry::counter(&jackin_telemetry::metric::TERMINAL_INPUT_MOUSE)
                    .add(1, &[]);
        }
        delivered
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
    pub(super) fn detect_selection_start(&self, row: u16, col: u16) -> Option<SelectionState> {
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
    pub(super) fn selection_motion(&mut self, row: u16, col: u16) {
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
    pub(super) fn pending_selection_motion(&mut self, row: u16, col: u16) {
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
    pub(super) fn finalize_selection(&mut self) {
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
    pub(super) fn register_pane_press(&mut self, candidate: &SelectionState) -> bool {
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

    /// Resolve a modified click in a mouse-disabled pane to a visible host-open
    /// URL and ask the host attach process to open it. Non-URL clicks return
    /// `false` so the caller can preserve the existing raw-mouse fallback.
    pub(super) fn open_visible_url_at(&mut self, row: u16, col: u16) -> bool {
        if !host_url_opening_allowed() {
            return false;
        }
        match self.resolve_host_open_target_at_mouse_cell(row, col) {
            Some(HostOpenTarget::Allowed(url)) => self.send_host_open_url(url),
            Some(HostOpenTarget::Rejected) => {
                self.reject_host_open_url();
                true
            }
            None => false,
        }
    }

    /// Resolve the focused pane's terminal cursor to a visible host-open URL
    /// and ask the host attach process to open it. This backs the command
    /// palette path for terminals or operators that prefer not to use a
    /// mouse-modifier gesture.
    pub(super) fn open_visible_url_under_cursor(&mut self) -> bool {
        let Some(session_id) = self.active_focused_id() else {
            return false;
        };
        let Some(inner) = self.active_focused_inner_rect() else {
            return false;
        };
        let Some(session) = self.session_supervisor.sessions.get(session_id) else {
            return false;
        };
        if session.scrollback_offset() != 0 {
            return false;
        }
        let (cursor_row, cursor_col) = session.shadow_grid.cursor_position();
        // Live-screen content index: scrollback oldest-first, then screen rows.
        let content_row = session
            .shadow_grid
            .scrollback_len()
            .saturating_add(usize::from(cursor_row));
        let rows = session
            .render_content_snapshot_range(inner.cols, content_row..content_row.saturating_add(1));
        if rows.is_empty() {
            return false;
        }
        let Some(target) = self.resolve_host_open_target_at_content_cell(
            session_id,
            &rows,
            content_row,
            content_row,
            cursor_col,
        ) else {
            return false;
        };
        match target {
            HostOpenTarget::Allowed(url) => self.send_host_open_url(url),
            HostOpenTarget::Rejected => {
                self.reject_host_open_url();
                true
            }
        }
    }

    fn resolve_host_open_target_at_mouse_cell(&self, row: u16, col: u16) -> Option<HostOpenTarget> {
        let candidate = self.detect_selection_start(row, col)?;
        let session = self.session_supervisor.sessions.get(candidate.session_id)?;
        // Hover/click URL resolution inspects only the anchor row:
        // single-row word_bounds_in_row; OSC8 uses absolute content coords.
        // Window = 1 row (this function).
        let content_row = candidate.anchor_row;
        let range_start = content_row;
        let rows = session.render_content_snapshot_range(
            candidate.inner.cols,
            content_row..content_row.saturating_add(1),
        );
        self.resolve_host_open_target_at_content_cell(
            candidate.session_id,
            &rows,
            range_start,
            content_row,
            candidate.anchor_col,
        )
    }

    /// Resolve a host-open target at an absolute content cell.
    ///
    /// `rows` may be a full content snapshot or a range slice; `rows_base` is
    /// the absolute content-row index of `rows[0]` so absolute coordinates keep
    /// working without re-basing selection/hyperlink semantics.
    fn resolve_host_open_target_at_content_cell(
        &self,
        session_id: u64,
        rows: &[RowSnapshot],
        rows_base: usize,
        row_idx: usize,
        anchor_col: u16,
    ) -> Option<HostOpenTarget> {
        let session = self.session_supervisor.sessions.get(session_id)?;

        if let Some(osc8_target) = session.hyperlink_target_at_content_row(row_idx, anchor_col) {
            if crate::tui::url_text::is_host_open_url(osc8_target) {
                return Some(HostOpenTarget::Allowed(osc8_target.to_owned()));
            }
            if !crate::tui::url_text::has_url_scheme(osc8_target) {
                return None;
            }
            return Some(HostOpenTarget::Rejected);
        }

        let local_row = row_idx.saturating_sub(rows_base);
        let row = rows.get(local_row)?;
        let (start_col, end_col) = word_bounds_in_row(row, anchor_col)?;
        let url = row.text_range(start_col, end_col);
        if !crate::tui::url_text::is_host_open_url(&url) {
            if !crate::tui::url_text::has_url_scheme(&url) {
                return None;
            }
            return Some(HostOpenTarget::Rejected);
        }
        Some(HostOpenTarget::Allowed(url))
    }

    fn resolve_http_url_at_mouse_cell(&self, row: u16, col: u16) -> Option<String> {
        match self.resolve_host_open_target_at_mouse_cell(row, col) {
            Some(HostOpenTarget::Allowed(url)) => Some(url),
            Some(HostOpenTarget::Rejected) | None => None,
        }
    }

    fn send_host_open_url(&mut self, url: String) -> bool {
        jackin_telemetry::ui::record_action(
            jackin_telemetry::schema::enums::UiActionName::LinkOpen,
            jackin_telemetry::schema::enums::ScreenId::Capsule,
            None,
        );
        self.send_protocol_frame(ServerFrame::HostOpenUrl(url));
        true
    }

    fn reject_host_open_url(&mut self) {
        self.set_clipboard_image_notice("Host link rejected: unsupported URL scheme".to_owned());
    }
}

pub(super) fn host_url_opening_allowed() -> bool {
    // `JACKIN_OPEN_LINKS` is process-launch config, never mutated at runtime, but
    // this is read up to ~2x per mouse-move. Resolve the env var once and cache
    // the parsed verdict so the hot path skips the syscall + allocation.
    static ALLOWED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ALLOWED.get_or_init(|| {
        let value = std::env::var(jackin_core::JACKIN_OPEN_LINKS_ENV_NAME).ok();
        host_url_opening_allowed_for(value.as_deref())
    })
}

pub(super) fn host_url_opening_allowed_for(value: Option<&str>) -> bool {
    jackin_core::open_links_allowed(value)
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

fn is_host_url_hover_button(button: u8) -> bool {
    const ALT_MODIFIER: u8 = 8;
    const CTRL_MODIFIER: u8 = 16;
    const MOTION_MODIFIER: u8 = 32;

    let no_button_motion = button & 0b11 == 3;
    let modified = button & (ALT_MODIFIER | CTRL_MODIFIER) != 0;
    let motion = button & MOTION_MODIFIER != 0;
    no_button_motion && modified && motion
}

fn register_row0_range_1based(
    regions: &mut Vec<HitRegion<HoverTarget>>,
    start: u16,
    end: u16,
    key: HoverTarget,
) {
    if let Some(range) = ColRange::new(start, end) {
        register_col_range_1based(regions, 0, Some(range), key);
    }
}

fn register_col_range_1based(
    regions: &mut Vec<HitRegion<HoverTarget>>,
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
        regions.push(HitRegion {
            id: key,
            area: Rect::new(x, row0, width, 1),
        });
    }
}

impl Multiplexer {
    pub(super) fn detect_drag_start(&self, row: u16, col: u16) -> Option<DragState> {
        if row < STATUS_BAR_ROWS || self.active_zoomed_id().is_some() {
            return None;
        }
        let content_rect = content_rect(self.render.content_rows, self.render.term_cols);
        let tab = self
            .session_supervisor
            .tabs
            .get(self.session_supervisor.active_tab)?;
        let (path, orient, rect) = tab.tree.border_at(content_rect, row, col)?;
        Some(DragState {
            tab_idx: self.session_supervisor.active_tab,
            path,
            orient,
            rect,
        })
    }

    pub(super) fn drag_motion(&mut self, row: u16, col: u16) {
        let Some(drag) = self.render.drag.clone() else {
            return;
        };
        let new_ratio = drag_resize_ratio(drag.orient, drag.rect, row, col);
        let Some(tab) = self.session_supervisor.tabs.get_mut(drag.tab_idx) else {
            return;
        };
        if !tab.tree.set_ratio_at(&drag.path, new_ratio) {
            return;
        }
        self.resize_panes();
        self.invalidate(drag_resize_redraw_reason());
    }
}
