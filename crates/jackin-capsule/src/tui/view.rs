//! Rendering helper types and functions for the capsule multiplexer.

use crate::pull_request::PullRequestInfo;
use crate::tui::app::{HoverTarget, VisibleAgentState, VisiblePane};
use crate::tui::components::branch_context_bar::{
    BRANCH_CONTEXT_BAR_ROWS, render_branch_context_bar,
};
use crate::tui::components::chrome::{DialogBackdrop, PaneBorderWidget, StatusBarWidget};
use crate::tui::components::dialog_widgets::{DialogRatatuiSnapshot, render_dialog_ratatui};
use crate::tui::components::pane::PaneBodyWidget;
use crate::tui::layout::Tab;
use jackin_tui::components::FocusOwner;
use ratatui::{Frame, layout::Rect as RatatuiRect, style::Modifier};

pub(crate) const fn hovered_tab(target: Option<HoverTarget>) -> Option<usize> {
    match target {
        Some(HoverTarget::Tab(idx)) => Some(idx),
        _ => None,
    }
}

pub(crate) const fn hovered_menu(target: Option<HoverTarget>) -> bool {
    matches!(target, Some(HoverTarget::Menu))
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
    /// Run ID for the red debug chip shown when `--debug` is active. `None` = no chip.
    pub(crate) debug_run_id: Option<&'a str>,
}

pub(crate) fn render_capsule_bottom_chrome(buf: &mut Vec<u8>, view: CapsuleBottomChrome<'_>) {
    crate::cdebug!(
        "bottom-chrome: site=raw-full term={}x{} branch_bar_row={} hint_row={} debug_chip={}",
        view.term_cols,
        view.term_rows,
        view.term_rows.saturating_sub(1),
        view.term_rows.saturating_sub(BRANCH_CONTEXT_BAR_ROWS + 2),
        view.debug_run_id.unwrap_or(""),
    );
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
    // Debug chip: red run-id chip at the far right of the branch context bar row.
    if let Some(run_id) = view.debug_run_id.filter(|r| !r.is_empty()) {
        let chip = format!(" {run_id} ");
        let chip_cols = jackin_tui::display_cols(&chip) as u16;
        let bar_row = view.term_rows.saturating_sub(1);
        let col = view.term_cols.saturating_sub(chip_cols).saturating_add(1); // 1-based
        jackin_tui::ansi::move_to(buf, bar_row, col);
        let (chip_bg, chip_fg) = if view.hover_target == Some(HoverTarget::DebugChip) {
            (
                jackin_tui::ansi::rgb_bg_dyn(jackin_tui::WHITE),
                jackin_tui::ansi::rgb_fg_dyn(jackin_tui::DANGER_RED),
            )
        } else {
            (
                jackin_tui::ansi::rgb_bg_dyn(jackin_tui::DANGER_RED),
                jackin_tui::ansi::rgb_fg_dyn(jackin_tui::WHITE),
            )
        };
        buf.extend_from_slice(chip_bg.as_bytes());
        buf.extend_from_slice(chip_fg.as_bytes());
        buf.extend_from_slice(jackin_tui::ansi::BOLD.as_bytes());
        buf.extend_from_slice(chip.as_bytes());
        buf.extend_from_slice(jackin_tui::ansi::RESET.as_bytes());
    }

    let hint_spans = crate::tui::components::dialog::main_view_hint(view.scrollback_active);
    let hint_row = view.term_rows.saturating_sub(BRANCH_CONTEXT_BAR_ROWS + 2);
    crate::tui::components::dialog::render_hint_row(buf, hint_row, view.term_cols, hint_spans);
}

pub(crate) struct CapsuleDialogBottomChrome<'a> {
    pub(crate) term_rows: u16,
    pub(crate) term_cols: u16,
    pub(crate) branch: Option<&'a str>,
    pub(crate) pull_request: Option<&'a PullRequestInfo>,
    pub(crate) pull_request_loading: bool,
    pub(crate) instance_id_label: &'a str,
    pub(crate) hint_spans: Option<&'a [jackin_tui::HintSpan<'a>]>,
    pub(crate) blank_background: bool,
}

pub(crate) fn render_capsule_dialog_bottom_chrome(
    buf: &mut Vec<u8>,
    view: CapsuleDialogBottomChrome<'_>,
) {
    crate::cdebug!(
        "bottom-chrome: site=dialog term={}x{} branch_bar_row={} hint_row={} has_hint={}",
        view.term_cols,
        view.term_rows,
        view.term_rows.saturating_sub(1),
        view.term_rows.saturating_sub(BRANCH_CONTEXT_BAR_ROWS + 2),
        view.hint_spans.is_some(),
    );
    if !view.blank_background {
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
    }
    if let Some(spans) = view.hint_spans {
        crate::tui::components::dialog::render_hint_row(
            buf,
            view.term_rows.saturating_sub(BRANCH_CONTEXT_BAR_ROWS + 2),
            view.term_cols,
            spans,
        );
    }
}

/// Dialog snapshot with its bounding rect — factored out to keep `CapsuleRatatuiFrame` readable.
pub(crate) type DialogFrameSnapshot = (DialogRatatuiSnapshot, (u16, u16, u16, u16));

#[derive(Debug)]
pub(crate) enum PaneScreen {
    Full(jackin_term::GridSnapshot),
}

pub(crate) struct CapsuleRatatuiFrame<'a> {
    pub(crate) tabs: &'a [Tab],
    pub(crate) active_tab: usize,
    pub(crate) term_cols: u16,
    pub(crate) term_rows: u16,
    pub(crate) panes: &'a [VisiblePane],
    pub(crate) pane_titles: &'a [(u64, String)],
    pub(crate) focus_owner: FocusOwner<u64>,
    pub(crate) zoomed: bool,
    pub(crate) dialog_open: bool,
    pub(crate) dialog_snapshot: Option<&'a DialogFrameSnapshot>,
    pub(crate) pane_screens: &'a [(u64, PaneScreen)],
    pub(crate) sessions_state: &'a [(u64, VisibleAgentState)],
    pub(crate) prefix_mode: crate::tui::components::status_bar::PrefixMode,
    pub(crate) hovered_tab: Option<usize>,
    pub(crate) menu_hovered: bool,
    pub(crate) selection: Option<crate::tui::selection::SelectionState>,
    pub(crate) selection_copied: bool,
    /// Per-pane scrollbar inputs `(session_id, offset, filled)`. A pane with
    /// `filled > 0` gets a thumb painted on its right border.
    pub(crate) scrollbars: &'a [(u64, usize, usize)],
}

/// Paint the scrollback thumb on a pane's right border, matching the raw
/// `draw_scrollbar`: the thumb spans the interior rows (top/bottom border
/// rows excluded) using the focused/unfocused colour.
fn apply_pane_scrollbar(
    buf: &mut ratatui::buffer::Buffer,
    pane: &VisiblePane,
    offset: usize,
    filled: usize,
    focused: bool,
) {
    if pane.outer.cols == 0 || pane.outer.rows < 2 {
        return;
    }
    let interior_rows = pane.outer.rows.saturating_sub(2);
    let Some(thumb) = jackin_tui::scroll::tail_vertical_thumb(interior_rows, filled, offset) else {
        return;
    };
    let col = pane
        .outer
        .col
        .saturating_add(pane.outer.cols)
        .saturating_sub(1);
    let color = if focused {
        jackin_tui::theme::PHOSPHOR_GREEN
    } else {
        jackin_tui::theme::BORDER_GRAY_LIGHT
    };
    let track_start_row = pane.outer.row + 1;
    for r in 0..thumb.len {
        let y = track_start_row + thumb.start + r;
        if let Some(cell) = buf.cell_mut((col, y)) {
            cell.set_symbol("█");
            cell.set_fg(color);
        }
    }
}

/// Overlay the inverse-video selection highlight onto the cells the pane
/// bodies already painted. The Ratatui equivalent of
/// `paint_selection_highlight`: it toggles `REVERSED` on the selected cells so
/// the `SocketBackend` diff carries it, instead of a raw post-frame append.
fn apply_selection_highlight(
    buf: &mut ratatui::buffer::Buffer,
    sel: &crate::tui::selection::SelectionState,
) {
    let (start_row, start_col, end_row, end_col) = crate::tui::selection::canonical_selection(sel);
    let inner = sel.inner;
    for r in start_row..=end_row {
        let from_col = if r == start_row { start_col } else { 0 };
        let to_col = if r == end_row {
            end_col
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

pub(crate) fn render_capsule_ratatui_frame(frame: &mut Frame<'_>, view: CapsuleRatatuiFrame<'_>) {
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
            sessions_state: view.sessions_state,
            prefix_mode: view.prefix_mode,
            hovered_tab: view.hovered_tab,
            menu_hovered: view.menu_hovered,
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
        frame.render_widget(DialogBackdrop, backdrop_area);
        if let Some((snapshot, rect)) = view.dialog_snapshot {
            render_dialog_ratatui(frame, *rect, snapshot);
        }
        return;
    }

    // Bottom chrome (hint row, separator pad, branch/PR bar) is NOT a Ratatui
    // widget: the caller appends it as raw ANSI after the Ratatui diff so a
    // single compositor owns each bottom row. Ratatui still clears these rows
    // (default blank cells in the swapped buffer) before the raw append paints
    // over them, so no stale chrome survives a resize.
    crate::cdebug!(
        "bottom-chrome: site=ratatui term={}x{} frame_area={}x{} hint_y={} sep_y={} branch_bar_y={} panes={}",
        view.term_cols,
        view.term_rows,
        frame.area().width,
        frame.area().height,
        view.term_rows.saturating_sub(BRANCH_CONTEXT_BAR_ROWS + 2),
        view.term_rows.saturating_sub(BRANCH_CONTEXT_BAR_ROWS + 1),
        view.term_rows.saturating_sub(BRANCH_CONTEXT_BAR_ROWS),
        view.panes.len(),
    );

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
                PaneScreen::Full(snap) => frame.render_widget(PaneBodyWidget::new(snap), body_area),
            }
        }
    }

    // Per-pane scrollback thumbs on the right border. Retained scrollback is
    // enough to show the thumb, even at the live tail; the shared tail-scroll
    // geometry places the thumb at the bottom for offset 0.
    for pane in view.panes {
        if let Some(&(_, offset, filled)) = view.scrollbars.iter().find(|(id, _, _)| *id == pane.id)
            && filled > 0
        {
            let focused = view.focus_owner.show_cursor_for(&pane.id);
            apply_pane_scrollbar(frame.buffer_mut(), pane, offset, filled, focused);
        }
    }

    // Selection highlight is overlaid after the pane bodies so the agent's
    // glyphs survive underneath the reversed-colour cue.
    if let Some(sel) = view.selection {
        apply_selection_highlight(frame.buffer_mut(), &sel);
    }
    if view.selection_copied {
        jackin_tui::components::render_toast(
            frame,
            RatatuiRect::new(0, 0, view.term_cols, view.term_rows),
            jackin_tui::components::Toast::new("Selection copied")
                .bottom_reserved_rows(BRANCH_CONTEXT_BAR_ROWS + 2),
        );
    }

    // Tab hover tooltip: codename pill painted one row below the hovered tab
    // cell, overlaid after pane bodies so it reads as a contextual label.
    // Hover enter/leave triggers a full redraw (see update_hover_for_mouse),
    // so the overlaid row is repainted clean when the operator moves away.
    if let Some(idx) = view.hovered_tab
        && let Some(tab) = view.tabs.get(idx)
    {
        apply_tab_codename_tooltip(
            frame.buffer_mut(),
            view.tabs,
            view.active_tab,
            view.sessions_state,
            view.prefix_mode,
            view.term_cols,
            idx,
            &tab.codename,
        );
    }
}

/// Paint the hovered tab's codename as a dark-bg + phosphor-green pill on the
/// row directly below the tab strip, left-aligned with the tab cell. Ratatui
/// `Buffer::set_string` clips to the buffer area, so an out-of-range column or
/// a too-long codename cannot overflow the frame.
#[allow(clippy::too_many_arguments)]
fn apply_tab_codename_tooltip(
    buf: &mut ratatui::buffer::Buffer,
    tabs: &[Tab],
    active_tab: usize,
    sessions_state: &[(u64, VisibleAgentState)],
    prefix_mode: crate::tui::components::status_bar::PrefixMode,
    cols: u16,
    hovered_idx: usize,
    codename: &str,
) {
    use ratatui::style::{Modifier, Style};
    let plan = crate::tui::components::status_bar::status_bar_plan(
        cols,
        tabs,
        active_tab,
        sessions_state,
        prefix_mode,
    );
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
            .bg(jackin_tui::theme::TAB_BG_INACTIVE)
            .fg(jackin_tui::theme::PHOSPHOR_GREEN)
            .add_modifier(Modifier::BOLD),
    );
}

/// Format a spawn-failure banner: save cursor → jump to row 1, col 1
/// → bold red text → clear to end of line → restore cursor. The
/// save/restore wrap prevents the banner from scrolling whichever
/// pane the composed frame left the cursor in.
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

#[cfg(test)]
mod tests;
