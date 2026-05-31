//! Shared Ratatui tab strip.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{
    TabCell, lay_out_tabs,
    theme::{
        PHOSPHOR_GREEN, TAB_BG_ACTIVE, TAB_BG_ACTIVE_HOVER, TAB_BG_INACTIVE, TAB_BG_INACTIVE_HOVER,
        WHITE,
    },
};

#[derive(Debug, Clone, Copy)]
pub struct TabStrip<'a> {
    labels: &'a [(&'a str, bool)],
    focused: bool,
    hovered: Option<usize>,
}

impl<'a> TabStrip<'a> {
    #[must_use]
    pub const fn new(labels: &'a [(&'a str, bool)]) -> Self {
        Self {
            labels,
            focused: false,
            hovered: None,
        }
    }

    #[must_use]
    pub const fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    #[must_use]
    pub const fn hovered(mut self, hovered: Option<usize>) -> Self {
        self.hovered = hovered;
        self
    }

    pub fn render(self, frame: &mut Frame<'_>, area: Rect) {
        frame.render_widget(self.paragraph(), area);
    }

    #[must_use]
    pub fn paragraph(self) -> Paragraph<'static> {
        let cells = lay_out_tabs(self.labels, 0);
        Paragraph::new(vec![
            tab_label_line(&cells, self.hovered),
            tab_underline_line(&cells, self.focused),
        ])
    }
}

#[must_use]
pub fn tab_label_line(cells: &[TabCell<'_>], hovered: Option<usize>) -> Line<'static> {
    let mut spans = Vec::with_capacity(cells.len().saturating_mul(2));
    for (idx, cell) in cells.iter().enumerate() {
        let bg = match (cell.active, hovered == Some(idx)) {
            (true, true) => TAB_BG_ACTIVE_HOVER,
            (true, false) => TAB_BG_ACTIVE,
            (false, true) => TAB_BG_INACTIVE_HOVER,
            (false, false) => TAB_BG_INACTIVE,
        };
        let style = if cell.active {
            Style::default()
                .bg(bg)
                .fg(WHITE)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().bg(bg).fg(WHITE)
        };
        spans.push(Span::styled(format!(" {} ", cell.label), style));
        spans.push(Span::raw(" ".repeat(usize::from(crate::TAB_GAP))));
    }
    Line::from(spans)
}

#[must_use]
pub fn tab_underline_line(cells: &[TabCell<'_>], focused: bool) -> Line<'static> {
    let mut spans = Vec::with_capacity(cells.len().saturating_mul(2));
    for cell in cells {
        if focused {
            let bar_text = if cell.active {
                "━".repeat(usize::from(cell.cell_cols))
            } else {
                " ".repeat(usize::from(cell.cell_cols))
            };
            // Active tab underline uses PHOSPHOR_GREEN when tab bar is focused
            // — consistent with the "focused = bright green" rule across all
            // surfaces. WHITE was too subtle against the dark background.
            spans.push(Span::styled(
                bar_text,
                if cell.active {
                    Style::default().fg(PHOSPHOR_GREEN)
                } else {
                    Style::default()
                },
            ));
        } else {
            spans.push(Span::raw(" ".repeat(usize::from(cell.cell_cols))));
        }
        spans.push(Span::raw(" ".repeat(usize::from(crate::TAB_GAP))));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::{TabStrip, tab_underline_line};
    use crate::lay_out_tabs;

    #[test]
    fn underline_marks_only_active_tab_when_focused() {
        let cells = lay_out_tabs(&[("General", true), ("Mounts", false)], 0);

        let text: String = tab_underline_line(&cells, true)
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();

        assert_eq!(text, "━━━━━━━━━          ");
    }

    #[test]
    fn tab_strip_exposes_two_rows() {
        let labels = [("General", true), ("Mounts", false)];
        let backend = ratatui::backend::TestBackend::new(24, 2);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                TabStrip::new(&labels)
                    .focused(true)
                    .render(frame, frame.area())
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(0, 0)].symbol(), " ");
        assert_eq!(buffer[(0, 1)].symbol(), "━");
    }
}
