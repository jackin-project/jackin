//! Compositor methods for the Multiplexer.
//!
//! Moved from daemon.rs to separate this concern from session lifecycle and
//! input dispatch. All methods are impl Multiplexer blocks.

use std::time::Instant;

use crate::tui::app::{VisibleAgentState, visible_agent_state_from_protocol};
use crate::tui::view::{
    CapsuleBottomChrome, CapsuleDialogBottomChrome, render_capsule_bottom_chrome,
    render_capsule_dialog_bottom_chrome,
};

use super::{
    CursorVisibilityState, FullRedrawReason, Multiplexer, Rect, append_osc_window_title,
    compose_outer_terminal_title, cursor_visible_for_state, session_display_title,
};

impl Multiplexer {
    /// Compose the frame for the current state when the generation moved
    /// since the last composed frame. This is the only compositor: there are
    /// no repaint tiers — every frame is the full widget tree, and the only
    /// branch is the wipe policy (a real `\x1b[2J` precedes the frame for
    /// `FirstAttach` and `Resize` only).
    pub(super) fn compose_pending_frame(&mut self) -> Vec<u8> {
        if self.rendered_generation == self.frame_generation {
            return Vec::new();
        }
        let generation = self.frame_generation;
        let reason = self.last_invalidate_reason.take();
        let wipe = self.wipe_pending.take();
        let started = Instant::now();
        let alloc_before = crate::alloc_telemetry::snapshot();
        if wipe.is_some() {
            // Terminal::clear() emits the screen erase and resets the diff
            // baseline; the sentinel fill below then forces a full re-emit
            // over the freshly blanked screen. The erase also wiped the raw
            // bottom-chrome rows, so drop the byte cache to re-assert them.
            drop(self.ratatui_terminal.clear());
            self.last_bottom_chrome = None;
        }
        let Some(output) = self.compose_ratatui_frame() else {
            // compose_ratatui_frame only returns None if the Ratatui draw
            // itself errored — effectively impossible with SocketBackend.
            // Skip the frame; the generation stays ahead so the next loop
            // pass retries.
            crate::clog!("compose_pending_frame: ratatui draw failed; skipping frame");
            return Vec::new();
        };
        self.rendered_generation = generation;
        crate::cdebug!(
            "render: reason={} wipe={} generation={} bytes={} duration_us={} term={}x{} dialog_open={}",
            reason.map_or("none", FullRedrawReason::as_str),
            wipe.is_some(),
            generation,
            output.len(),
            started.elapsed().as_micros(),
            self.term_cols,
            self.term_rows,
            self.dialog_open(),
        );
        if let Some(delta) = crate::alloc_telemetry::delta_since(alloc_before) {
            crate::cdebug!(
                "render_alloc: scope=frame alloc_blocks={} alloc_bytes={} bytes={}",
                delta.blocks,
                delta.bytes,
                output.len(),
            );
        }
        self.frame_with_title(output)
    }

    pub(super) fn append_outer_terminal_title(&mut self, buf: &mut Vec<u8>) {
        let title = compose_outer_terminal_title(
            &self.workdir,
            self.context_bar_branch(),
            self.pull_request_context.as_deref(),
        );
        if self.last_outer_terminal_title.as_deref() == Some(title.as_str()) {
            return;
        }
        append_osc_window_title(buf, &title);
        self.last_outer_terminal_title = Some(title);
    }

    /// Prepend the outer-terminal title to a freshly composed frame.
    ///
    /// `append_outer_terminal_title` writes nothing when the title is unchanged
    /// (the common case: workdir/branch/PR static), so the frame is returned by
    /// move with no copy. Only a title change allocates and prepends.
    fn frame_with_title(&mut self, ratatui_output: Vec<u8>) -> Vec<u8> {
        let mut out = Vec::new();
        self.append_outer_terminal_title(&mut out);
        if out.is_empty() {
            return ratatui_output;
        }
        out.reserve(ratatui_output.len());
        out.extend_from_slice(&ratatui_output);
        out
    }

    /// Compose one frame of the full widget tree through the Ratatui
    /// `SocketBackend`: status bar, pane bodies, pane borders, scrollbars,
    /// selection, and the dialog when open. The bottom chrome and cursor are
    /// still appended as raw ANSI until the chrome-widget step lands.
    ///
    /// Returns the ANSI output to send to the attach client, or `None` if the
    /// Ratatui terminal fails to draw (the caller then skips the frame).
    fn compose_ratatui_frame(&mut self) -> Option<Vec<u8>> {
        use crate::tui::components::dialog_widgets::DialogRatatuiSnapshot;
        use crate::tui::view::{CapsuleRatatuiFrame, PaneScreen, render_capsule_ratatui_frame};

        // Convergence stopgap: force this frame's diff to re-emit every cell
        // — including cells whose target state is default-blank — with no
        // screen-erase byte. Resetting the baseline to default cells is not
        // enough: the diff would skip default-blank cells and residue (dialog
        // backdrops, selection highlights, glyphs scrolled out of the live
        // view) would survive on the client. Filling the about-to-become-
        // previous buffer with a sentinel no widget ever renders makes every
        // composed cell differ, so the diff repaints the full frame in place.
        // Until the extra writers are deleted (PR 3 of the capsule rendering
        // plan), the previous buffer cannot be trusted as the client model.
        for cell in &mut self.ratatui_terminal.current_buffer_mut().content {
            cell.set_symbol("\u{1}");
        }
        self.ratatui_terminal.swap_buffers();

        let term_rows = self.term_rows;
        let term_cols = self.term_cols;
        let active_tab = self.active_tab;
        let tabs = &self.tabs;
        let panes = self.visible_panes();
        let focused_id = self.active_focused_id();
        let focus_owner = focused_id.map_or(
            jackin_tui::components::FocusOwner::TabBar,
            jackin_tui::components::FocusOwner::Content,
        );
        let zoomed = self.active_zoomed_id().is_some();
        let dialog_open = self.dialog_open();
        // Status-bar inputs snapshotted before the draw closure borrows self.
        let session_states = self.snapshot_session_states();
        let prefix_mode = self.status_bar.prefix_mode;
        // Lay out row 0 once per frame. The owned plan is shared with the
        // status-bar widget (paint), the tab tooltip, and the click-region
        // refresh below, so the bar is never laid out more than once per frame.
        let status_plan = crate::tui::components::status_bar::status_bar_plan(
            term_cols,
            tabs,
            active_tab,
            &session_states,
            prefix_mode,
        );
        let hovered_tab = crate::tui::view::hovered_tab(self.hover_target);
        let menu_hovered = crate::tui::view::hovered_menu(self.hover_target);
        // Selection highlight is only meaningful in the unzoomed multi-pane
        // view; a zoom toggle cancels it, matching the raw path's gate.
        let selection = if zoomed { None } else { self.selection };
        let selection_copied = self.selection_copied;

        // Snapshot session display titles before the draw closure borrows self.
        let pane_titles: Vec<(u64, String)> = panes
            .iter()
            .filter_map(|pane| {
                self.sessions
                    .get(&pane.id)
                    .map(|s| (pane.id, session_display_title(s)))
            })
            .collect();
        // Per-pane scrollbar inputs (offset, filled). get_mut because
        // scrollback_filled lazily counts; done before the immutable pane_screens
        // borrow below.
        let pane_scrollbars: Vec<(u64, usize, usize)> = panes
            .iter()
            .filter_map(|pane| {
                self.sessions.get_mut(&pane.id).map(|s| {
                    // Alt-screen apps (Claude Code, vim, …) own their own
                    // scroll — jackin keeps no scrollback for them, so report
                    // filled=0 to suppress the scrollbar thumb on their border.
                    let filled = if s.shadow_grid.alternate_screen() {
                        0
                    } else {
                        s.scrollback_filled()
                    };
                    (pane.id, s.scrollback_offset, filled)
                })
            })
            .collect();
        // Snapshot dialog state (fully owned) before the draw closure.
        let dialog_snapshot: Option<(DialogRatatuiSnapshot, (u16, u16, u16, u16))> = if dialog_open
        {
            let pr_branch = self.pull_request_context_branch.as_deref();
            let pr_info = self.pull_request_context.as_deref();
            let pr_loading = self.pull_request_context_loading();
            let github = crate::tui::components::dialog::github_context_view_from_state(
                pr_branch, pr_info, pr_loading,
            );
            self.dialog_top().map(|d| {
                let rect = d.box_rect(term_rows, term_cols);
                let snapshot = d.to_ratatui_snapshot(Some(&github));
                (snapshot, rect)
            })
        } else {
            None
        };

        // Dialog footer hint. Built from the snapshot + rect so the scrollable
        // info dialogs advertise only the scroll axes their body actually
        // overflows — the hint and the dialog scrollbar are measured the same
        // way and never disagree.
        let github_view_for_hint = self.github_context_view();
        let dialog_hint_spans: Option<Vec<jackin_tui::HintSpan<'static>>> =
            dialog_snapshot.as_ref().and_then(|(snapshot, rect)| {
                self.dialog_top().map(|dialog| {
                    let block = ratatui::layout::Rect {
                        x: rect.1,
                        y: rect.0,
                        width: rect.3,
                        height: rect.2,
                    };
                    dialog
                        .footer_hint_spans(Some(&github_view_for_hint), snapshot.scroll_axes(block))
                })
            });

        // Snapshot scrollback state for the focused session before the draw closure.
        let scrollback_active = focused_id
            .and_then(|id| self.sessions.get(&id))
            .is_some_and(|s| s.scrollback_offset != 0);
        let main_scroll_axes = focused_id
            .and_then(|id| {
                let pane = panes.iter().find(|pane| pane.id == id)?;
                let (_, offset, filled) = pane_scrollbars.iter().find(|(sid, _, _)| *sid == id)?;
                let vertical = jackin_tui::scroll::tail_vertical_thumb(
                    pane.outer.rows.saturating_sub(2),
                    *filled,
                    *offset,
                )
                .is_some();
                Some(jackin_tui::components::ScrollAxes {
                    vertical,
                    horizontal: false,
                })
            })
            .unwrap_or_default();

        // Reset each visible grid's damage memory: under derived rendering
        // damage never selects what to emit, but draining keeps the dirty
        // tracker's span list from growing across frames.
        for pane in &panes {
            if let Some(session) = self.sessions.get_mut(&pane.id) {
                drop(session.shadow_grid.dirty_spans());
            }
        }
        // Pane bodies. Every Ratatui frame must paint complete visible pane
        // bodies, even when the trigger is a single dirty pane. Ratatui builds
        // each draw from a fresh current buffer and diffs it against the
        // previous buffer; omitting an unchanged pane body leaves blank cells in
        // the current buffer, which the diff can send as spaces over the live
        // terminal. Borrowed views avoid per-frame owned snapshots while keeping
        // fallback frames self-contained.
        let pane_screens: Vec<(u64, PaneScreen<'_>)> = panes
            .iter()
            .filter_map(|pane| {
                self.sessions.get(&pane.id).map(|s| {
                    let view = s
                        .shadow_grid
                        .scrollback_view(s.scrollback_offset, pane.inner.rows);
                    (pane.id, PaneScreen::View(view))
                })
            })
            .collect();
        if crate::logging::debug_enabled() {
            for pane in &panes {
                let Some(session) = self.sessions.get(&pane.id) else {
                    continue;
                };
                let actual_filled = session.scrollback_filled();
                let reported = pane_scrollbars
                    .iter()
                    .find(|(id, _, _)| *id == pane.id)
                    .map_or((0, 0), |(_, offset, filled)| (*offset, *filled));
                let thumb = jackin_tui::scroll::tail_vertical_thumb(
                    pane.outer.rows.saturating_sub(2),
                    reported.1,
                    reported.0,
                )
                .map(|thumb| (thumb.start, thumb.len));
                let (grid_rows, grid_cols) = session.shadow_grid.size();
                let (cursor_row, cursor_col) = session.shadow_grid.cursor_position();
                let visible_start =
                    actual_filled.saturating_sub(session.scrollback_offset.min(actual_filled));
                let cursor_visible = cursor_visible_for_state(CursorVisibilityState {
                    dialog_open,
                    focused_pane_available: focused_id == Some(pane.id),
                    focused_session_received_output: session.received_output,
                    scrollback_active: session.scrollback_offset != 0,
                    agent_cursor_hidden: session.shadow_grid.hide_cursor(),
                });
                crate::cdebug!(
                    "pane scroll frame: id={} focused={} agent={:?} label={} alt_screen={} mouse_enabled={} content_rows={} scrollback_actual={} scrollback_reported={} offset={} reported_offset={} viewport={}x{} screen={}x{} visible_start={} thumb={:?} cursor={}x{} cursor_visible={}",
                    pane.id,
                    focused_id == Some(pane.id),
                    session.agent,
                    session.label,
                    session.shadow_grid.alternate_screen(),
                    session.mouse_enabled(),
                    actual_filled.saturating_add(usize::from(grid_rows)),
                    actual_filled,
                    reported.1,
                    session.scrollback_offset,
                    reported.0,
                    pane.inner.rows,
                    pane.inner.cols,
                    grid_rows,
                    grid_cols,
                    visible_start,
                    thumb,
                    cursor_row,
                    cursor_col,
                    cursor_visible,
                );
            }
        }
        crate::cdebug!(
            "render: ratatui-frame panes={} pane_screens={}",
            panes.len(),
            pane_screens.len(),
        );

        let result = self.ratatui_terminal.draw(|frame| {
            render_capsule_ratatui_frame(
                frame,
                CapsuleRatatuiFrame {
                    tabs,
                    status_plan: &status_plan,
                    term_cols,
                    term_rows,
                    panes: &panes,
                    pane_titles: &pane_titles,
                    focus_owner,
                    zoomed,
                    dialog_open,
                    dialog_snapshot: dialog_snapshot.as_ref(),
                    pane_screens: &pane_screens,
                    prefix_mode,
                    hovered_tab,
                    menu_hovered,
                    selection,
                    selection_copied,
                    scrollbars: &pane_scrollbars,
                },
            );
        });

        // Keep tab/menu click regions in sync with the columns the widget
        // just painted, from the same plan the widget rendered, so hit-testing
        // is correct after a Ratatui frame without re-laying out the bar.
        self.status_bar.set_click_regions_from_plan(&status_plan);

        match result {
            Ok(_) => {
                let mut output = Vec::new();
                self.ratatui_terminal
                    .backend_mut()
                    .drain_output_into(&mut output);
                drop(pane_screens);
                // OSC 8 hyperlinks can't ride the Ratatui cell buffer, so the
                // shared Debug info dialog's clickable rows (the diagnostics
                // log path) are re-emitted as a raw overlay over the cells the
                // frame already drew — same pattern the host uses.
                if let Some((DialogRatatuiSnapshot::DebugInfo(state), (row, col, height, width))) =
                    dialog_snapshot.as_ref()
                {
                    let area = ratatui::layout::Rect {
                        x: *col,
                        y: *row,
                        width: *width,
                        height: *height,
                    };
                    output.extend_from_slice(
                        &jackin_tui::components::container_info_hyperlink_overlay(area, state),
                    );
                }
                // Structural exception: bottom chrome (branch/PR bar, hint row,
                // debug chip) remains raw ANSI because it sits in reserved
                // attach-tail rows that must be asserted after Ratatui's cell
                // diff. Ratatui owns status + panes; this adapter owns only the
                // bottom rows. Build it into its own buffer and re-emit only on
                // change, otherwise streaming output can repaint unchanged
                // chrome often enough to visibly flicker. The cache is reset to
                // None after a screen-clearing frame (see compose_full_redraw)
                // so the rows are re-asserted after the wipe.
                let mut chrome_buf = Vec::new();
                if dialog_open {
                    render_capsule_dialog_bottom_chrome(
                        &mut chrome_buf,
                        CapsuleDialogBottomChrome {
                            term_rows: self.term_rows,
                            term_cols: self.term_cols,
                            branch: self.context_bar_branch(),
                            pull_request: self.pull_request_context.as_deref(),
                            pull_request_loading: self.pull_request_context_loading(),
                            instance_id_label: self.status_bar.instance_id_label(),
                            hint_spans: dialog_hint_spans.as_deref(),
                            blank_background: false,
                        },
                    );
                } else {
                    let debug_run_id_owned: Option<String> = if crate::logging::debug_enabled() {
                        let diag = crate::container_context::resolve_container_diagnostics();
                        (!diag.run_id.is_empty()).then_some(diag.run_id)
                    } else {
                        None
                    };
                    render_capsule_bottom_chrome(
                        &mut chrome_buf,
                        CapsuleBottomChrome {
                            term_rows: self.term_rows,
                            term_cols: self.term_cols,
                            branch: self.context_bar_branch(),
                            pull_request: self.pull_request_context.as_deref(),
                            pull_request_loading: self.pull_request_context_loading(),
                            instance_id_label: self.status_bar.instance_id_label(),
                            hover_target: self.hover_target,
                            scrollback_active,
                            scroll_axes: main_scroll_axes,
                            debug_run_id: debug_run_id_owned.as_deref(),
                        },
                    );
                }
                if self.last_bottom_chrome.as_deref() != Some(chrome_buf.as_slice()) {
                    output.extend_from_slice(&chrome_buf);
                    self.last_bottom_chrome = Some(chrome_buf);
                }
                // Position (or hide) the operator's cursor at the focused pane's
                // live VT cursor. Ratatui's draw hides the cursor by default and
                // the SocketBackend never repositions it, so without this append
                // the blinking cursor is absent in every Ratatui frame. No-ops
                // while a dialog is open (cursor stays hidden).
                let focused_pane_rect = panes.iter().find(|p| p.focused).map(|p| p.inner);
                self.append_cursor_state(&mut output, focused_id, focused_pane_rect);
                Some(output)
            }
            Err(e) => {
                crate::clog!("compose_ratatui_frame: draw failed: {e}; skipping frame");
                None
            }
        }
    }

    pub(super) fn snapshot_session_states(&self) -> Vec<(u64, VisibleAgentState)> {
        self.sessions
            .iter()
            .map(|(&id, s)| (id, visible_agent_state_from_protocol(s.state)))
            .collect()
    }

    pub(super) fn append_cursor_state(
        &self,
        buf: &mut Vec<u8>,
        focused_id: Option<u64>,
        focused_pane_rect: Option<Rect>,
    ) {
        // Position cursor at the focused pane's screen cursor only when
        // the pane has something the operator can actually type into.
        // Show conditions, all must hold:
        //   1. No dialog is open (already gated above).
        //   2. Focused session has produced PTY output. A pane that
        //      just spawned (or split-into-shell that hasn't drawn its
        //      first prompt yet) paints a stray blinking cursor at
        //      `(0, 0)` of an empty rectangle otherwise.
        //   3. The agent did not request cursor hidden (`\x1b[?25l`).
        //   4. The operator is not browsing scrollback — the live VT
        //      cursor position is meaningless against history rows.
        // When any rule fails we emit `\x1b[?25l` so no second cursor
        // remains visible anywhere else in the multiplexer chrome.
        if !self.dialog_open() {
            let mut showed = false;
            if let (Some(fid), Some(rect)) = (focused_id, focused_pane_rect)
                && let Some(session) = self.sessions.get(&fid)
                && cursor_visible_for_state(CursorVisibilityState {
                    dialog_open: self.dialog_open(),
                    focused_pane_available: true,
                    focused_session_received_output: session.received_output,
                    scrollback_active: session.scrollback_offset != 0,
                    agent_cursor_hidden: session.shadow_grid.hide_cursor(),
                })
            {
                let (vt_row, vt_col) = session.shadow_grid.cursor_position();
                use std::io::Write as _;
                let _unused = write!(
                    buf,
                    "\x1b[{};{}H",
                    rect.row + vt_row + 1,
                    rect.col + vt_col + 1
                );
                buf.extend_from_slice(b"\x1b[?25h");
                showed = true;
            }
            if !showed {
                buf.extend_from_slice(b"\x1b[?25l");
            }
        }
    }
}
