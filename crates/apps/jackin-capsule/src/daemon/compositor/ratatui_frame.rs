// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Ratatui frame composition for the daemon-owned `Multiplexer`.

use std::{collections::HashSet, time::Instant};

use super::super::{Multiplexer, session_display_title};
use super::cached_pane_regions;

impl Multiplexer {
    /// Compose one frame of the full widget tree through the Ratatui
    /// `SocketBackend`: status bar, pane bodies, pane borders, scrollbars,
    /// selection, dialog, and bottom chrome when open. Cursor/mode state is
    /// reconciled after cell output as frame state.
    ///
    /// Returns the ANSI output to send to the attach client, or `None` if the
    /// Ratatui terminal fails to draw (the caller then skips the frame).
    #[expect(
        clippy::too_many_lines,
        reason = "Per-frame compositor that snapshots the multiplexer state and \
                  renders the resulting capsule frame to ANSI bytes. The inline \
                  shape preserves snapshot borrow scope across the snapshot → \
                  render → emit pipeline."
    )]
    pub(crate) fn compose_ratatui_frame(&mut self) -> Option<Vec<u8>> {
        use crate::tui::components::dialog_widgets::DialogRatatuiSnapshot;
        use crate::tui::view::{CapsuleRatatuiFrame, PaneScreen};

        let term_rows = self.render.term_rows;
        let term_cols = self.render.term_cols;
        let active_tab = self.session_supervisor.active_tab;
        // `focused_usage_snapshot()` returns an owned view; move the headline out
        // rather than cloning on the per-frame compose path.
        let usage_status_label = self.focused_usage_snapshot().status_bar_label;
        let tabs = &self.session_supervisor.tabs;
        let panes = self.visible_panes();
        let focused_id = self.active_focused_id();
        // P5: tab-bar focus is part of the one shared focus-ring model, not a
        // parallel signal. When the operator has moved focus to the tab bar the
        // owner is `TabBar` even while a pane is open — so the pane cursor hides
        // and the active-tab underline goes green through the same abstraction
        // that drives pane-border focus and cursor visibility. Otherwise the
        // owner follows the focused pane.
        let focus_owner = if self.render.tab_bar_focused {
            jackin_tui::runtime::SurfaceFocus::tab_bar(focused_id.unwrap_or_default())
        } else {
            focused_id.map_or(
                jackin_tui::runtime::SurfaceFocus::tab_bar(0),
                jackin_tui::runtime::SurfaceFocus::content,
            )
        };
        let zoomed = self.active_zoomed_id().is_some();
        let dialog_open = self.dialog_open();
        // Status-bar inputs snapshotted before the draw closure borrows self.
        let session_states = self.snapshot_session_states();
        let prefix_mode = self.status.status_bar.prefix_mode;
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
        let hover_target = self.render.hover_target;
        let hovered_tab = crate::tui::view::hovered_tab(hover_target);
        let menu_hovered = crate::tui::view::hovered_menu(hover_target);
        // Selection highlight is only meaningful in the unzoomed multi-pane
        // view; a zoom toggle cancels it, matching the raw path's gate.
        let selection = if zoomed {
            None
        } else {
            self.clipboard.selection
        };
        let selection_copied = self.clipboard.selection_copied;

        // Snapshot session display titles before the draw closure borrows self.
        let pane_titles: Vec<(u64, String)> = panes
            .iter()
            .filter_map(|pane| {
                self.session_supervisor
                    .sessions
                    .get(pane.id)
                    .map(|s| (pane.id, session_display_title(s)))
            })
            .collect();
        // Per-pane scrollbar inputs (offset, filled). get_mut because
        // scrollback_filled lazily counts; done before the immutable pane_screens
        // borrow below.
        let pane_scrollbars: Vec<(u64, usize, usize)> = panes
            .iter()
            .filter_map(|pane| {
                self.session_supervisor.sessions.get_mut(pane.id).map(|s| {
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
            let pr_branch = self.pr_watch.pull_request_context_branch.as_deref();
            let pr_info = self.pr_watch.pull_request_context.as_deref();
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
        let dialog_hint_spans: Option<Vec<termrock::widgets::HintSpan<'static>>> =
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
            .and_then(|id| self.session_supervisor.sessions.get(id))
            .is_some_and(|s| s.scrollback_offset() != 0);
        let main_scroll_axes = focused_id
            .and_then(|id| {
                let pane = panes.iter().find(|pane| pane.id == id)?;
                let (_, offset, filled) = pane_scrollbars.iter().find(|(sid, _, _)| *sid == id)?;
                let vertical = termrock::scroll::tail_vertical_thumb(
                    pane.outer.rows.saturating_sub(2),
                    *filled,
                    *offset,
                )
                .is_some();
                Some(termrock::scroll::ScrollAxes {
                    vertical,
                    horizontal: false,
                })
            })
            .unwrap_or_default();

        // Reset each visible grid's damage memory. Derived rendering still
        // paints a complete Ratatui frame; the drained spans only invalidate
        // cached per-pane metadata scans.
        let mut damaged_panes = HashSet::new();
        for pane in &panes {
            if let Some(session) = self.session_supervisor.sessions.get_mut(pane.id)
                && !session.shadow_grid.dirty_spans().is_empty()
            {
                damaged_panes.insert(pane.id);
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
                self.session_supervisor.sessions.get(pane.id).map(|s| {
                    let view = s
                        .shadow_grid
                        .scrollback_view(s.scrollback_offset(), pane.inner.rows);
                    (pane.id, PaneScreen::View(view))
                })
            })
            .collect();
        let debug_run_id_owned: Option<String> = if crate::logging::debug_enabled() {
            let diag = crate::container_context::resolve_container_diagnostics();
            (!diag.invocation_id.is_empty()).then_some(diag.invocation_id)
        } else {
            None
        };
        let branch = self.context_bar_branch().map(str::to_owned);
        let pull_request = self.pr_watch.pull_request_context.clone();
        let pull_request_loading = self.pull_request_context_loading();
        let palette_key = self.control.input_parser.palette_key().unwrap_or(0x1C);
        let clipboard_image_notice = self.clipboard.clipboard_image_notice.clone();
        let link_hover_notice = self
            .render
            .link_hover_url
            .as_ref()
            .map(|url| format!("Open link: {url}"));

        // Frame hyperlink layer (§3.4): the encoder brackets exactly these
        // cells with OSC 8 during emission — no raw overlay writes.
        let (mut hyperlink_regions, sgr_regions) = cached_pane_regions(
            &mut self.render.pane_region_cache,
            &panes,
            &pane_screens,
            &self.session_supervisor.sessions,
            &damaged_panes,
            focused_id,
        );
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
                crate::tui::components::container_info_surface::container_info_hyperlink_regions(
                    area, state,
                )
            } else {
                Vec::new()
            };
        hyperlink_regions.extend(ui_hyperlink_regions);
        self.render
            .ratatui_terminal
            .backend_mut()
            .set_hyperlink_regions(hyperlink_regions);
        self.render
            .ratatui_terminal
            .backend_mut()
            .set_sgr_regions(sgr_regions);

        let frame_model = CapsuleRatatuiFrame {
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
            usage_status_label: Some(usage_status_label.as_str()),
            pull_request: pull_request.as_deref(),
            pull_request_loading,
            instance_id_label: self.status.status_bar.instance_id_label(),
            hover_target,
            scrollback_active,
            main_scroll_axes,
            debug_run_id: debug_run_id_owned.as_deref(),
            dialog_hint_spans: dialog_hint_spans.as_deref(),
            palette_key,
            clipboard_image_notice: clipboard_image_notice.as_deref(),
            link_hover_notice: link_hover_notice.as_deref(),
        };
        let area = ratatui::layout::Rect {
            x: 0,
            y: 0,
            width: term_cols,
            height: term_rows,
        };
        // Shared product drive_frame — CapsuleView is the View adapter.
        let render_started = Instant::now();
        let result = jackin_tui::runtime::drive_frame(
            &mut self.render.ratatui_terminal,
            &crate::tui::runtime::CapsuleView,
            &frame_model,
            area,
            |_| {},
        );
        jackin_telemetry::ui::record_render(
            jackin_telemetry::schema::enums::ScreenId::Capsule,
            render_started.elapsed().as_secs_f64(),
        );

        // Keep tab/menu click regions in sync with the columns the widget
        // just painted, from the same plan the widget rendered, so hit-testing
        // is correct after a Ratatui frame without re-laying out the bar.
        self.status
            .status_bar
            .set_click_regions_from_plan(&status_plan);

        if result.is_ok() {
            let mut output = Vec::new();
            self.render
                .ratatui_terminal
                .backend_mut()
                .drain_output_into(&mut output);
            drop(pane_screens);
            let focused_pane_rect = panes.iter().find(|p| p.focused).map(|p| p.inner);
            self.append_client_state_reconciliation(&mut output, focused_id, focused_pane_rect);
            Some(output)
        } else {
            let _error =
                jackin_telemetry::record_error(jackin_telemetry::schema::enums::ErrorType::IoError);
            None
        }
    }
}
