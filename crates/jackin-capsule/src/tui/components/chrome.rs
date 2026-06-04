//! Ratatui widgets for capsule chrome: status bar, pane borders, branch bar.
//!
//! These widgets replace the raw-ANSI rendering in `compose_full_frame` and
//! `compose_partial_frame`. Together with `PaneBodyWidget` they make the
//! capsule's full rendering path go through the Ratatui `Buffer` → `SocketBackend`
//! pipeline, eliminating the hand-rolled ANSI diff in `PaneBodyCache`.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders, Widget},
};

use crate::tui::app::VisibleAgentState;
use crate::tui::components::status_bar::{PrefixMode, StatusTabCell, TabGlyph, status_bar_plan};
use crate::tui::layout::Tab;

use jackin_tui::components::FocusPalette;

// ── Status bar (row 0 + row 1) ────────────────────────────────────────────────

const BRAND_TEXT: &str = " jackin' ";
// Menu button colours (mirror BUTTON_BG_* in status_bar.rs). Kept as literal
// Rgb here because they are button-only and not part of the shared theme set.
const BUTTON_BG_IDLE: Color = Color::Rgb(18, 70, 130);
const BUTTON_BG_IDLE_HOVER: Color = Color::Rgb(32, 92, 158);
const BUTTON_BG_AWAITING: Color = Color::Rgb(96, 180, 255);
const BUTTON_BG_AWAITING_HOVER: Color = Color::Rgb(132, 202, 255);
const GLYPH_BLOCKED: Color = Color::Rgb(255, 60, 60);

/// Brand pill + tab cells (row 0) and the active-tab underline (row 1),
/// painted into the Ratatui `Buffer` so the SocketBackend diff tracks every
/// chrome cell. Layout columns come from `status_bar_plan`, the same source
/// `StatusBar::refresh_click_regions` uses, so the painted cells and the
/// click regions cannot drift.
pub struct StatusBarWidget<'a> {
    pub tabs: &'a [Tab],
    pub active_tab: usize,
    pub cols: u16,
    pub sessions_state: &'a [(u64, VisibleAgentState)],
    pub prefix_mode: PrefixMode,
    pub hovered_tab: Option<usize>,
    pub menu_hovered: bool,
}

impl StatusBarWidget<'_> {
    fn paint_tab(&self, cell: &StatusTabCell, idx: usize, area: Rect, buf: &mut Buffer) {
        let hovered = self.hovered_tab == Some(idx);
        let bg = match (cell.active, hovered) {
            (true, false) => jackin_tui::theme::TAB_BG_ACTIVE,
            (true, true) => jackin_tui::theme::TAB_BG_ACTIVE_HOVER,
            (false, false) => jackin_tui::theme::TAB_BG_INACTIVE,
            (false, true) => jackin_tui::theme::TAB_BG_INACTIVE_HOVER,
        };
        let mut style = Style::default().bg(bg).fg(jackin_tui::theme::WHITE);
        if cell.active {
            style = style.add_modifier(Modifier::BOLD);
        }
        let glyph_char = match cell.glyph {
            TabGlyph::None => ' ',
            TabGlyph::Done => '○',
            TabGlyph::Blocked => '●',
        };
        // Cell layout: ` <name> <sep> <glyph> ` — matches emit_tab_row0.
        let content = format!(" {} {} ", cell.name, glyph_char);
        let x = area.x.saturating_add(cell.start_col0);
        buf.set_string(x, area.y, &content, style);
        // Blocked glyph is bright red; overpaint just that cell, same bg.
        if matches!(cell.glyph, TabGlyph::Blocked) {
            let name_cols = u16::try_from(jackin_tui::display_cols(&cell.name)).unwrap_or(u16::MAX);
            let glyph_x = x.saturating_add(name_cols).saturating_add(2);
            buf.set_string(
                glyph_x,
                area.y,
                "●",
                Style::default()
                    .bg(bg)
                    .fg(GLYPH_BLOCKED)
                    .add_modifier(Modifier::BOLD),
            );
        }
    }
}

impl Widget for StatusBarWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 {
            return;
        }
        let plan = status_bar_plan(
            self.cols,
            self.tabs,
            self.active_tab,
            self.sessions_state,
            self.prefix_mode,
        );

        // Row 0: brand pill.
        buf.set_string(
            area.x,
            area.y,
            BRAND_TEXT,
            Style::default()
                .bg(jackin_tui::theme::PHOSPHOR_GREEN)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        );

        // Row 0: tab cells.
        for (idx, cell) in plan.cells.iter().enumerate() {
            self.paint_tab(cell, idx, area, buf);
        }

        // Row 0: right-side menu button.
        if let Some(start_1based) = plan.hint_start {
            let (bg, fg) = match (self.prefix_mode, self.menu_hovered) {
                (PrefixMode::Idle, false) => (BUTTON_BG_IDLE, jackin_tui::theme::WHITE),
                (PrefixMode::Idle, true) => (BUTTON_BG_IDLE_HOVER, jackin_tui::theme::WHITE),
                (PrefixMode::Awaiting, false) => (BUTTON_BG_AWAITING, Color::Black),
                (PrefixMode::Awaiting, true) => (BUTTON_BG_AWAITING_HOVER, Color::Black),
            };
            buf.set_string(
                area.x.saturating_add(start_1based.saturating_sub(1)),
                area.y,
                &plan.hint_text,
                Style::default().bg(bg).fg(fg).add_modifier(Modifier::BOLD),
            );
        }

        // Row 0: overflow indicator when a tab was clipped.
        if let Some(pos_1based) = plan.overflow_col {
            buf.set_string(
                area.x.saturating_add(pos_1based.saturating_sub(1)),
                area.y,
                "›",
                Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM),
            );
        }

        // Row 1: underline beneath the active tab cell only (blank elsewhere),
        // matching the raw StatusBar::render focus signal.
        if area.height > 1
            && let Some(active) = plan.cells.iter().find(|c| c.active)
        {
            let underline = "━".repeat(active.cell_cols as usize);
            buf.set_string(
                area.x.saturating_add(active.start_col0),
                area.y + 1,
                &underline,
                Style::default()
                    .fg(jackin_tui::theme::WHITE)
                    .add_modifier(Modifier::BOLD),
            );
        }
    }
}

// ── Pane border ───────────────────────────────────────────────────────────────

/// Renders the border and title for one pane, consistent with `draw_pane_box`.
pub struct PaneBorderWidget {
    pub title: String,
    pub focused: bool,
}

impl Widget for PaneBorderWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Use the capsule pane palette (gray ramp) rather than the console's
        // PHOSPHOR green. Green focus rings clash with agent terminal output;
        // near-white/gray provides clear focused/unfocused contrast without
        // the distraction.
        let palette = FocusPalette::CAPSULE_PANE;
        let border_color = if self.focused {
            palette.focused
        } else {
            palette.unfocused
        };
        let border_style = Style::default().fg(border_color);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(Span::styled(format!(" {} ", self.title), border_style));
        block.render(area, buf);
    }
}

pub use jackin_tui::components::ModalBackdrop as DialogBackdrop;

#[cfg(test)]
mod tests;
