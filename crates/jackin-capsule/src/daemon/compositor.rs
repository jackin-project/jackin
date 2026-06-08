//! Compositor methods for the Multiplexer.
//!
//! Moved from daemon.rs to separate this concern from session lifecycle and
//! input dispatch. All methods are impl Multiplexer blocks.

use std::collections::HashSet;
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

enum FrameDamage {
    Full,
    Dirty,
}

impl Multiplexer {
    pub(super) fn compose_pending_frame(&mut self) -> Vec<u8> {
        let backend_size = self
            .ratatui_terminal
            .size()
            .map_or((0, 0), |s| (s.width, s.height));
        crate::cdebug!(
            "frame: full_redraw={:?} diff_redraw={:?} dirty_panes={} term={}x{} backend={}x{} content_rows={} dialog_open={}",
            self.pending_full_redraw.map(FullRedrawReason::as_str),
            self.pending_diff_redraw.map(FullRedrawReason::as_str),
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
        if let Some(reason) = self.pending_diff_redraw.take() {
            return self.compose_diff_frame(reason);
        }
        let dirty_panes = std::mem::take(&mut self.dirty_panes);
        self.compose_partial_frame(dirty_panes)
    }

    /// Single entry point for every full frame, regardless of which path
    /// requested it (initial pending-redraw, an interactive action, the chrome
    /// ticker, or an interactive action).
    /// Routing *all* full frames through here keeps one compositor in charge:
    /// the Ratatui `SocketBackend` paints status + panes and `compose_ratatui_frame`
    /// appends the raw bottom chrome + cursor. This is the only full-frame
    /// renderer — the legacy raw-ANSI `compose_full_frame` has been removed.
    pub(super) fn compose_full_redraw(&mut self, reason: FullRedrawReason) -> Vec<u8> {
        self.dirty_panes.clear();
        // Wipe + full repaint on every full frame. Terminal::clear() routes
        // through SocketBackend::clear_region(All) → `\x1b[2J\x1b[H`, then the
        // next draw re-emits every cell. This is deliberately heavier than a
        // cell diff: the diff alone leaves stale cells behind for high-frequency
        // alt-screen repainters (Claude Code, Amp) — dark-bg blocks and ghosted
        // rows the diff never overwrites because its baseline disagrees with the
        // terminal. A full repaint cannot desync, so it renders every agent
        // correctly. (The interaction flicker this reintroduces is tracked as a
        // follow-up; correctness wins over flicker.)
        drop(self.ratatui_terminal.clear());
        // The 2J wiped the bottom rows too; force the chrome to re-emit.
        self.last_bottom_chrome = None;
        let Some(ratatui_output) = self.compose_ratatui_frame(FrameDamage::Full) else {
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
    fn compose_ratatui_frame(&mut self, damage: FrameDamage) -> Option<Vec<u8>> {
        use crate::tui::components::dialog_widgets::DialogRatatuiSnapshot;
        use crate::tui::view::{CapsuleRatatuiFrame, PaneScreen, render_capsule_ratatui_frame};

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
        let damage_label = match damage {
            FrameDamage::Full => "full",
            FrameDamage::Dirty => "dirty",
        };
        crate::cdebug!(
            "render: ratatui-frame damage={} panes={} pane_screens={}",
            damage_label,
            panes.len(),
            pane_screens.len(),
        );

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
                    focus_owner,
                    zoomed,
                    dialog_open,
                    dialog_snapshot: dialog_snapshot.as_ref(),
                    pane_screens: &pane_screens,
                    sessions_state: &session_states,
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
                let mut output = Vec::new();
                self.ratatui_terminal
                    .backend_mut()
                    .drain_output_into(&mut output);
                drop(pane_screens);
                for (pane_id, _) in &pane_titles {
                    if let Some(session) = self.sessions.get_mut(pane_id) {
                        session.clear_pane_chrome_dirty();
                        session.clear_pane_body_repaint_pending();
                    }
                }
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

    pub(super) fn compose_dialog_overlay_frame(&mut self, reason: FullRedrawReason) -> Vec<u8> {
        self.compose_diff_frame(reason)
    }

    pub(super) fn compose_diff_frame(&mut self, reason: FullRedrawReason) -> Vec<u8> {
        // Prefer the Ratatui diff path: it only sends changed cells so a
        // dialog or selection whose state hasn't changed produces an empty or
        // near-empty diff instead of a full fill_screen + repaint. This keeps
        // high-frequency interaction repaint out of the clear tier.
        if let Some(ratatui_output) = self.compose_ratatui_frame(FrameDamage::Full) {
            crate::cdebug!(
                "render: kind=diff reason={} via=ratatui bytes={}",
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
        crate::clog!("compose_diff_frame: ratatui draw failed; skipping frame");
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
        // A dirty-pane frame (PTY output / hover / selection change) is a
        // Ratatui DIFF — NOT a full redraw. compose_ratatui_frame draws without
        // clearing the double-buffer, so the SocketBackend emits only the cells
        // that changed since the previous frame: no `\x1b[2J`, no full repaint.
        // This keeps streaming agent output incremental and flicker-free.
        // (compose_full_redraw, which clears the screen, is reserved for
        // geometry/layout changes that must wipe the previous layout.)
        if dirty_panes.is_empty() && !self.dialog_open() && self.selection.is_none() {
            return Vec::new();
        }
        let started = Instant::now();
        let dirty_pane_count = dirty_panes.len();
        if let Some(output) = self.compose_direct_dirty_pane_frame(&dirty_panes, started) {
            return output;
        }
        let damage = if self.selection.is_some() {
            FrameDamage::Full
        } else {
            FrameDamage::Dirty
        };
        let Some(ratatui_output) = self.compose_ratatui_frame(damage) else {
            crate::clog!("compose_partial_frame: ratatui draw failed; skipping frame");
            return Vec::new();
        };
        let mut buf = Vec::with_capacity(ratatui_output.len() + 64);
        self.append_outer_terminal_title(&mut buf);
        buf.extend_from_slice(&ratatui_output);
        crate::cdebug!(
            "render: kind=partial reason=pty-output dirty_panes={} bytes={} duration_us={}",
            dirty_pane_count,
            buf.len(),
            started.elapsed().as_micros()
        );
        buf
    }

    fn compose_direct_dirty_pane_frame(
        &mut self,
        dirty_panes: &HashSet<u64>,
        started: Instant,
    ) -> Option<Vec<u8>> {
        if dirty_panes.is_empty() || self.dialog_open() || self.selection.is_some() {
            return None;
        }
        let focused_id = self.active_focused_id()?;
        let visible_panes = self.visible_panes();
        let focused_rect = visible_panes
            .iter()
            .find(|pane| pane.id == focused_id)
            .map(|pane| pane.inner)?;
        if visible_panes
            .iter()
            .filter(|pane| dirty_panes.contains(&pane.id))
            .count()
            != dirty_panes.len()
        {
            return None;
        }

        for pane in &visible_panes {
            if !dirty_panes.contains(&pane.id) {
                continue;
            }
            let session = self.sessions.get(&pane.id)?;
            if session.scrollback_offset != 0
                || session.pane_chrome_dirty()
                || session.pane_body_repaint_pending()
            {
                return None;
            }
        }

        let alloc_before = crate::alloc_telemetry::snapshot();
        let mut changed_rows = 0;
        let mut changed_cells = 0;
        let mut max_grid_rows = 0;
        let mut max_grid_cols = 0;
        for pane in &visible_panes {
            if !dirty_panes.contains(&pane.id) {
                continue;
            }
            let area = ratatui::layout::Rect {
                x: pane.inner.col,
                y: pane.inner.row,
                width: pane.inner.cols,
                height: pane.inner.rows,
            };
            let session = self.sessions.get_mut(&pane.id)?;
            let dirty = session.shadow_grid.dirty_spans();
            let patch = session.shadow_grid.dirty_patch_from(dirty);
            changed_rows += patch.changed_row_count();
            changed_cells += patch.changed_cell_count();
            max_grid_rows = max_grid_rows.max(patch.rows);
            max_grid_cols = max_grid_cols.max(patch.cols);
            self.ratatui_terminal
                .backend_mut()
                .draw_grid_patch(area, &patch);
        }
        let alloc_delta = crate::alloc_telemetry::delta_since(alloc_before);
        let mut output = Vec::new();
        self.ratatui_terminal
            .backend_mut()
            .drain_output_into(&mut output);
        if let Some(delta) = alloc_delta {
            crate::cdebug!(
                "render_alloc: kind=partial reason=pty-output via=direct-grid-patch alloc_blocks={} alloc_bytes={} dirty_panes={} changed_rows={} changed_cells={} max_grid={}x{}",
                delta.blocks,
                delta.bytes,
                dirty_panes.len(),
                changed_rows,
                changed_cells,
                max_grid_rows,
                max_grid_cols,
            );
        }
        self.append_cursor_state(&mut output, Some(focused_id), Some(focused_rect));
        crate::cdebug!(
            "render: kind=partial reason=pty-output dirty_panes={} via=direct-grid-patch bytes={} duration_us={} changed_rows={} changed_cells={} max_grid={}x{}",
            dirty_panes.len(),
            output.len(),
            started.elapsed().as_micros(),
            changed_rows,
            changed_cells,
            max_grid_rows,
            max_grid_cols,
        );
        Some(output)
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
