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

use crate::layout::Tab;

use jackin_tui::{
    PHOSPHOR_GREEN, PHOSPHOR_DARK, WHITE,
    theme::{color as tc},
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
const BRAND_BG_COLOR: Color = Color::Rgb(0, 255, 65);  // PHOSPHOR_GREEN
const BRAND_FG_COLOR: Color = Color::Black;

impl Widget for StatusBarWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 {
            return;
        }

        // Row 0: brand pill + tabs
        let row0 = Rect { height: 1, ..area };
        let mut spans: Vec<Span<'static>> = vec![
            Span::styled(
                BRAND_TEXT.to_string(),
                Style::default()
                    .bg(BRAND_BG_COLOR)
                    .fg(BRAND_FG_COLOR)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
        ];

        for (i, tab) in self.tabs.iter().enumerate() {
            let active = i == self.active_tab;
            let label = tab.label();
            if active {
                spans.push(Span::styled(
                    format!(" {label} "),
                    Style::default()
                        .bg(tc(WHITE))
                        .fg(Color::Black)
                        .add_modifier(Modifier::BOLD),
                ));
            } else {
                spans.push(Span::styled(
                    format!(" {label} "),
                    Style::default()
                        .fg(tc(PHOSPHOR_GREEN)),
                ));
            }
        }

        ratatui::widgets::Paragraph::new(Line::from(spans))
            .style(Style::default().bg(Color::Black))
            .render(row0, buf);

        // Row 1: underline separator
        if area.height > 1 {
            let row1 = Rect { y: area.y + 1, height: 1, ..area };
            let line = "━".repeat(area.width as usize);
            ratatui::widgets::Paragraph::new(Span::styled(
                line,
                Style::default().fg(tc(PHOSPHOR_DARK)),
            ))
            .render(row1, buf);
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
        let border_style = if self.focused {
            Style::default().fg(tc(PHOSPHOR_GREEN))
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(Span::styled(
                format!(" {} ", self.title),
                border_style,
            ));
        block.render(area, buf);
    }
}

// ── Dialog backdrop ───────────────────────────────────────────────────────────

/// Fills the entire terminal with the dialog backdrop color (opaque black).
///
/// Used in the dialog overlay path to hide pane content behind the dialog.
pub struct DialogBackdrop;

impl Widget for DialogBackdrop {
    fn render(self, area: Rect, buf: &mut Buffer) {
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                let cell = &mut buf[(x, y)];
                cell.set_char(' ');
                cell.set_bg(Color::Black);
                cell.set_fg(Color::Reset);
                cell.modifier = Modifier::empty();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn status_bar_renders_without_tabs() {
        let backend = TestBackend::new(80, 2);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| {
            frame.render_widget(
                StatusBarWidget { tabs: &[], active_tab: 0, cols: 80 },
                frame.area(),
            );
        }).unwrap();
        let buf = terminal.backend().buffer();
        // Brand pill should appear in row 0
        let row0: String = (0..5).map(|x| buf[(x, 0)].symbol().to_string()).collect();
        assert!(row0.contains("▓"), "brand pill missing: {row0:?}");
    }

    #[test]
    fn dialog_backdrop_fills_with_black() {
        let backend = TestBackend::new(10, 5);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| {
            frame.render_widget(DialogBackdrop, frame.area());
        }).unwrap();
        let buf = terminal.backend().buffer();
        assert_eq!(buf[(0, 0)].bg, Color::Black);
        assert_eq!(buf[(9, 4)].bg, Color::Black);
    }

    #[test]
    fn pane_border_renders_border() {
        let backend = TestBackend::new(20, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| {
            frame.render_widget(
                PaneBorderWidget { title: "shell".into(), focused: true },
                frame.area(),
            );
        }).unwrap();
        let buf = terminal.backend().buffer();
        // Top-left corner should be a border character
        let tl = buf[(0, 0)].symbol();
        assert!(!tl.trim().is_empty(), "top-left border missing");
    }
}
