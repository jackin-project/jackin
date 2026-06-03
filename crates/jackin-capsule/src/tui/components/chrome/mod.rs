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
    text::{Line, Span},
    widgets::{Block, Borders, Widget},
};

use crate::tui::layout::Tab;

use jackin_tui::{
    PHOSPHOR_DARK,
    components::{FocusPalette, TabStrip},
    theme::color as tc,
};

// ── Status bar (row 0 + row 1) ────────────────────────────────────────────────

/// Brand + tab strip (row 0) and the underline separator (row 1).
///
/// Mirrors the visual output of `StatusBar::render` using Ratatui spans so
/// the SocketBackend's `Buffer::diff` can track which cells changed between
/// frames.
pub struct StatusBarWidget<'a> {
    pub tabs: &'a [Tab],
    pub active_tab: usize,
    pub cols: u16,
}

const BRAND_TEXT: &str = "▓▓▓▓ ";
const BRAND_FG_COLOR: Color = Color::Black;

impl Widget for StatusBarWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 {
            return;
        }

        // Row 0: brand pill + tabs
        let row0 = Rect { height: 1, ..area };
        ratatui::widgets::Paragraph::new(Line::from(Span::styled(
            BRAND_TEXT,
            Style::default()
                .bg(jackin_tui::theme::PHOSPHOR_GREEN)
                .fg(BRAND_FG_COLOR)
                .add_modifier(Modifier::BOLD),
        )))
        .style(Style::default().bg(Color::Black))
        .render(row0, buf);

        // Row 1: baseline separator. `TabStrip` renders over the active
        // tab underline below, preserving one shared tab implementation.
        if area.height > 1 {
            let row1 = Rect {
                y: area.y + 1,
                height: 1,
                ..area
            };
            let line = "━".repeat(area.width as usize);
            ratatui::widgets::Paragraph::new(Span::styled(
                line,
                Style::default().fg(tc(PHOSPHOR_DARK)),
            ))
            .render(row1, buf);
        }

        let brand_cols = u16::try_from(jackin_tui::display_cols(BRAND_TEXT)).unwrap_or(u16::MAX);
        let tab_x = area.x.saturating_add(brand_cols).saturating_add(1);
        if tab_x < area.right() {
            let tab_area = Rect {
                x: tab_x,
                width: area.right().saturating_sub(tab_x),
                ..area
            };
            let labels: Vec<(&str, bool)> = self
                .tabs
                .iter()
                .enumerate()
                .map(|(i, tab)| (tab.label(), i == self.active_tab))
                .collect();
            TabStrip::new(&labels)
                .focused(false)
                .paragraph()
                .render(tab_area, buf);
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
