// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Main frame composition: status bar, panes, overlays, and bottom chrome.

use super::{
    CapsuleRatatuiFrame, PaneScreen, apply_tab_codename_tooltip, render_clipboard_image_notice,
    render_link_hover_notice, render_notice_toast,
};

use crate::tui::components::chrome::{PaneBorderWidget, StatusBarWidget};
use crate::tui::components::dialog_widgets::render_dialog_ratatui;
use crate::tui::components::pane::PaneBodyWidget;
use crate::tui::layout::{self};
use crate::tui::model::VisiblePane;

use ratatui::{Frame, layout::Rect as RatatuiRect, style::Modifier};

/// Paint the scrollback scrollbar on a pane's right border through the shared
/// `scrollable_panel` component — `┃` thumb on a `·` track in the shared
/// dialog-scrollbar colors, glyph-identical to every other scrollbar in
/// jackin❯. The pane's tail-relative offset is bridged to the component's
/// top-relative offset via `TailScroll::to_top_offset` over the same
/// `filled + interior` content length `tail_vertical_thumb` uses, so wheel
/// scrolling and the painted thumb can never disagree.
pub(crate) fn apply_pane_scrollbar(
    frame: &mut Frame<'_>,
    pane: &VisiblePane,
    offset: usize,
    filled: usize,
) {
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
    let top_offset = u16::try_from(
        termrock::scroll::TailScroll::new(offset)
            .to_top_offset(content_len, interior_rows)
            .min(usize::from(u16::MAX)),
    )
    .unwrap_or(u16::MAX);
    let theme = termrock::style::DesignSystem::default();
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
pub(crate) fn apply_selection_highlight(
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

pub(crate) fn selection_toast_area(view: &CapsuleRatatuiFrame<'_>) -> RatatuiRect {
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
            // The tab underline reads the one shared focus ring, the same
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
        // Head's default backdrop is a stippled dim wash; pre-bump jackin❯
        // painted the terminal background under modals. `reset()` keeps that
        // product behavior (plan 003 owns any paint compensation).
        frame.render_widget(termrock::widgets::Backdrop::reset(), backdrop_area);
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
