// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Rendering helper types and functions for the capsule multiplexer.

use crate::pull_request::PullRequestInfo;
use crate::tui::components::chrome::{PaneBorderWidget, StatusBarWidget};
use crate::tui::components::dialog_widgets::{DialogRatatuiSnapshot, render_dialog_ratatui};
use crate::tui::components::pane::PaneBodyWidget;
use crate::tui::layout::{self, Tab};
use crate::tui::model::{HoverTarget, VisiblePane};
use ratatui::{Frame, layout::Rect as RatatuiRect, style::Modifier};
use termrock::interaction::FocusOwner;

pub(crate) const fn hovered_tab(target: Option<HoverTarget>) -> Option<usize> {
    match target {
        Some(HoverTarget::Tab(idx)) => Some(idx),
        _ => None,
    }
}

pub(crate) const fn hovered_menu(target: Option<HoverTarget>) -> bool {
    matches!(target, Some(HoverTarget::Menu))
}

/// Dialog snapshot with its bounding rect — factored out to keep `CapsuleRatatuiFrame` readable.
pub(crate) type DialogFrameSnapshot = (DialogRatatuiSnapshot, (u16, u16, u16, u16));

#[derive(Debug)]
pub(crate) enum PaneScreen<'a> {
    View(jackin_term::GridView<'a>),
}

#[expect(
    clippy::struct_excessive_bools,
    reason = "Six orthogonal render-state flags on the per-frame snapshot \
              (zoomed, dialog_open, menu_hovered, selection_copied, \
              pull_request_loading, scrollback_active) — each tracks an \
              independent UI state consumed individually by the compositor \
              branches. Named-field reads match the per-branch dispatch idiom \
              this snapshot feeds."
)]
#[derive(Clone)]
pub(crate) struct CapsuleRatatuiFrame<'a> {
    pub(crate) tabs: &'a [Tab],
    /// Row-0 layout computed once per frame and shared by the status-bar
    /// widget (paint), the tab tooltip, and the compositor's click-region
    /// refresh, so the bar is laid out once rather than per consumer.
    pub(crate) status_plan: &'a crate::tui::components::status_bar::StatusBarPlan,
    pub(crate) term_cols: u16,
    pub(crate) term_rows: u16,
    pub(crate) panes: &'a [VisiblePane],
    pub(crate) pane_titles: &'a [(u64, String)],
    pub(crate) focus_owner: FocusOwner<u64>,
    pub(crate) zoomed: bool,
    pub(crate) dialog_open: bool,
    pub(crate) dialog_snapshot: Option<&'a DialogFrameSnapshot>,
    pub(crate) pane_screens: &'a [(u64, PaneScreen<'a>)],
    pub(crate) prefix_mode: crate::tui::components::status_bar::PrefixMode,
    pub(crate) hovered_tab: Option<usize>,
    pub(crate) menu_hovered: bool,
    pub(crate) selection: Option<crate::tui::selection::SelectionState>,
    pub(crate) selection_copied: bool,
    /// Per-pane scrollbar inputs `(session_id, offset, filled)`. A pane with
    /// `filled > 0` gets a thumb painted on its right border.
    pub(crate) scrollbars: &'a [(u64, usize, usize)],
    pub(crate) branch: Option<&'a str>,
    pub(crate) usage_status_label: Option<&'a str>,
    pub(crate) pull_request: Option<&'a PullRequestInfo>,
    pub(crate) pull_request_loading: bool,
    pub(crate) instance_id_label: &'a str,
    pub(crate) hover_target: Option<HoverTarget>,
    pub(crate) scrollback_active: bool,
    pub(crate) main_scroll_axes: termrock::scroll::ScrollAxes,
    pub(crate) debug_run_id: Option<&'a str>,
    pub(crate) dialog_hint_spans: Option<&'a [termrock::widgets::HintSpan<'a>]>,
    /// Resolved palette-key byte (`InputParser::palette_key().unwrap_or(0x1C)`).
    /// Forwarded to the hint builder so the palette-key glyph reflects the
    /// operator's `JACKIN_PALETTE_KEY` setting.
    pub(crate) palette_key: u8,
    /// Transient host clipboard image paste result. Painted in the content
    /// toast area so it cannot overwrite status rows or bottom chrome.
    pub(crate) clipboard_image_notice: Option<&'a str>,
    /// Host-open target under an Alt/Ctrl hover in a mouse-disabled pane.
    /// Painted through the compositor so the PTY never receives hover text.
    pub(crate) link_hover_notice: Option<&'a str>,
}

/// Paint the scrollback scrollbar on a pane's right border through the shared
/// `scrollable_panel` component — `┃` thumb on a `·` track in the shared
/// dialog-scrollbar colors, glyph-identical to every other scrollbar in
/// jackin❯. The pane's tail-relative offset is bridged to the component's
/// top-relative offset via `TailScroll::to_top_offset` over the same
/// `filled + interior` content length `tail_vertical_thumb` uses, so wheel
/// scrolling and the painted thumb can never disagree.
fn apply_pane_scrollbar(frame: &mut Frame<'_>, pane: &VisiblePane, offset: usize, filled: usize) {
    if pane.outer.cols == 0 || pane.outer.rows < 3 {
        return;
    }
    let border_area = RatatuiRect {
        x: pane.outer.col,
        y: pane.outer.row,
        width: pane.outer.cols,
        height: pane.outer.rows,
    };
    let track = termrock::scroll::vertical_scrollbar_area(border_area);
    let interior_rows = usize::from(track.height);
    let content_len = filled.saturating_add(interior_rows);
    let top_offset = termrock::scroll::TailScroll::new(offset)
        .to_top_offset(content_len, interior_rows)
        .min(usize::from(u16::MAX)) as u16;
    let theme = termrock::Theme::default();
    termrock::scroll::render_scrollbar(
        frame.buffer_mut(),
        track,
        termrock::scroll::ScrollbarSpec::new(
            termrock::scroll::ScrollAxis::Vertical,
            termrock::scroll::ScrollbarGeometry::new(content_len, interior_rows, top_offset),
        ),
        &theme,
    );
}

/// Overlay the inverse-video selection highlight onto the cells the pane
/// bodies already painted. The Ratatui equivalent of
/// `paint_selection_highlight`: it toggles `REVERSED` on the selected cells so
/// the `SocketBackend` diff carries it, instead of a raw post-frame append.
fn apply_selection_highlight(
    buf: &mut ratatui::buffer::Buffer,
    sel: &crate::tui::selection::SelectionState,
    scrollback_filled: usize,
    scrollback_offset: usize,
) {
    let Some(visible) =
        crate::tui::selection::visible_selection(sel, scrollback_filled, scrollback_offset)
    else {
        return;
    };
    let inner = visible.inner;
    for r in visible.start_row..=visible.end_row {
        let from_col = if r == visible.start_row {
            visible.start_col
        } else {
            0
        };
        let to_col = if r == visible.end_row {
            visible.end_col
        } else {
            inner.cols.saturating_sub(1)
        };
        if to_col < from_col {
            continue;
        }
        let y = inner.row + r;
        for c in from_col..=to_col {
            let x = inner.col + c;
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.modifier |= Modifier::REVERSED;
            }
        }
    }
}

fn selection_toast_area(view: &CapsuleRatatuiFrame<'_>) -> RatatuiRect {
    RatatuiRect::new(
        0,
        crate::tui::components::status_bar::STATUS_BAR_ROWS,
        view.term_cols,
        layout::available_content_rows(view.term_rows),
    )
}

pub(crate) fn render_capsule_ratatui_frame(frame: &mut Frame<'_>, view: CapsuleRatatuiFrame<'_>) {
    let status_area = RatatuiRect {
        x: 0,
        y: 0,
        width: view.term_cols,
        height: crate::tui::components::status_bar::STATUS_BAR_ROWS,
    };
    frame.render_widget(
        StatusBarWidget {
            plan: view.status_plan,
            prefix_mode: view.prefix_mode,
            hovered_tab: view.hovered_tab,
            menu_hovered: view.menu_hovered,
            // the tab underline reads the one shared FocusOwner, the same
            // signal that drives pane-border focus and cursor visibility.
            focused: view.focus_owner.is_tab_bar(),
        },
        status_area,
    );

    // A modal owns the pane region, but the persistent status bar remains
    // visible and interactive above it.
    if view.dialog_open {
        let backdrop_area = RatatuiRect {
            x: 0,
            y: crate::tui::components::status_bar::STATUS_BAR_ROWS,
            width: view.term_cols,
            height: view
                .term_rows
                .saturating_sub(crate::tui::components::status_bar::STATUS_BAR_ROWS),
        };
        frame.render_widget(termrock::widgets::Backdrop::default(), backdrop_area);
        if let Some((snapshot, rect)) = view.dialog_snapshot {
            render_dialog_ratatui(frame, *rect, snapshot);
        }
        frame.render_widget(
            crate::tui::components::chrome::DialogBottomChromeWidget {
                branch: view.branch,
                usage_status_label: view.usage_status_label,
                pull_request: view.pull_request,
                pull_request_loading: view.pull_request_loading,
                debug_run_id: view.debug_run_id,
                instance_id_label: view.instance_id_label,
                hint_spans: view.dialog_hint_spans,
            },
            frame.area(),
        );
        render_clipboard_image_notice(frame, &view);
        return;
    }

    for pane in view.panes {
        let title = view
            .pane_titles
            .iter()
            .find(|(id, _)| *id == pane.id)
            .map_or("", |(_, t)| t.as_str());

        let focused = view.focus_owner.show_cursor_for(&pane.id);
        let border_area = RatatuiRect {
            x: pane.outer.col,
            y: pane.outer.row,
            width: pane.outer.cols,
            height: pane.outer.rows,
        };
        frame.render_widget(
            PaneBorderWidget {
                title: title.to_owned(),
                focused: focused && !view.zoomed,
            },
            border_area,
        );

        if let Some((_, screen)) = view
            .pane_screens
            .iter()
            .find(|(session_id, _)| *session_id == pane.id)
        {
            let body_area = RatatuiRect {
                x: pane.inner.col,
                y: pane.inner.row,
                width: pane.inner.cols,
                height: pane.inner.rows,
            };
            match screen {
                PaneScreen::View(view) => {
                    frame.render_widget(PaneBodyWidget::view(view), body_area);
                }
            }
        }
    }

    // Per-pane scrollback scrollbars on the right border. Retained scrollback
    // is enough to show the bar, even at the live tail; the shared tail-scroll
    // geometry places the thumb at the bottom for offset 0.
    for pane in view.panes {
        if let Some(&(_, offset, filled)) = view.scrollbars.iter().find(|(id, _, _)| *id == pane.id)
            && filled > 0
        {
            apply_pane_scrollbar(frame, pane, offset, filled);
        }
    }

    // Selection highlight is overlaid after the pane bodies so the agent's
    // glyphs survive underneath the reversed-colour cue.
    if let Some(sel) = view.selection
        && let Some(&(_, offset, filled)) = view
            .scrollbars
            .iter()
            .find(|(id, _, _)| *id == sel.session_id)
    {
        apply_selection_highlight(frame.buffer_mut(), &sel, filled, offset);
    }
    if view.selection_copied {
        render_notice_toast(frame, selection_toast_area(&view), "Selection copied");
    }
    render_clipboard_image_notice(frame, &view);
    render_link_hover_notice(frame, &view);

    // Tab hover tooltip: codename pill painted one row below the hovered tab
    // cell, overlaid after pane bodies so it reads as a contextual label.
    // Hover enter/leave triggers a full redraw (see update_hover_for_mouse),
    // so the overlaid row is repainted clean when the operator moves away.
    if let Some(idx) = view.hovered_tab
        && let Some(tab) = view.tabs.get(idx)
    {
        apply_tab_codename_tooltip(frame.buffer_mut(), view.status_plan, idx, &tab.codename);
    }

    // Bottom chrome rides the cell buffer like every other widget — one
    // compositor owns the whole frame, no raw appends, no byte cache.
    frame.render_widget(
        crate::tui::components::chrome::BottomChromeWidget {
            branch: view.branch,
            usage_status_label: view.usage_status_label,
            pull_request: view.pull_request,
            pull_request_loading: view.pull_request_loading,
            instance_id_label: view.instance_id_label,
            hover_target: view.hover_target,
            scrollback_active: view.scrollback_active,
            scroll_axes: view.main_scroll_axes,
            debug_run_id: view.debug_run_id,
            prefix_awaiting: view.prefix_mode
                == crate::tui::components::status_bar::PrefixMode::Awaiting,
            palette_key: view.palette_key,
        },
        frame.area(),
    );
}

fn render_clipboard_image_notice(frame: &mut Frame<'_>, view: &CapsuleRatatuiFrame<'_>) {
    if let Some(notice) = view.clipboard_image_notice {
        render_notice_toast(frame, selection_toast_area(view), notice);
    }
}

fn render_link_hover_notice(frame: &mut Frame<'_>, view: &CapsuleRatatuiFrame<'_>) {
    if view.clipboard_image_notice.is_some() {
        return;
    }
    if let Some(notice) = view.link_hover_notice {
        render_notice_toast(frame, selection_toast_area(view), notice);
    }
}

fn render_notice_toast(frame: &mut Frame<'_>, area: RatatuiRect, message: &str) {
    let theme = termrock::Theme::default();
    // Full TermRock toast contract: severity border role, bottom-left anchor
    // under the status strip, and theme-derived text — no product local chrome.
    frame.render_widget(
        termrock::widgets::Toast::new(&theme, message, termrock::widgets::Severity::Success)
            .anchor(termrock::widgets::Anchor::BottomLeft)
            .margins(1, 0),
        area,
    );
}

/// Paint the hovered tab's codename as a dark-bg + phosphor-green pill on the
/// row directly below the tab strip, left-aligned with the tab cell. Ratatui
/// `Buffer::set_string` clips to the buffer area, so an out-of-range column or
/// a too-long codename cannot overflow the frame.
fn apply_tab_codename_tooltip(
    buf: &mut ratatui::buffer::Buffer,
    plan: &crate::tui::components::status_bar::StatusBarPlan,
    hovered_idx: usize,
    codename: &str,
) {
    use ratatui::style::{Modifier, Style};
    let Some(cell) = plan.cells.get(hovered_idx) else {
        return;
    };
    // Row index 2 (0-based): one row below the tab strip (row 0) and the
    // active-tab underline (row 1).
    let tooltip_row = crate::tui::components::status_bar::STATUS_BAR_ROWS;
    let pill = format!(" {codename} ");
    buf.set_string(
        cell.start_col0,
        tooltip_row,
        &pill,
        Style::default()
            .bg(termrock::Theme::default()
                .style(termrock::style::Role::TabInactive)
                .bg
                .unwrap_or_default())
            .fg(termrock::Theme::default()
                .style(termrock::style::Role::Accent)
                .fg
                .unwrap_or_default())
            .add_modifier(Modifier::BOLD),
    );
}

/// Format a `label: error` string.
pub(crate) fn spawn_failure_message(agent_label: &str, error: impl std::fmt::Display) -> String {
    format!("{agent_label}: {error:#}")
}

pub(crate) fn spawn_failure_agent_label(agent_slug: Option<&str>) -> &str {
    agent_slug.unwrap_or("shell")
}

pub(crate) fn spawn_request_failure_message(
    request_label: &str,
    error: impl std::fmt::Display,
) -> String {
    format!("spawn {request_label} failed: {error:#}")
}

pub(crate) fn tab_limit_failure_message(max_tabs: usize) -> String {
    format!("tab limit reached ({max_tabs}); close one before spawning another")
}

pub(crate) fn pane_limit_failure_message(max_sessions: usize) -> String {
    format!("pane limit reached ({max_sessions}); close some panes before opening more")
}

/// Forwarded to the operator's outer terminal via `send_output` from the
/// `CopyToClipboard` dialog action. The OSC 52 byte encoding and terminal
/// compatibility notes live with the canonical typed `TermRock` encoder.
pub(crate) fn encode_osc52_clipboard_write(payload: &str) -> Vec<u8> {
    termrock::osc::encode_clipboard(termrock::osc::ClipboardWrite {
        selection: termrock::osc::ClipboardSelection::Clipboard,
        text: payload,
    })
}

#[cfg(test)]
mod tests;
