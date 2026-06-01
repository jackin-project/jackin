//! Rendering helper types and functions for the capsule multiplexer.

use crate::tui::app::{HoverTarget, VisibleAgentState, VisiblePane};
use crate::tui::components::branch_context_bar::{
    BRANCH_CONTEXT_BAR_ROWS, render_branch_context_bar,
};
use crate::tui::layout::Tab;
use crate::tui::render::{
    PaneBodyCache, PaneBodyRenderStats, RowSnapshot, draw_scrollbar, fill_screen,
};
use crate::pull_request::PullRequestInfo;
use crate::tui::selection::{SelectionState, paint_selection_highlight};
use crate::tui::components::status_bar::{StatusBar, draw_pane_box};
use crate::tui::components::chrome::{DialogBackdrop, PaneBorderWidget, StatusBarWidget};
use crate::tui::components::dialog::{Dialog, GithubContextView};
use crate::tui::components::dialog_widgets::{DialogRatatuiSnapshot, render_dialog_ratatui};
use crate::tui::components::pane::PaneBodyWidget;
use ratatui::{Frame, layout::Rect as RatatuiRect};

pub(crate) const fn hovered_tab(target: Option<HoverTarget>) -> Option<usize> {
    match target {
        Some(HoverTarget::Tab(idx)) => Some(idx),
        _ => None,
    }
}

pub(crate) const fn hovered_menu(target: Option<HoverTarget>) -> bool {
    matches!(target, Some(HoverTarget::Menu))
}

#[derive(Default)]
pub(crate) struct PaneScrollbar {
    pub(crate) offset: usize,
    pub(crate) filled: usize,
}

impl PaneScrollbar {
    pub(crate) const fn visible(&self) -> bool {
        self.filled > 0
    }
}

/// Draw the pane box and optional scrollbar for one visible pane.
///
/// Called identically from compose_full_frame and compose_partial_frame;
/// lives here so both compositors stay in lock-step when the chrome rules
/// change.
pub(crate) fn render_capsule_pane_chrome(
    buf: &mut Vec<u8>,
    pane: &VisiblePane,
    title: &str,
    scrollbar: PaneScrollbar,
    zoomed: bool,
    multi_pane: bool,
) {
    // Focused-border highlight: show the bright focus ring when the
    // operator must look at this pane to understand scroll state.
    let highlight_focus = if zoomed {
        scrollbar.visible()
    } else {
        multi_pane || scrollbar.visible()
    };
    draw_pane_box(
        buf,
        pane.outer.row,
        pane.outer.col,
        pane.outer.rows,
        pane.outer.cols,
        title,
        pane.focused && highlight_focus,
    );
    draw_scrollbar(
        buf,
        pane.outer.row,
        pane.outer.col,
        pane.outer.rows,
        pane.outer.cols,
        scrollbar.offset,
        scrollbar.filled,
        pane.focused && highlight_focus,
    );
}

pub(crate) fn render_capsule_pane_body_snapshot(
    buf: &mut Vec<u8>,
    cache: &mut PaneBodyCache,
    pane: &VisiblePane,
    snapshot: Vec<RowSnapshot>,
) -> PaneBodyRenderStats {
    cache.render_full_snapshot(
        snapshot,
        pane.inner.row,
        pane.inner.col,
        pane.inner.rows,
        pane.inner.cols,
        pane.body_dim,
        buf,
    )
}

pub(crate) fn render_capsule_pane_body_partial(
    buf: &mut Vec<u8>,
    cache: &mut PaneBodyCache,
    pane: &VisiblePane,
    screen: &vt100::Screen,
) -> PaneBodyRenderStats {
    cache.render_partial(
        screen,
        pane.inner.row,
        pane.inner.col,
        pane.inner.rows,
        pane.inner.cols,
        pane.body_dim,
        buf,
    )
}

pub(crate) fn render_capsule_selection_highlight(
    buf: &mut Vec<u8>,
    rows: &[RowSnapshot],
    selection: &SelectionState,
    dim: crate::tui::render::PaneBodyDim,
) {
    paint_selection_highlight(buf, rows, selection, dim);
}

pub(crate) struct CapsuleChromeHoverFrame<'a> {
    pub(crate) term_rows: u16,
    pub(crate) term_cols: u16,
    pub(crate) tabs: &'a [Tab],
    pub(crate) active_tab: usize,
    pub(crate) session_states: &'a [(u64, VisibleAgentState)],
    pub(crate) branch: Option<&'a str>,
    pub(crate) pull_request: Option<&'a PullRequestInfo>,
    pub(crate) pull_request_loading: bool,
    pub(crate) hover_target: Option<HoverTarget>,
}

pub(crate) fn render_capsule_chrome_hover_frame(
    buf: &mut Vec<u8>,
    status_bar: &mut StatusBar,
    view: CapsuleChromeHoverFrame<'_>,
) {
    render_capsule_status_bar(
        buf,
        status_bar,
        CapsuleStatusBarFrame {
            term_cols: view.term_cols,
            tabs: view.tabs,
            active_tab: view.active_tab,
            session_states: view.session_states,
            hover_target: view.hover_target,
        },
    );
    render_branch_context_bar(
        buf,
        view.term_rows,
        view.term_cols,
        view.branch,
        view.pull_request,
        view.pull_request_loading,
        status_bar.instance_id_label(),
        view.hover_target,
    );
}

pub(crate) struct CapsuleStatusBarFrame<'a> {
    pub(crate) term_cols: u16,
    pub(crate) tabs: &'a [Tab],
    pub(crate) active_tab: usize,
    pub(crate) session_states: &'a [(u64, VisibleAgentState)],
    pub(crate) hover_target: Option<HoverTarget>,
}

pub(crate) fn render_capsule_status_bar(
    buf: &mut Vec<u8>,
    status_bar: &mut StatusBar,
    view: CapsuleStatusBarFrame<'_>,
) {
    status_bar.render(
        buf,
        view.term_cols,
        view.tabs,
        view.active_tab,
        view.session_states,
        hovered_tab(view.hover_target),
        hovered_menu(view.hover_target),
    );
}

pub(crate) struct CapsuleRawDialogOverlay<'a> {
    pub(crate) term_rows: u16,
    pub(crate) term_cols: u16,
    pub(crate) dialog: &'a Dialog,
    pub(crate) copy_target_hovered: bool,
    pub(crate) github: GithubContextView<'a>,
}

pub(crate) fn render_capsule_raw_dialog_overlay(
    buf: &mut Vec<u8>,
    view: CapsuleRawDialogOverlay<'_>,
) {
    fill_screen(
        buf,
        view.term_rows,
        view.term_cols,
        jackin_tui::DIALOG_BACKDROP,
    );
    view.dialog.render_with_hover(
        buf,
        view.term_rows,
        view.term_cols,
        view.copy_target_hovered,
        Some(&view.github),
    );
    view.dialog.render_footer_hint(
        buf,
        view.term_rows,
        view.term_cols,
        Some(&view.github),
    );
}

pub(crate) fn render_capsule_dialog_backdrop(buf: &mut Vec<u8>, term_rows: u16, term_cols: u16) {
    fill_screen(buf, term_rows, term_cols, jackin_tui::DIALOG_BACKDROP);
}

pub(crate) struct CapsuleBottomChrome<'a> {
    pub(crate) term_rows: u16,
    pub(crate) term_cols: u16,
    pub(crate) branch: Option<&'a str>,
    pub(crate) pull_request: Option<&'a PullRequestInfo>,
    pub(crate) pull_request_loading: bool,
    pub(crate) instance_id_label: &'a str,
    pub(crate) hover_target: Option<HoverTarget>,
    pub(crate) scrollback_active: bool,
}

pub(crate) fn render_capsule_bottom_chrome(
    buf: &mut Vec<u8>,
    view: CapsuleBottomChrome<'_>,
) {
    render_branch_context_bar(
        buf,
        view.term_rows,
        view.term_cols,
        view.branch,
        view.pull_request,
        view.pull_request_loading,
        view.instance_id_label,
        view.hover_target,
    );

    let hint_spans = crate::tui::dialog::main_view_hint(view.scrollback_active);
    let hint_row = view.term_rows.saturating_sub(BRANCH_CONTEXT_BAR_ROWS + 2);
    crate::tui::dialog::render_hint_row(buf, hint_row, view.term_cols, hint_spans);
}

pub(crate) struct CapsuleDialogBottomChrome<'a> {
    pub(crate) term_rows: u16,
    pub(crate) term_cols: u16,
    pub(crate) branch: Option<&'a str>,
    pub(crate) pull_request: Option<&'a PullRequestInfo>,
    pub(crate) pull_request_loading: bool,
    pub(crate) instance_id_label: &'a str,
    pub(crate) hint_spans: Option<&'a [jackin_tui::HintSpan<'a>]>,
}

pub(crate) fn render_capsule_dialog_bottom_chrome(
    buf: &mut Vec<u8>,
    view: CapsuleDialogBottomChrome<'_>,
) {
    render_branch_context_bar(
        buf,
        view.term_rows,
        view.term_cols,
        view.branch,
        view.pull_request,
        view.pull_request_loading,
        view.instance_id_label,
        None,
    );
    if let Some(spans) = view.hint_spans {
        crate::tui::dialog::render_hint_row(
            buf,
            view.term_rows.saturating_sub(BRANCH_CONTEXT_BAR_ROWS + 2),
            view.term_cols,
            spans,
        );
    }
}

pub(crate) struct CapsuleRatatuiFrame<'a> {
    pub(crate) tabs: &'a [Tab],
    pub(crate) active_tab: usize,
    pub(crate) term_cols: u16,
    pub(crate) term_rows: u16,
    pub(crate) panes: &'a [VisiblePane],
    pub(crate) pane_titles: &'a [(u64, String)],
    pub(crate) focused_id: Option<u64>,
    pub(crate) zoomed: bool,
    pub(crate) dialog_open: bool,
    pub(crate) dialog_snapshot: Option<&'a (DialogRatatuiSnapshot, (u16, u16, u16, u16))>,
    pub(crate) scrollback_active: bool,
    pub(crate) pane_screens: &'a [(u64, &'a vt100::Screen)],
}

pub(crate) fn render_capsule_ratatui_frame(
    frame: &mut Frame<'_>,
    view: CapsuleRatatuiFrame<'_>,
) {
    let status_area = RatatuiRect {
        x: 0,
        y: 0,
        width: view.term_cols,
        height: crate::tui::components::status_bar::STATUS_BAR_ROWS,
    };
    frame.render_widget(
        StatusBarWidget {
            tabs: view.tabs,
            active_tab: view.active_tab,
            cols: view.term_cols,
        },
        status_area,
    );

    if view.dialog_open {
        let content_area = RatatuiRect {
            x: 0,
            y: crate::tui::components::status_bar::STATUS_BAR_ROWS,
            width: view.term_cols,
            height: view
                .term_rows
                .saturating_sub(crate::tui::components::status_bar::STATUS_BAR_ROWS),
        };
        frame.render_widget(DialogBackdrop, content_area);
        if let Some((snapshot, rect)) = view.dialog_snapshot {
            render_dialog_ratatui(frame, *rect, snapshot);
        }
        return;
    }

    let hint_spans = crate::tui::dialog::main_view_hint(view.scrollback_active);
    let hint_area = RatatuiRect {
        x: 0,
        y: view.term_rows.saturating_sub(BRANCH_CONTEXT_BAR_ROWS + 2),
        width: view.term_cols,
        height: 1,
    };
    jackin_tui::components::render_hint_bar(frame, hint_area, hint_spans);

    let sep_area = RatatuiRect {
        x: 0,
        y: view.term_rows.saturating_sub(BRANCH_CONTEXT_BAR_ROWS + 1),
        width: view.term_cols,
        height: 1,
    };
    frame.render_widget(ratatui::widgets::Block::default(), sep_area);

    for pane in view.panes {
        let title = view
            .pane_titles
            .iter()
            .find(|(id, _)| *id == pane.id)
            .map(|(_, t)| t.as_str())
            .unwrap_or("");

        let focused = Some(pane.id) == view.focused_id;
        let border_area = RatatuiRect {
            x: pane.outer.col,
            y: pane.outer.row,
            width: pane.outer.cols,
            height: pane.outer.rows,
        };
        frame.render_widget(
            PaneBorderWidget {
                title: title.to_string(),
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
            frame.render_widget(PaneBodyWidget::new(screen), body_area);
        }
    }
}

pub(crate) struct ScrollAffordanceMetrics {
    pub(crate) screen_rows: u16,
    pub(crate) screen_cols: u16,
    pub(crate) cursor_row: u16,
    pub(crate) cursor_col: u16,
    pub(crate) occupied_rows: usize,
    pub(crate) first_occupied_row: Option<u16>,
    pub(crate) last_occupied_row: Option<u16>,
}

pub(crate) fn screen_scroll_affordance_metrics(
    screen: &vt100::Screen,
    viewport_rows: u16,
    viewport_cols: u16,
) -> Option<ScrollAffordanceMetrics> {
    let (screen_rows, screen_cols) = screen.size();
    let rows = viewport_rows.min(screen_rows);
    let cols = viewport_cols.min(screen_cols);
    if rows == 0 || cols == 0 {
        return None;
    }

    let mut occupied_rows = 0usize;
    let mut first_occupied_row = None;
    let mut last_occupied_row = None;
    for row in 0..rows {
        if (0..cols).any(|col| screen.cell(row, col).is_some_and(|c| c.has_contents())) {
            occupied_rows += 1;
            first_occupied_row.get_or_insert(row);
            last_occupied_row = Some(row);
        }
    }
    let (cursor_row, cursor_col) = screen.cursor_position();

    Some(ScrollAffordanceMetrics {
        screen_rows,
        screen_cols,
        cursor_row,
        cursor_col,
        occupied_rows,
        first_occupied_row,
        last_occupied_row,
    })
}

/// Format a spawn-failure banner: save cursor → jump to row 1, col 1
/// → bold red text → clear to end of line → restore cursor. The
/// save/restore wrap prevents the banner from scrolling whichever
/// pane the composed frame left the cursor in.
pub(crate) fn spawn_failure_banner(reason: &str) -> Vec<u8> {
    format!("\x1b7\x1b[1;1H\x1b[1;31mjackin: {reason}\x1b[0m\x1b[K\x1b8").into_bytes()
}

/// Forwarded to the operator's outer terminal via `send_output` from the
/// `CopyToClipboard` dialog action. The OSC 52 byte encoding and terminal
/// compatibility notes live with the canonical implementation in
/// `jackin_tui::ansi::encode_osc52_clipboard_write`; keeping that detail in
/// one place stops the two copies from drifting.
pub(crate) fn encode_osc52_clipboard_write(payload: &str) -> Vec<u8> {
    jackin_tui::ansi::encode_osc52_clipboard_write(payload)
}
