//! Rendering helper types and functions for the capsule multiplexer.

use crate::input::PrefixCommand;
use crate::render::draw_scrollbar;
use crate::session::Session;
use crate::statusbar::draw_pane_box;
use crate::tui::app::{HoverTarget, PointerShape, VisiblePane};
use crate::tui::update::FullRedrawReason;

pub(crate) const fn hovered_tab(target: Option<HoverTarget>) -> Option<usize> {
    match target {
        Some(HoverTarget::Tab(idx)) => Some(idx),
        _ => None,
    }
}

pub(crate) const fn hovered_menu(target: Option<HoverTarget>) -> bool {
    matches!(target, Some(HoverTarget::Menu))
}

pub(crate) fn prefix_full_redraw_reason(cmd: &PrefixCommand) -> FullRedrawReason {
    match cmd {
        PrefixCommand::NewTab | PrefixCommand::Palette => FullRedrawReason::PaletteOverlay,
        PrefixCommand::NextTab | PrefixCommand::PrevTab | PrefixCommand::JumpTab(_) => {
            FullRedrawReason::TabSwitch
        }
        PrefixCommand::SplitTopBottom | PrefixCommand::SplitSideBySide => {
            FullRedrawReason::LayoutChange
        }
        PrefixCommand::MoveFocus(_) => FullRedrawReason::FocusChange,
        PrefixCommand::ZoomToggle => FullRedrawReason::ZoomChange,
        PrefixCommand::KillPane | PrefixCommand::KillTab => FullRedrawReason::SplitClose,
        PrefixCommand::ClearPane => FullRedrawReason::PaneClear,
        PrefixCommand::Detach | PrefixCommand::Redraw => FullRedrawReason::ExplicitRedraw,
    }
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

pub(crate) fn pane_scrollbar(
    session: &mut Session,
    viewport_rows: u16,
    viewport_cols: u16,
) -> PaneScrollbar {
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

/// Draw the pane box and optional scrollbar for one visible pane.
///
/// Called identically from compose_full_frame and compose_partial_frame;
/// lives here so both compositors stay in lock-step when the chrome rules
/// change.
pub(crate) fn draw_pane_chrome(
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

pub(crate) fn osc22_pointer_shape(shape: PointerShape) -> Vec<u8> {
    format!("\x1b]22;{}\x1b\\", shape.as_osc22_name()).into_bytes()
}
