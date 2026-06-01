//! Compositor methods for the Multiplexer.
//!
//! Moved from daemon.rs to separate this concern from session lifecycle and
//! input dispatch. All methods are impl Multiplexer blocks.

use std::collections::HashSet;
use std::time::Instant;

use crate::tui::view::{
    CapsuleBottomChrome, CapsuleChromeHoverFrame, CapsuleDialogBottomChrome,
    CapsuleRawDialogOverlay, CapsuleStatusBarFrame, PaneScrollbar,
    render_capsule_bottom_chrome, render_capsule_chrome_hover_frame,
    render_capsule_dialog_backdrop, render_capsule_dialog_bottom_chrome,
    render_capsule_pane_body_partial, render_capsule_pane_body_snapshot,
    render_capsule_pane_chrome,
    render_capsule_raw_dialog_overlay, render_capsule_selection_highlight,
    render_capsule_status_bar,
    screen_scroll_affordance_metrics,
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
        screen_scroll_affordance_metrics(session.screen(), viewport_rows, viewport_cols)
    } else {
        None
    };
    crate::cdebug!(
        "scrollbar decision: agent={:?} alt_screen={} mouse_enabled={} viewport={}x{} screen={}x{} cursor={}x{} occupied_rows={} first_occupied_row={} last_occupied_row={} vt_scrollback={} inline_scrollback={} scrollback_filled={} visible={} reason={}",
        session.agent,
        session.screen().alternate_screen(),
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
        if let Some(reason) = self.pending_full_redraw.take() {
            self.dirty_panes.clear();
            // Reset Ratatui's internal double-buffer before a full redraw so the
            // diff treats every cell as "changed". Without this, a layout change
            // (e.g. tab close) leaves stale cells from the previous layout in the
            // buffer; the diff skips them because their values happen to match new
            // content at the same screen position, causing visible corruption.
            // SocketBackend::clear() deliberately does NOT emit \x1b[2J so this
            // reset is flicker-free — the next draw() sends every cell instead.
            let _ = self.ratatui_terminal.clear();
            // Use the Ratatui compositor for all full frames; visible widget
            // rendering lives under the capsule TUI boundary.
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
        use crate::title::display_title;
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
                    .map(|s| (pane.id, display_title(s)))
            })
            .collect();
        let pane_screens: Vec<(u64, &vt100::Screen)> = panes
            .iter()
            .filter_map(|pane| self.sessions.get(&pane.id).map(|s| (pane.id, s.screen())))
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
                    scrollback_active,
                    pane_screens: &pane_screens,
                },
            );
        });

        match result {
            Ok(_) => {
                let mut output = self.ratatui_terminal.backend_mut().take_output();
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
                }
                Some(output)
            }
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
            let github = self.github_context_view();
            if let Some(dialog) = self.dialog_top() {
                render_capsule_raw_dialog_overlay(
                    &mut buf,
                    CapsuleRawDialogOverlay {
                        term_rows: self.term_rows,
                        term_cols: self.term_cols,
                        dialog,
                        copy_target_hovered: self.hover_target
                            == Some(HoverTarget::DialogCopyTarget),
                        github,
                    },
                );
            } else {
                render_capsule_dialog_backdrop(&mut buf, self.term_rows, self.term_cols);
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
        render_capsule_status_bar(
            &mut buf,
            &mut self.status_bar,
            CapsuleStatusBarFrame {
                term_cols: self.term_cols,
                tabs: &self.tabs,
                active_tab: self.active_tab,
                session_states: &states,
                hover_target: self.hover_target,
            },
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
                let cache = self.pane_body_caches.entry(pane.id).or_default();
                let stats = render_capsule_pane_body_snapshot(&mut buf, cache, pane, body_snapshot);
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
                render_capsule_pane_chrome(&mut buf, pane, &title, scrollbar, zoomed, multi_pane);
            }
        }

        if let Some((rows, sel, dim)) = selection_paint {
            // Paint the selection highlight on top of pane content
            // (but underneath the pane box so the inverse stops at
            // the inner edge). The row snapshot is the exact content
            // rendered above, including inline scrollback prefixes.
            render_capsule_selection_highlight(&mut buf, &rows, &sel, dim);
        }

        let pull_request_loading = self.pull_request_context_loading();
        let scrollback_active = focused_id
            .and_then(|id| self.sessions.get(&id))
            .is_some_and(|s| s.scrollback_offset != 0);
        render_capsule_bottom_chrome(
            &mut buf,
            CapsuleBottomChrome {
                term_rows: self.term_rows,
                term_cols: self.term_cols,
                branch: self.context_bar_branch(),
                pull_request: self.pull_request_context.as_deref(),
                pull_request_loading,
                instance_id_label: self.status_bar.instance_id_label(),
                hover_target: self.hover_target,
                scrollback_active,
            },
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
        // Raw-ANSI fallback: only reached when the Ratatui terminal fails.
        self.compose_full_frame(reason)
    }

    pub(super) fn snapshot_session_states(&self) -> Vec<(u64, AgentState)> {
        self.sessions.iter().map(|(&id, s)| (id, s.state)).collect()
    }

    pub(super) fn compose_chrome_hover_frame(&mut self) -> Vec<u8> {
        self.refresh_tab_labels();
        let mut buf = b"\x1b7".to_vec();
        let states = self.snapshot_session_states();
        let branch = self.context_bar_branch().map(str::to_string);
        let pull_request = self.pull_request_context.clone();
        let pull_request_loading = self.pull_request_context_loading();
        render_capsule_chrome_hover_frame(
            &mut buf,
            &mut self.status_bar,
            CapsuleChromeHoverFrame {
                term_rows: self.term_rows,
                term_cols: self.term_cols,
                tabs: &self.tabs,
                active_tab: self.active_tab,
                session_states: &states,
                branch: branch.as_deref(),
                pull_request: pull_request.as_deref(),
                pull_request_loading,
                hover_target: self.hover_target,
            },
        );
        buf.extend_from_slice(b"\x1b8");
        buf
    }

    pub(super) fn compose_partial_frame(&mut self, dirty_panes: HashSet<u64>) -> Vec<u8> {
        if dirty_panes.is_empty() {
            return Vec::new();
        }
        if self.dialog_open() || self.selection.is_some() {
            // Prefer the Ratatui diff path so dialog state that hasn't
            // changed produces an empty diff instead of a full fill_screen.
            // The raw-ANSI fallback is kept for the (rare) case where the
            // Ratatui terminal fails to draw.
            if let Some(ratatui_output) = self.compose_ratatui_frame() {
                let mut out = Vec::with_capacity(ratatui_output.len() + 64);
                self.append_outer_terminal_title(&mut out);
                out.extend_from_slice(&ratatui_output);
                return out;
            }
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
                let cache = self.pane_body_caches.entry(pane.id).or_default();
                let stats =
                    render_capsule_pane_body_partial(&mut buf, cache, pane, session.screen());
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
