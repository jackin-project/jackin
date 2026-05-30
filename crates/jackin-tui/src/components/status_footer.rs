//! White bottom status footer component.

use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Widget};

use crate::theme::{DEBUG_AMBER, LINK_BLUE, WHITE, faded};

#[derive(Debug, Clone, Copy)]
pub struct StatusFooter<'a> {
    left: &'a str,
    right: &'a str,
    right_debug: Option<&'a str>,
    alpha: f32,
    left_hover: bool,
}

impl<'a> StatusFooter<'a> {
    #[must_use]
    pub const fn new(left: &'a str) -> Self {
        Self {
            left,
            right: "",
            right_debug: None,
            alpha: 1.0,
            left_hover: false,
        }
    }

    #[must_use]
    pub const fn right(mut self, right: &'a str) -> Self {
        self.right = right;
        self
    }

    #[must_use]
    pub const fn right_debug(mut self, right_debug: Option<&'a str>) -> Self {
        self.right_debug = right_debug;
        self
    }

    #[must_use]
    pub const fn alpha(mut self, alpha: f32) -> Self {
        self.alpha = alpha;
        self
    }

    #[must_use]
    pub const fn left_hover(mut self, left_hover: bool) -> Self {
        self.left_hover = left_hover;
        self
    }
}

impl Widget for StatusFooter<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Block::default()
            .style(
                Style::default()
                    .bg(faded(WHITE, self.alpha))
                    .fg(Color::Black),
            )
            .render(area, buf);

        let mut right_spans: Vec<Span<'static>> = Vec::new();
        if !self.right.is_empty() {
            right_spans.push(Span::styled(
                format!(" {} ", self.right),
                Style::default()
                    .bg(faded(WHITE, self.alpha))
                    .fg(faded(LINK_BLUE, self.alpha))
                    .add_modifier(Modifier::BOLD),
            ));
        }
        if let Some(debug) = self.right_debug.filter(|debug| !debug.is_empty()) {
            right_spans.push(Span::styled(
                format!(" {debug} "),
                Style::default()
                    .bg(faded(WHITE, self.alpha))
                    .fg(faded(DEBUG_AMBER, self.alpha))
                    .add_modifier(Modifier::BOLD),
            ));
        }
        let right_width = u16::try_from(
            right_spans
                .iter()
                .map(|span| span.content.chars().count())
                .sum::<usize>(),
        )
        .unwrap_or(u16::MAX);

        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(1), Constraint::Length(right_width)])
            .split(area);

        let activity_fg = if self.left_hover {
            faded(LINK_BLUE, self.alpha)
        } else {
            Color::Black
        };
        let activity = Line::from(vec![
            Span::raw(" "),
            Span::styled(
                self.left.to_string(),
                Style::default()
                    .bg(faded(WHITE, self.alpha))
                    .fg(activity_fg)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);
        Paragraph::new(activity).render(cols[0], buf);

        if !right_spans.is_empty() {
            Paragraph::new(Line::from(right_spans))
                .alignment(Alignment::Right)
                .render(cols[1], buf);
        }
    }
}

pub fn render_status_footer(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    left: &str,
    right: &str,
    right_debug: Option<&str>,
    alpha: f32,
    left_hover: bool,
) {
    frame.render_widget(
        StatusFooter::new(left)
            .right(right)
            .right_debug(right_debug)
            .alpha(alpha)
            .left_hover(left_hover),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn dump(left: &str, right: &str, width: u16) -> String {
        let backend = TestBackend::new(width, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(StatusFooter::new(left).right(right), frame.area());
            })
            .unwrap();
        (0..width)
            .map(|x| terminal.backend().buffer()[(x, 0)].symbol().to_string())
            .collect()
    }

    #[test]
    fn renders_activity_on_the_left_and_chip_on_the_right() {
        let row = dump("Building Docker image", "k7p9m2xq", 60);
        assert!(
            row.contains("Building Docker image"),
            "activity missing: {row:?}"
        );
        assert!(row.contains("k7p9m2xq"), "chip missing: {row:?}");
        assert!(
            row.find("Building").unwrap() < row.find("k7p9m2xq").unwrap(),
            "activity must be left of the chip: {row:?}"
        );
    }

    #[test]
    fn bar_fills_white_background_across_the_row() {
        let backend = TestBackend::new(30, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(StatusFooter::new("x").right("y"), frame.area());
            })
            .unwrap();
        for x in 0..30 {
            assert_eq!(
                terminal.backend().buffer()[(x, 0)].bg,
                WHITE,
                "cell {x} should have white bg"
            );
        }
    }

    #[test]
    fn debug_chip_renders_in_amber_to_the_right_of_the_instance_chip() {
        let backend = TestBackend::new(60, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(
                    StatusFooter::new("building")
                        .right("s9994y2n")
                        .right_debug(Some("jk-run-3d7e23")),
                    frame.area(),
                );
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        let row: String = (0..60)
            .map(|x| buffer[(x, 0)].symbol().to_string())
            .collect();
        assert!(row.contains("s9994y2n"), "instance chip missing: {row:?}");
        assert!(
            row.contains("jk-run-3d7e23"),
            "debug run-id chip missing: {row:?}"
        );
        assert!(
            row.find("s9994y2n").unwrap() < row.find("jk-run-3d7e23").unwrap(),
            "run id must be right of the instance id: {row:?}"
        );
        assert!(
            (0..60).any(|x| buffer[(x, 0)].fg == DEBUG_AMBER),
            "run-id chip must use the debug amber accent"
        );
    }
}
