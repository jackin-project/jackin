//! Compositor methods for the Multiplexer.
//!
//! Moved from daemon.rs to separate this concern from session lifecycle and
//! input dispatch. All methods are impl Multiplexer blocks.

use std::time::Instant;

use crate::tui::{
    app::{VisibleAgentState, visible_agent_state_from_protocol},
    socket_backend::SgrMetadata,
};

use super::{
    CursorVisibilityState, FullRedrawReason, Multiplexer, Rect, append_osc_window_title,
    compose_outer_terminal_title, cursor_visible_for_state, session_display_title,
};

/// Client terminal state the encoder asserted with the last frame. The
/// reconciliation in `append_client_state_reconciliation` diffs the desired
/// state (derived fresh from the focused pane's grid every frame) against
/// this and emits only the transitions — replacing the three hand-maintained
/// mode lists (`current_mode_state`, `drain_mode_transitions`,
/// `focus_swap_reset`) with one derivation (§3.4 of the capsule rendering
/// plan).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct AssertedClientState {
    pub(super) bracketed_paste: bool,
    pub(super) application_cursor: bool,
    pub(super) kitty_flags: u32,
    pub(super) cursor_visible: bool,
    /// DECSCUSR style (`0` = terminal default).
    pub(super) cursor_style: u16,
}

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
            // Terminal::clear() emits the screen erase and resets Ratatui's
            // previous buffer so FirstAttach/Resize get a real baseline reset.
            drop(self.ratatui_terminal.clear());
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
    /// selection, dialog, and bottom chrome when open. Cursor/mode state is
    /// reconciled after cell output as frame state.
    ///
    /// Returns the ANSI output to send to the attach client, or `None` if the
    /// Ratatui terminal fails to draw (the caller then skips the frame).
    fn compose_ratatui_frame(&mut self) -> Option<Vec<u8>> {
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
        let palette_key_glyph = self.status_bar.palette_key_glyph.as_deref();
        // Lay out row 0 once per frame. The owned plan is shared with the
        // status-bar widget (paint), the tab tooltip, and the click-region
        // refresh below, so the bar is never laid out more than once per frame.
        let status_plan = crate::tui::components::status_bar::status_bar_plan(
            term_cols,
            tabs,
            active_tab,
            &session_states,
            prefix_mode,
            palette_key_glyph,
        );
        let hover_target = self.hover_target;
        let hovered_tab = crate::tui::view::hovered_tab(hover_target);
        let menu_hovered = crate::tui::view::hovered_menu(hover_target);
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
                    (pane.id, s.scrollback_offset(), filled)
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
            .is_some_and(|s| s.scrollback_offset() != 0);
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
                        .scrollback_view(s.scrollback_offset(), pane.inner.rows);
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
                    actual_filled.saturating_sub(session.scrollback_offset().min(actual_filled));
                let cursor_visible = cursor_visible_for_state(CursorVisibilityState {
                    dialog_open,
                    focused_pane_available: focused_id == Some(pane.id),
                    focused_session_received_output: session.received_output,
                    scrollback_active: session.scrollback_offset() != 0,
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
                    session.scrollback_offset(),
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
        let debug_run_id_owned: Option<String> = if crate::logging::debug_enabled() {
            let diag = crate::container_context::resolve_container_diagnostics();
            (!diag.run_id.is_empty()).then_some(diag.run_id)
        } else {
            None
        };
        let palette_key = self.input_parser.palette_key().unwrap_or(0x1C);
        let branch = self.context_bar_branch().map(str::to_owned);
        let pull_request = self.pull_request_context.clone();
        let pull_request_loading = self.pull_request_context_loading();
        let spawn_failure = self.spawn_failure.clone();

        // Frame hyperlink layer (§3.4): the encoder brackets exactly these
        // cells with OSC 8 during emission — no raw overlay writes.
        let mut hyperlink_regions = pane_hyperlink_regions(&panes, &pane_screens, &self.sessions);
        let ui_hyperlink_regions =
            if let Some((DialogRatatuiSnapshot::DebugInfo(state), (row, col, height, width))) =
                dialog_snapshot.as_ref()
            {
                let area = ratatui::layout::Rect {
                    x: *col,
                    y: *row,
                    width: *width,
                    height: *height,
                };
                jackin_tui::components::container_info_hyperlink_regions(area, state)
            } else {
                Vec::new()
            };
        hyperlink_regions.extend(ui_hyperlink_regions);
        self.ratatui_terminal
            .backend_mut()
            .set_hyperlink_regions(hyperlink_regions);
        let sgr_regions = pane_sgr_regions(&panes, &pane_screens);
        self.ratatui_terminal
            .backend_mut()
            .set_sgr_regions(sgr_regions);

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
                    branch: branch.as_deref(),
                    pull_request: pull_request.as_deref(),
                    pull_request_loading,
                    instance_id_label: self.status_bar.instance_id_label(),
                    hover_target,
                    scrollback_active,
                    main_scroll_axes,
                    debug_run_id: debug_run_id_owned.as_deref(),
                    dialog_hint_spans: dialog_hint_spans.as_deref(),
                    spawn_failure: spawn_failure.as_deref(),
                    palette_key,
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
                let focused_pane_rect = panes.iter().find(|p| p.focused).map(|p| p.inner);
                self.append_client_state_reconciliation(&mut output, focused_id, focused_pane_rect);
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

    /// Reconcile the client terminal's cursor and mode state with the
    /// focused pane's grid — the frame model's non-cell payload (§3.4).
    /// Desired state is derived fresh every frame; only transitions against
    /// the last asserted state are emitted, except the cursor position +
    /// show, which must be re-asserted whenever visible because Ratatui's
    /// draw hides the cursor at the start of every frame.
    pub(super) fn append_client_state_reconciliation(
        &mut self,
        buf: &mut Vec<u8>,
        focused_id: Option<u64>,
        focused_pane_rect: Option<Rect>,
    ) {
        let dialog_open = self.dialog_open();
        let focused = focused_id.and_then(|id| self.sessions.get(&id));
        let desired = AssertedClientState {
            bracketed_paste: focused.is_some_and(|s| s.shadow_grid.bracketed_paste()),
            application_cursor: focused.is_some_and(|s| s.shadow_grid.application_cursor()),
            kitty_flags: focused.map_or(0, |s| s.shadow_grid.kitty_kb_flags()),
            cursor_style: focused.map_or(0, |s| s.shadow_grid.cursor_style()),
            cursor_visible: match (focused, focused_pane_rect) {
                (Some(session), Some(_)) => cursor_visible_for_state(CursorVisibilityState {
                    dialog_open,
                    focused_pane_available: true,
                    focused_session_received_output: session.received_output,
                    scrollback_active: session.scrollback_offset() != 0,
                    agent_cursor_hidden: session.shadow_grid.hide_cursor(),
                }),
                _ => false,
            },
        };
        let last = self.last_asserted_client_state;
        if last.is_none_or(|l| l.bracketed_paste != desired.bracketed_paste) {
            buf.extend_from_slice(if desired.bracketed_paste {
                b"\x1b[?2004h"
            } else {
                b"\x1b[?2004l"
            });
        }
        if last.is_none_or(|l| l.application_cursor != desired.application_cursor) {
            buf.extend_from_slice(if desired.application_cursor {
                b"\x1b[?1h"
            } else {
                b"\x1b[?1l"
            });
        }
        if last.is_none_or(|l| l.cursor_style != desired.cursor_style) {
            // DECSCUSR per pane: the focused pane's requested cursor shape
            // flows through the same reconciliation as every other mode, so
            // one pane's shape can never leak into another (D5).
            use std::io::Write as _;
            let _unused = write!(buf, "\x1b[{} q", desired.cursor_style);
        }
        if last.is_none_or(|l| l.kitty_flags != desired.kitty_flags) {
            // Pop whatever the previous pane pushed, then push the desired
            // level — the same pop+push shape the focus-swap reset used, so
            // the outer terminal's kitty stack depth stays bounded.
            buf.extend_from_slice(b"\x1b[<u");
            if desired.kitty_flags != 0 {
                use std::io::Write as _;
                let _unused = write!(buf, "\x1b[>{}u", desired.kitty_flags);
            }
        }
        if desired.cursor_visible {
            // Position at the focused pane's VT cursor in screen space, then
            // show. Re-asserted every frame: Ratatui's draw hid the cursor.
            if let (Some(session), Some(rect)) = (focused, focused_pane_rect) {
                let (vt_row, vt_col) = session.shadow_grid.cursor_position();
                use std::io::Write as _;
                let _unused = write!(
                    buf,
                    "\x1b[{};{}H",
                    rect.row + vt_row + 1,
                    rect.col + vt_col + 1
                );
                buf.extend_from_slice(b"\x1b[?25h");
            }
        } else if last.is_none_or(|l| l.cursor_visible) {
            // Hidden, and either never asserted or previously visible. The
            // draw already emitted ?25l this frame; this keeps the asserted
            // record explicit for the first frame after attach.
            buf.extend_from_slice(b"\x1b[?25l");
        }
        self.last_asserted_client_state = Some(desired);
    }
}

fn pane_hyperlink_regions(
    panes: &[crate::tui::app::VisiblePane],
    pane_screens: &[(u64, crate::tui::view::PaneScreen<'_>)],
    sessions: &std::collections::HashMap<u64, crate::session::Session>,
) -> Vec<(ratatui::layout::Rect, String)> {
    let mut regions = Vec::new();
    for pane in panes {
        let Some(session) = sessions.get(&pane.id) else {
            continue;
        };
        if !session.allow_frame_hyperlinks() {
            continue;
        }
        let Some((_, crate::tui::view::PaneScreen::View(view))) =
            pane_screens.iter().find(|(id, _)| *id == pane.id)
        else {
            continue;
        };
        for row in 0..pane.inner.rows.min(view.rows) {
            let mut col = 0;
            while col < pane.inner.cols.min(view.cols) {
                let uri = view
                    .cell(row, col)
                    .and_then(|cell| cell.hyperlink.as_ref())
                    .map(|link| link.uri.as_str())
                    .filter(|uri| crate::session::osc8_uri_is_safe(uri));
                let Some(uri) = uri else {
                    col += 1;
                    continue;
                };
                let start = col;
                col += 1;
                while col < pane.inner.cols.min(view.cols)
                    && view
                        .cell(row, col)
                        .and_then(|cell| cell.hyperlink.as_ref())
                        .is_some_and(|link| link.uri == uri)
                {
                    col += 1;
                }
                regions.push((
                    ratatui::layout::Rect {
                        x: pane.inner.col + start,
                        y: pane.inner.row + row,
                        width: col - start,
                        height: 1,
                    },
                    uri.to_owned(),
                ));
            }
        }
    }
    regions
}

fn pane_sgr_regions(
    panes: &[crate::tui::app::VisiblePane],
    pane_screens: &[(u64, crate::tui::view::PaneScreen<'_>)],
) -> Vec<(ratatui::layout::Rect, SgrMetadata)> {
    let mut regions = Vec::new();
    for pane in panes {
        let Some((_, crate::tui::view::PaneScreen::View(view))) =
            pane_screens.iter().find(|(id, _)| *id == pane.id)
        else {
            continue;
        };
        for row in 0..pane.inner.rows.min(view.rows) {
            let mut col = 0;
            while col < pane.inner.cols.min(view.cols) {
                let metadata = view
                    .cell(row, col)
                    .map(cell_sgr_metadata)
                    .filter(|metadata| *metadata != SgrMetadata::default());
                let Some(metadata) = metadata else {
                    col += 1;
                    continue;
                };
                let start = col;
                col += 1;
                while col < pane.inner.cols.min(view.cols)
                    && view
                        .cell(row, col)
                        .map(cell_sgr_metadata)
                        .is_some_and(|next| next == metadata)
                {
                    col += 1;
                }
                regions.push((
                    ratatui::layout::Rect {
                        x: pane.inner.col + start,
                        y: pane.inner.row + row,
                        width: col - start,
                        height: 1,
                    },
                    metadata,
                ));
            }
        }
    }
    regions
}

fn cell_sgr_metadata(cell: &jackin_term::Cell) -> SgrMetadata {
    SgrMetadata {
        underline_style: match cell.attrs.underline_style {
            jackin_term::UnderlineStyle::Single => jackin_term::UnderlineStyle::None,
            other => other,
        },
        underline_color: cell.attrs.underline_color,
        overline: cell.attrs.overline,
    }
}
