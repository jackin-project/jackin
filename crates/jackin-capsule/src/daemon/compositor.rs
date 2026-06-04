//! Compositor methods for the Multiplexer.
//!
//! Moved from daemon.rs to separate this concern from session lifecycle and
//! input dispatch. All methods are impl Multiplexer blocks.

use std::collections::HashSet;
use std::time::Instant;

use crate::tui::app::{VisibleAgentState, visible_agent_state_from_protocol};
use crate::tui::view::{
    CapsuleBottomChrome, CapsuleDialogBottomChrome, PaneScrollbar, grid_scroll_affordance_metrics,
    render_capsule_bottom_chrome, render_capsule_dialog_bottom_chrome, render_capsule_pane_chrome,
};

use super::*;

fn pane_scrollbar(session: &mut Session, viewport_rows: u16, viewport_cols: u16) -> PaneScrollbar {
    let debug_enabled = crate::logging::debug_enabled();
    let (filled, vt_filled, inline_filled) = if debug_enabled {
        let (vt_filled, inline_filled) = session.scrollback_counts();
        (
            vt_filled.saturating_add(inline_filled),
            vt_filled,
            inline_filled,
        )
    } else {
        (session.scrollback_filled(), 0, 0)
    };
    let scrollbar = PaneScrollbar {
        offset: session.scrollback_offset,
        filled,
    };
    let metrics = if debug_enabled {
        let snap = session.shadow_grid.dump();
        grid_scroll_affordance_metrics(&snap, viewport_rows, viewport_cols)
    } else {
        None
    };
    crate::cdebug!(
        "scrollbar decision: agent={:?} alt_screen={} mouse_enabled={} viewport={}x{} screen={}x{} cursor={}x{} occupied_rows={} first_occupied_row={} last_occupied_row={} vt_scrollback={} inline_scrollback={} scrollback_filled={} visible={} reason={}",
        session.agent,
        session.shadow_grid.alternate_screen(),
        session.mouse_enabled(),
        viewport_rows,
        viewport_cols,
        metrics.as_ref().map_or(0, |m| m.screen_rows),
        metrics.as_ref().map_or(0, |m| m.screen_cols),
        metrics.as_ref().map_or(0, |m| m.cursor_row),
        metrics.as_ref().map_or(0, |m| m.cursor_col),
        metrics.as_ref().map_or(0, |m| m.occupied_rows),
        metrics
            .as_ref()
            .and_then(|m| m.first_occupied_row)
            .map_or(-1, i32::from),
        metrics
            .as_ref()
            .and_then(|m| m.last_occupied_row)
            .map_or(-1, i32::from),
        vt_filled,
        inline_filled,
        filled,
        scrollbar.visible(),
        if scrollbar.visible() {
            "retained-scrollback"
        } else {
            "none"
        }
    );
    scrollbar
}

impl Multiplexer {
    pub(super) fn compose_pending_frame(&mut self) -> Vec<u8> {
        let backend_size = self
            .ratatui_terminal
            .size()
            .map_or((0, 0), |s| (s.width, s.height));
        crate::cdebug!(
            "frame: full_redraw={:?} dirty_panes={} term={}x{} backend={}x{} content_rows={} dialog_open={}",
            self.pending_full_redraw.map(|r| r.as_str()),
            self.dirty_panes.len(),
            self.term_cols,
            self.term_rows,
            backend_size.0,
            backend_size.1,
            self.content_rows,
            self.dialog_open(),
        );
        if let Some(reason) = self.pending_full_redraw.take() {
            return self.compose_full_redraw(reason);
        }
        let dirty_panes = std::mem::take(&mut self.dirty_panes);
        self.compose_partial_frame(dirty_panes)
    }

    /// Single entry point for every full frame, regardless of which path
    /// requested it (initial pending-redraw, an interactive action, the chrome
    /// ticker, or a partial frame that escalated to `PartialFramePlan::Full`).
    /// Routing *all* full frames through here keeps one compositor in charge:
    /// the Ratatui `SocketBackend` paints status + panes and `compose_ratatui_frame`
    /// appends the raw bottom chrome + cursor. This is the only full-frame
    /// renderer — the legacy raw-ANSI `compose_full_frame` has been removed.
    pub(super) fn compose_full_redraw(&mut self, reason: FullRedrawReason) -> Vec<u8> {
        self.dirty_panes.clear();
        // Reset Ratatui's double-buffer so the next draw is a full repaint.
        // Terminal::clear() routes through SocketBackend::clear_region(All),
        // which emits `\x1b[2J\x1b[H` — every full frame therefore starts from
        // a wiped screen, so chrome from a prior geometry (the raw-ANSI bottom
        // bar the diff cannot track) is never orphaned. The erase is sent
        // atomically with the immediately-following full repaint, so there is
        // no visible blank flash.
        let _ = self.ratatui_terminal.clear();
        let Some(ratatui_output) = self.compose_ratatui_frame() else {
            // compose_ratatui_frame only returns None if the Ratatui draw
            // itself errored — effectively impossible with SocketBackend. Skip
            // the frame; the next tick repaints. (There is no raw fallback: the
            // legacy compose_full_frame renderer has been removed.)
            crate::clog!("compose_full_redraw: ratatui draw failed; skipping frame");
            return Vec::new();
        };
        crate::cdebug!(
            "render: kind=full reason={} via=ratatui bytes={}",
            reason.as_str(),
            ratatui_output.len(),
        );
        let mut out = Vec::with_capacity(ratatui_output.len() + 64);
        self.append_outer_terminal_title(&mut out);
        out.extend_from_slice(&ratatui_output);
        out
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

    /// Compose a full frame using the Ratatui `SocketBackend`.
    ///
    /// Renders status bar, pane bodies, pane borders, selection, and the dialog
    /// (when open) through the `ratatui_terminal` double-buffer so only changed
    /// cells are sent over the attach socket, then appends the bottom chrome and
    /// cursor as raw ANSI (neither rides the cell buffer). This is the capsule's
    /// only renderer.
    ///
    /// Returns the ANSI output to send to the attach client, or `None` if the
    /// Ratatui terminal fails to draw (the caller then skips the frame).
    pub(super) fn compose_ratatui_frame(&mut self) -> Option<Vec<u8>> {
        use crate::tui::components::dialog_widgets::DialogRatatuiSnapshot;
        use crate::tui::view::{CapsuleRatatuiFrame, render_capsule_ratatui_frame};

        let term_rows = self.term_rows;
        let term_cols = self.term_cols;
        let active_tab = self.active_tab;
        let tabs = &self.tabs;
        let panes = self.visible_panes();
        let focused_id = self.active_focused_id();
        let zoomed = self.active_zoomed_id().is_some();
        let dialog_open = self.dialog_open();
        // Status-bar inputs snapshotted before the draw closure borrows self.
        let session_states = self.snapshot_session_states();
        let prefix_mode = self.status_bar.prefix_mode;
        let hovered_tab = crate::tui::view::hovered_tab(self.hover_target);
        let menu_hovered = crate::tui::view::hovered_menu(self.hover_target);
        // Selection highlight is only meaningful in the unzoomed multi-pane
        // view; a zoom toggle cancels it, matching the raw path's gate.
        let selection = if zoomed { None } else { self.selection };
        let github_view_for_hint = self.github_context_view();
        let dialog_hint_spans = self
            .dialog_top()
            .map(|dialog| dialog.footer_hint_spans(Some(&github_view_for_hint)));

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
                self.sessions
                    .get_mut(&pane.id)
                    .map(|s| (pane.id, s.scrollback_offset, s.scrollback_filled()))
            })
            .collect();
        // Pane bodies as GridSnapshots. A pane the operator has scrolled up in
        // (scrollback_offset != 0) is dumped as a scrollback VIEW so the
        // Ratatui body shows history instead of the live tail — the parallel of
        // the raw path's render_snapshot scrollback branch.
        let pane_screens: Vec<(u64, jackin_term::GridSnapshot)> = panes
            .iter()
            .filter_map(|pane| {
                self.sessions.get(&pane.id).map(|s| {
                    (
                        pane.id,
                        s.shadow_grid
                            .dump_scrollback_view(s.scrollback_offset, pane.inner.rows),
                    )
                })
            })
            .collect();

        // Snapshot dialog state (fully owned) before the draw closure.
        let dialog_snapshot: Option<(DialogRatatuiSnapshot, (u16, u16, u16, u16))> = if dialog_open
        {
            let pr_branch = self.pull_request_context_branch.as_deref();
            let pr_info = self.pull_request_context.as_deref();
            let pr_loading = self.pull_request_context_loading();
            self.dialog_top().map(|d| {
                let rect = d.box_rect(term_rows, term_cols);
                let snapshot = d.to_ratatui_snapshot(pr_branch, pr_info, pr_loading);
                (snapshot, rect)
            })
        } else {
            None
        };

        // Snapshot scrollback state for the focused session before the draw closure.
        let scrollback_active = focused_id
            .and_then(|id| self.sessions.get(&id))
            .is_some_and(|s| s.scrollback_offset != 0);

        let result = self.ratatui_terminal.draw(|frame| {
            render_capsule_ratatui_frame(
                frame,
                CapsuleRatatuiFrame {
                    tabs,
                    active_tab,
                    term_cols,
                    term_rows,
                    panes: &panes,
                    pane_titles: &pane_titles,
                    focused_id,
                    zoomed,
                    dialog_open,
                    dialog_snapshot: dialog_snapshot.as_ref(),
                    pane_screens: &pane_screens,
                    sessions_state: &session_states,
                    prefix_mode,
                    hovered_tab,
                    menu_hovered,
                    selection,
                    scrollbars: &pane_scrollbars,
                },
            );
        });

        // Keep tab/menu click regions in sync with the columns the widget
        // just painted (both derive from status_bar_plan), so hit-testing is
        // correct after a Ratatui frame, not just after a raw one.
        self.status_bar.refresh_click_regions(
            self.term_cols,
            &self.tabs,
            self.active_tab,
            &session_states,
        );

        match result {
            Ok(_) => {
                let mut output = self.ratatui_terminal.backend_mut().take_output();
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
                // Bottom chrome (branch/PR bar, hint row, debug chip) is raw
                // ANSI appended after the Ratatui diff for both the dialog and
                // non-dialog cases. Ratatui owns status + panes; the raw append
                // owns the bottom rows. Keeping a single compositor in charge of
                // each row is what stops the two shadow buffers from drifting.
                if dialog_open {
                    render_capsule_dialog_bottom_chrome(
                        &mut output,
                        CapsuleDialogBottomChrome {
                            term_rows: self.term_rows,
                            term_cols: self.term_cols,
                            branch: self.context_bar_branch(),
                            pull_request: self.pull_request_context.as_deref(),
                            pull_request_loading: self.pull_request_context_loading(),
                            instance_id_label: self.status_bar.instance_id_label(),
                            hint_spans: dialog_hint_spans,
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
                        &mut output,
                        CapsuleBottomChrome {
                            term_rows: self.term_rows,
                            term_cols: self.term_cols,
                            branch: self.context_bar_branch(),
                            pull_request: self.pull_request_context.as_deref(),
                            pull_request_loading: self.pull_request_context_loading(),
                            instance_id_label: self.status_bar.instance_id_label(),
                            hover_target: self.hover_target,
                            scrollback_active,
                            debug_run_id: debug_run_id_owned.as_deref(),
                        },
                    );
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

    pub(super) fn compose_dialog_overlay_frame(&mut self, reason: FullRedrawReason) -> Vec<u8> {
        // Prefer the Ratatui diff path: it only sends changed cells so a
        // dialog whose state hasn't changed produces an empty or near-empty
        // diff instead of a full fill_screen + repaint. This eliminates the
        // flicker visible when the state_ticker fires while a dialog is open.
        if let Some(ratatui_output) = self.compose_ratatui_frame() {
            crate::cdebug!(
                "render: kind=dialog-overlay reason={} via=ratatui bytes={}",
                reason.as_str(),
                ratatui_output.len()
            );
            let mut out = Vec::with_capacity(ratatui_output.len() + 64);
            self.append_outer_terminal_title(&mut out);
            out.extend_from_slice(&ratatui_output);
            return out;
        }
        // No raw fallback: the Ratatui draw is effectively infallible with
        // SocketBackend. Skip the frame on the impossible error; the next tick
        // repaints.
        crate::clog!("compose_dialog_overlay_frame: ratatui draw failed; skipping frame");
        let _ = reason;
        Vec::new()
    }

    pub(super) fn snapshot_session_states(&self) -> Vec<(u64, VisibleAgentState)> {
        self.sessions
            .iter()
            .map(|(&id, s)| (id, visible_agent_state_from_protocol(s.state)))
            .collect()
    }

    pub(super) fn compose_partial_frame(&mut self, dirty_panes: HashSet<u64>) -> Vec<u8> {
        match partial_frame_plan(PartialFrameState {
            dirty_empty: dirty_panes.is_empty(),
            overlay_active: self.dialog_open() || self.selection.is_some(),
            any_dirty_visible_pane: true,
            dirty_pane_scrollback_active: false,
            dirty_pane_cache_invalid: false,
        }) {
            PartialFramePlan::Empty => return Vec::new(),
            PartialFramePlan::OverlayDiff => {
                // Ratatui diff path: unchanged dialog state produces an empty
                // diff instead of a full repaint. No raw fallback — skip the
                // frame on the impossible draw error.
                if let Some(ratatui_output) = self.compose_ratatui_frame() {
                    let mut out = Vec::with_capacity(ratatui_output.len() + 64);
                    self.append_outer_terminal_title(&mut out);
                    out.extend_from_slice(&ratatui_output);
                    return out;
                }
                crate::clog!("compose_partial_frame overlay: ratatui draw failed; skipping frame");
                return Vec::new();
            }
            PartialFramePlan::Full(reason) => return self.compose_full_redraw(reason),
            PartialFramePlan::Partial => {}
        }

        let started = Instant::now();
        let panes = self.visible_panes();
        let focused_id = self.active_focused_id();
        let focused_pane_rect = panes
            .iter()
            .find(|pane| pane.focused)
            .map(|pane| pane.inner);

        let any_dirty_visible_pane = panes.iter().any(|pane| dirty_panes.contains(&pane.id));
        if let PartialFramePlan::Empty = partial_frame_plan(PartialFrameState {
            dirty_empty: false,
            overlay_active: false,
            any_dirty_visible_pane,
            dirty_pane_scrollback_active: false,
            dirty_pane_cache_invalid: false,
        }) {
            crate::cdebug!(
                "render: kind=partial reason=pty-output dirty_panes={} panes=0 rows=0 pane_bytes=0 bytes=0 duration_us={}",
                dirty_panes.len(),
                started.elapsed().as_micros()
            );
            return Vec::new();
        }

        let mut dirty_pane_scrollback_active = false;
        let mut dirty_pane_cache_invalid = false;
        for pane in panes.iter().filter(|pane| dirty_panes.contains(&pane.id)) {
            let Some(session) = self.sessions.get(&pane.id) else {
                continue;
            };
            if session.scrollback_offset != 0 {
                dirty_pane_scrollback_active = true;
            }
            if !self.pane_body_caches.get(&pane.id).is_some_and(|cache| {
                cache.is_valid_for(pane.inner.rows, pane.inner.cols, pane.body_dim)
            }) {
                dirty_pane_cache_invalid = true;
            }
        }
        if let PartialFramePlan::Full(reason) = partial_frame_plan(PartialFrameState {
            dirty_empty: false,
            overlay_active: false,
            any_dirty_visible_pane,
            dirty_pane_scrollback_active,
            dirty_pane_cache_invalid,
        }) {
            return self.compose_full_redraw(reason);
        }

        let mut buf = Vec::with_capacity(16384);
        self.append_outer_terminal_title(&mut buf);
        buf.extend_from_slice(b"\x1b[?25l");
        let mut rows_emitted = 0usize;
        let mut panes_rendered = 0usize;
        let mut pane_body_bytes = 0usize;
        let multi_pane = panes.len() > 1;
        let zoomed = self.active_zoomed_id().is_some();
        for pane in panes.iter().filter(|pane| dirty_panes.contains(&pane.id)) {
            let mut scrollbar = PaneScrollbar::default();
            let mut title = None;
            if let Some(session) = self.sessions.get_mut(&pane.id) {
                scrollbar = pane_scrollbar(session, pane.inner.rows, pane.inner.cols);
                title = Some(session_display_title(session));
                let before = buf.len();
                // Phase 5: dirty-pane incremental render uses WireEmitter
                // (dirty_spans only, no PaneBodyCache snapshot diffing).
                let (snap, spans) = session.take_damagegrid_frame();
                let mut emitter = jackin_term::WireEmitter::new();
                emitter.emit_pane(&snap, &spans, pane.inner.row, pane.inner.col);
                buf.extend_from_slice(emitter.as_bytes());
                let rows_count = emitter.as_bytes().len();
                if rows_count > 0 {
                    panes_rendered += 1;
                }
                rows_emitted += snap.rows as usize;
                pane_body_bytes += buf.len() - before;
            }
            if let Some(title) = title {
                render_capsule_pane_chrome(&mut buf, pane, &title, scrollbar, zoomed, multi_pane);
            }
        }

        self.append_cursor_state(&mut buf, focused_id, focused_pane_rect);

        crate::cdebug!(
            "render: kind=partial reason=pty-output dirty_panes={} panes={} rows={} pane_bytes={} bytes={} duration_us={}",
            dirty_panes.len(),
            panes_rendered,
            rows_emitted,
            pane_body_bytes,
            buf.len(),
            started.elapsed().as_micros()
        );

        buf
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
            {
                let snap = session.shadow_grid.dump();
                if cursor_visible_for_state(CursorVisibilityState {
                    dialog_open: self.dialog_open(),
                    focused_pane_available: true,
                    focused_session_received_output: session.received_output,
                    scrollback_active: session.scrollback_offset != 0,
                    agent_cursor_hidden: session.shadow_grid.hide_cursor(),
                }) {
                    let (vt_row, vt_col) = snap.cursor;
                    use std::io::Write as _;
                    let _ = write!(
                        buf,
                        "\x1b[{};{}H",
                        rect.row + vt_row + 1,
                        rect.col + vt_col + 1
                    );
                    buf.extend_from_slice(b"\x1b[?25h");
                    showed = true;
                }
            }
            if !showed {
                buf.extend_from_slice(b"\x1b[?25l");
            }
        }
    }
}
