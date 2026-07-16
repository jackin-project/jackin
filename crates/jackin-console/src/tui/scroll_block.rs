// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Console adapter around TermRock [`Viewport`] for bordered scrollable panels.
//!
//! Migration 0018 removed free-function `render_scrollable_block*` helpers in
//! favor of the canonical stateful widget. This thin adapter preserves the
//! call shape used across workspace/settings/editor tabs.
//!
//! `focused` means **interaction ownership** (green border via
//! [`PanelEmphasis::Focused`]). Callers that implement the passive-scroll
//! focusability rule must clear their focus state when content fits, before
//! calling this helper.

use ratatui::{Frame, layout::Rect, text::Line};
use termrock::{
    Theme,
    scroll::DialogScroll,
    widgets::{PanelEmphasis, Viewport},
};

/// Render a bordered scrollable block using TermRock `Viewport`.
pub fn render_scrollable_block_at(
    frame: &mut Frame<'_>,
    area: Rect,
    lines: Vec<Line<'_>>,
    scroll_x: u16,
    scroll_y: u16,
    focused: bool,
    title: Option<&str>,
) {
    let theme = Theme::default();
    let mut scroll = DialogScroll::default();
    scroll.scroll_x = scroll_x;
    scroll.scroll_y = scroll_y;
    let emphasis = if focused {
        PanelEmphasis::Focused
    } else {
        PanelEmphasis::Normal
    };
    let mut viewport = Viewport::new(&lines, &theme).emphasis(emphasis);
    if let Some(title) = title {
        viewport = viewport.title(title);
    }
    frame.render_stateful_widget(viewport, area, &mut scroll);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend, style::Color};
    use termrock::style::Role;

    fn border_fg(role: Role) -> Color {
        Theme::default().style(role).fg.expect("theme role has fg")
    }

    #[test]
    fn focused_content_uses_focused_border_even_when_content_fits() {
        let backend = TestBackend::new(24, 6);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                render_scrollable_block_at(
                    frame,
                    area,
                    vec![Line::from("fits")],
                    0,
                    0,
                    true,
                    Some("Body"),
                );
            })
            .unwrap();
        let cell = &terminal.backend().buffer()[(0, 0)];
        assert_eq!(cell.fg, border_fg(Role::BorderFocused));
    }

    #[test]
    fn unfocused_content_uses_normal_border() {
        let backend = TestBackend::new(24, 6);
        let mut terminal = Terminal::new(backend).unwrap();
        let lines: Vec<Line<'_>> = (0..20).map(|i| Line::from(format!("row {i}"))).collect();
        terminal
            .draw(|frame| {
                let area = frame.area();
                render_scrollable_block_at(frame, area, lines, 0, 0, false, Some("Body"));
            })
            .unwrap();
        let cell = &terminal.backend().buffer()[(0, 0)];
        assert_eq!(cell.fg, border_fg(Role::Border));
    }
}
