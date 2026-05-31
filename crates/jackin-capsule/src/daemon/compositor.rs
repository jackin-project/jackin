//! Compositor methods for the Multiplexer.
//!
//! Moved from daemon.rs to separate this concern from session lifecycle and
//! input dispatch. All methods are impl Multiplexer blocks.

use std::collections::HashSet;
use std::time::Instant;

use super::*;

impl Multiplexer {
    pub(super) fn compose_pending_frame(&mut self) -> Vec<u8> {
        if let Some(reason) = self.pending_full_redraw.take() {
            self.dirty_panes.clear();
            // Use the Ratatui compositor for all full frames — dialogs are
            // now rendered via shared jackin-tui widgets inside compose_ratatui_frame.
            if let Some(ratatui_output) = self.compose_ratatui_frame() {
                crate::cdebug!(
                    "render: kind=full reason={} via=ratatui bytes={}",
                    reason.as_str(),
                    ratatui_output.len()
                );
                let mut out = Vec::with_capacity(ratatui_output.len() + 64);
                self.append_outer_terminal_title(&mut out);
                out.extend_from_slice(&ratatui_output);
                return out;
            }
            return self.compose_full_frame(reason);
        }
        let dirty_panes = std::mem::take(&mut self.dirty_panes);
        self.compose_partial_frame(dirty_panes)
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
    /// Renders chrome (status bar, pane bodies, pane borders) via the
    /// `ratatui_terminal` double-buffer so only changed cells are sent over
    /// the attach socket. This is the Ratatui-backed replacement for the
    /// raw-ANSI `compose_full_frame`; currently used only for the non-dialog
    /// case while dialog rendering remains on the raw-ANSI path.
    ///
    /// Returns the ANSI output to send to the attach client, or `None` if
    /// the Ratatui terminal fails to draw (falls back to raw-ANSI).
    pub(super) fn compose_ratatui_frame(&mut self) -> Option<Vec<u8>> {
        use crate::chrome_widget::{DialogBackdrop, PaneBorderWidget, StatusBarWidget};
        use crate::dialog_widgets::{DialogRatatuiSnapshot, render_dialog_ratatui};
        use crate::pane_widget::PaneBodyWidget;
        use crate::title::display_title;
        use ratatui::layout::Rect as RatatuiRect;

        let term_rows = self.term_rows;
        let term_cols = self.term_cols;
        let active_tab = self.active_tab;
        let tabs = &self.tabs;
        let panes = self.visible_panes();
        let focused_id = self.active_focused_id();
        let zoomed = self.active_zoomed_id().is_some();
        let multi_pane = panes.len() > 1;
        let dialog_open = self.dialog_open();

        // Snapshot session display titles before the draw closure borrows self.
        let pane_titles: Vec<(u64, String)> = panes
            .iter()
            .filter_map(|pane| {
                self.sessions
                    .get(&pane.id)
                    .map(|s| (pane.id, display_title(s)))
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

        let sessions = &self.sessions;
        let status_bar = StatusBarWidget {
            tabs,
            active_tab,
            cols: term_cols,
        };

        let result = self.ratatui_terminal.draw(|frame| {
            // Status bar: rows 0-1 (STATUS_BAR_ROWS = 2)
            let status_area = RatatuiRect {
                x: 0,
                y: 0,
                width: term_cols,
                height: crate::statusbar::STATUS_BAR_ROWS,
            };
            frame.render_widget(status_bar, status_area);

            if dialog_open {
                // Backdrop fills the content area behind the dialog.
                let content_area = RatatuiRect {
                    x: 0,
                    y: crate::statusbar::STATUS_BAR_ROWS,
                    width: term_cols,
                    height: term_rows.saturating_sub(crate::statusbar::STATUS_BAR_ROWS),
                };
                frame.render_widget(DialogBackdrop, content_area);
                // Render dialog content via shared jackin-tui components.
                if let Some((snapshot, rect)) = &dialog_snapshot {
                    render_dialog_ratatui(frame, *rect, snapshot);
                }
                return;
            }

            // Pane bodies + borders
            for pane in &panes {
                let title = pane_titles
                    .iter()
                    .find(|(id, _)| *id == pane.id)
                    .map(|(_, t)| t.as_str())
                    .unwrap_or("");

                let focused = Some(pane.id) == focused_id;
                let highlight_focus = if zoomed { false } else { multi_pane };

                // Pane border (outer rect)
                let border_area = RatatuiRect {
                    x: pane.outer.col,
                    y: pane.outer.row,
                    width: pane.outer.cols,
                    height: pane.outer.rows,
                };
                frame.render_widget(
                    PaneBorderWidget {
                        title: title.to_string(),
                        focused: focused && highlight_focus,
                    },
                    border_area,
                );

                // Pane body (inner rect)
                if let Some(session) = sessions.get(&pane.id) {
                    let body_area = RatatuiRect {
                        x: pane.inner.col,
                        y: pane.inner.row,
                        width: pane.inner.cols,
                        height: pane.inner.rows,
                    };
                    frame.render_widget(PaneBodyWidget::new(session.screen()), body_area);
                }
            }
        });

        match result {
            Ok(_) => Some(self.ratatui_terminal.backend_mut().take_output()),
            Err(e) => {
                crate::clog!("compose_ratatui_frame: draw failed: {e}; falling back to raw ANSI");
                None
            }
        }
    }

    pub(super) fn compose_full_frame(&mut self, reason: FullRedrawReason) -> Vec<u8> {
        let started = Instant::now();
        let mut buf = Vec::with_capacity(65536);
        self.append_outer_terminal_title(&mut buf);
        buf.extend_from_slice(b"\x1b[?25l");

        // A modal dialog takes over the whole screen: paint an opaque
        // black backdrop so the panes and chrome behind it are fully
        // hidden (not dimmed), then draw the dialog on top. The cursor
        // stays hidden from the `?25l` above (append_cursor_state
        // no-ops while a dialog is open).
        if self.dialog_open() {
            fill_screen(
                &mut buf,
                self.term_rows,
                self.term_cols,
                jackin_tui::DIALOG_BACKDROP,
            );
            if let Some(dialog) = self.dialog_top() {
                let github = self.github_context_view();
                dialog.render_with_hover(
                    &mut buf,
                    self.term_rows,
                    self.term_cols,
                    self.hover_target == Some(HoverTarget::DialogCopyTarget),
                    Some(&github),
                );
                dialog.render_footer_hint(&mut buf, self.term_rows, self.term_cols, Some(&github));
            }
            crate::cdebug!(
                "render: kind=dialog reason={} bytes={} duration_us={}",
                reason.as_str(),
                buf.len(),
                started.elapsed().as_micros()
            );
            return buf;
        }

        // Tab labels track the pane makeup. Done here (not on every
        // spawn / split / remove) so the rule lives in one place.
        self.refresh_tab_labels();

        let states = self.snapshot_session_states();
        self.status_bar.render(
            &mut buf,
            self.term_cols,
            &self.tabs,
            self.active_tab,
            &states,
            hovered_tab(self.hover_target),
            hovered_menu(self.hover_target),
        );

        let focused_id = self.active_focused_id();
        let mut focused_pane_rect: Option<Rect> = None;
        let panes = self.visible_panes();
        let multi_pane = panes.len() > 1;
        let zoomed = self.active_zoomed_id().is_some();
        let mut pane_rows_emitted = 0usize;
        let mut pane_body_bytes = 0usize;
        let mut selection_paint = None;

        for pane in &panes {
            let mut scrollbar = PaneScrollbar::default();
            let mut title = None;
            let selection_for_pane = (!zoomed)
                .then_some(())
                .and_then(|()| self.selection.filter(|sel| sel.session_id == pane.id));
            if let Some(session) = self.sessions.get_mut(&pane.id) {
                scrollbar = pane_scrollbar(session, pane.inner.rows, pane.inner.cols);
                title = Some(display_title(session));
                let body_snapshot = session.render_snapshot(pane.inner.rows, pane.inner.cols);
                if let Some(sel) = selection_for_pane {
                    selection_paint = Some((body_snapshot.clone(), sel, pane.body_dim));
                }
                let before = buf.len();
                let stats = self
                    .pane_body_caches
                    .entry(pane.id)
                    .or_default()
                    .render_full_snapshot(
                        body_snapshot,
                        pane.inner.row,
                        pane.inner.col,
                        pane.inner.rows,
                        pane.inner.cols,
                        pane.body_dim,
                        &mut buf,
                    );
                pane_rows_emitted += stats.rows_emitted;
                pane_body_bytes += buf.len() - before;
                if pane.focused {
                    focused_pane_rect = Some(pane.inner);
                }
            }
            if let Some(title) = title {
                // Always draw a pane box, even for the single-pane
                // case — matches zellij's "every pane is framed"
                // convention and gives the operator a reliable place
                // to read the live `OSC 2` title.
                draw_pane_chrome(&mut buf, pane, &title, scrollbar, zoomed, multi_pane);
            }
        }

        if let Some((rows, sel, dim)) = selection_paint {
            // Paint the selection highlight on top of pane content
            // (but underneath the pane box so the inverse stops at
            // the inner edge). The row snapshot is the exact content
            // rendered above, including inline scrollback prefixes.
            paint_selection_highlight(&mut buf, &rows, &sel, dim);
        }

        let pull_request_loading = self.pull_request_context_loading();
        render_branch_context_bar(
            &mut buf,
            self.term_rows,
            self.term_cols,
            self.context_bar_branch(),
            self.pull_request_context.as_deref(),
            pull_request_loading,
            self.status_bar.instance_id_label(),
            self.hover_target,
        );

        self.append_cursor_state(&mut buf, focused_id, focused_pane_rect);

        crate::cdebug!(
            "render: kind=full reason={} panes={} rows={} pane_bytes={} bytes={} duration_us={}",
            reason.as_str(),
            panes.len(),
            pane_rows_emitted,
            pane_body_bytes,
            buf.len(),
            started.elapsed().as_micros()
        );

        buf
    }

    pub(super) fn compose_dialog_overlay_frame(&mut self, reason: FullRedrawReason) -> Vec<u8> {
        // Dialog overlays always go through the full compositor so the
        // opaque backdrop + footer hint stay consistent for every
        // dialog type.
        self.compose_full_frame(reason)
    }

    pub(super) fn snapshot_session_states(&self) -> Vec<(u64, AgentState)> {
        self.sessions.iter().map(|(&id, s)| (id, s.state)).collect()
    }

    pub(super) fn compose_chrome_hover_frame(&mut self) -> Vec<u8> {
        self.refresh_tab_labels();
        let mut buf = b"\x1b7".to_vec();
        let states = self.snapshot_session_states();
        self.status_bar.render(
            &mut buf,
            self.term_cols,
            &self.tabs,
            self.active_tab,
            &states,
            hovered_tab(self.hover_target),
            hovered_menu(self.hover_target),
        );
        render_branch_context_bar(
            &mut buf,
            self.term_rows,
            self.term_cols,
            self.context_bar_branch(),
            self.pull_request_context.as_deref(),
            self.pull_request_context_loading(),
            self.status_bar.instance_id_label(),
            self.hover_target,
        );
        buf.extend_from_slice(b"\x1b8");
        buf
    }

    pub(super) fn compose_partial_frame(&mut self, dirty_panes: HashSet<u64>) -> Vec<u8> {
        if dirty_panes.is_empty() {
            return Vec::new();
        }
        if self.dialog_open() || self.selection.is_some() {
            return self.compose_full_frame(FullRedrawReason::UnsafePartial);
        }

        let started = Instant::now();
        let panes = self.visible_panes();
        let focused_id = self.active_focused_id();
        let focused_pane_rect = panes
            .iter()
            .find(|pane| pane.focused)
            .map(|pane| pane.inner);

        if !panes.iter().any(|pane| dirty_panes.contains(&pane.id)) {
            crate::cdebug!(
                "render: kind=partial reason=pty-output dirty_panes={} panes=0 rows=0 pane_bytes=0 bytes=0 duration_us={}",
                dirty_panes.len(),
                started.elapsed().as_micros()
            );
            return Vec::new();
        }

        for pane in panes.iter().filter(|pane| dirty_panes.contains(&pane.id)) {
            let Some(session) = self.sessions.get(&pane.id) else {
                continue;
            };
            if session.scrollback_offset != 0 {
                return self.compose_full_frame(FullRedrawReason::ScrollbackMovement);
            }
            if !self.pane_body_caches.get(&pane.id).is_some_and(|cache| {
                cache.is_valid_for(pane.inner.rows, pane.inner.cols, pane.body_dim)
            }) {
                return self.compose_full_frame(FullRedrawReason::PaneCacheMiss);
            }
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
                title = Some(display_title(session));
                let before = buf.len();
                let stats = self
                    .pane_body_caches
                    .entry(pane.id)
                    .or_default()
                    .render_partial(
                        session.screen(),
                        pane.inner.row,
                        pane.inner.col,
                        pane.inner.rows,
                        pane.inner.cols,
                        pane.body_dim,
                        &mut buf,
                    );
                if stats.mode == PaneBodyRenderMode::Full {
                    return self.compose_full_frame(FullRedrawReason::PaneCacheMiss);
                }
                if stats.rows_emitted > 0 {
                    panes_rendered += 1;
                }
                rows_emitted += stats.rows_emitted;
                pane_body_bytes += buf.len() - before;
            }
            if let Some(title) = title {
                draw_pane_chrome(&mut buf, pane, &title, scrollbar, zoomed, multi_pane);
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
                let screen = session.screen();
                let live_input = session.received_output
                    && session.scrollback_offset == 0
                    && !screen.hide_cursor();
                if live_input {
                    let (vt_row, vt_col) = screen.cursor_position();
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
