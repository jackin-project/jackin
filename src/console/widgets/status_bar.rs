//! White bottom status bar shared across host TUI surfaces.
//!
//! Mirrors the in-container multiplexer's bottom bar (`jackin-capsule`'s
//! `render_branch_context_bar`): a full-width white band with a left
//! activity label (black, bold) and a right link chip (blue, bold) —
//! typically a clickable identifier such as a container instance id. The
//! colours come from the shared `jackin-tui` palette so the host bar and
//! the in-container bar cannot drift. Used by the launch/loading screen
//! today; any future host surface that needs a bottom bar renders through
//! this helper rather than re-inlining the band geometry.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

use super::{LINK_BLUE, WHITE};

/// Render the white status bar into `area`. `left` is the current-activity
/// text (rendered black, bold); `right` is the link chip (rendered blue,
/// bold, right-aligned) — pass an empty string to omit it.
pub(crate) fn render(frame: &mut Frame, area: Rect, left: &str, right: &str) {
    // White band across the whole row first, so the inter-chunk gap is also
    // white rather than the terminal default.
    frame.render_widget(
        Block::default().style(Style::default().bg(WHITE).fg(Color::Black)),
        area,
    );

    let chip = if right.is_empty() {
        String::new()
    } else {
        format!(" {right} ")
    };
    let chip_w = u16::try_from(chip.chars().count()).unwrap_or(u16::MAX);
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(chip_w)])
        .split(area);

    let activity = Line::from(vec![
        Span::raw(" "),
        Span::styled(
            left.to_string(),
            Style::default()
                .bg(WHITE)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    frame.render_widget(Paragraph::new(activity), cols[0]);

    if !chip.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                chip,
                Style::default()
                    .bg(WHITE)
                    .fg(LINK_BLUE)
                    .add_modifier(Modifier::BOLD),
            )))
            .alignment(Alignment::Right),
            cols[1],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn dump(left: &str, right: &str, w: u16) -> String {
        let backend = TestBackend::new(w, 1);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| render(f, Rect::new(0, 0, w, 1), left, right))
            .unwrap();
        let buf = term.backend().buffer();
        (0..w).map(|x| buf[(x, 0)].symbol().to_string()).collect()
    }

    #[test]
    fn renders_activity_on_the_left_and_chip_on_the_right() {
        let row = dump("Building Docker image", "k7p9m2xq", 60);
        assert!(row.contains("Building Docker image"), "activity missing: {row:?}");
        assert!(row.contains("k7p9m2xq"), "chip missing: {row:?}");
        // The activity sits left of the chip.
        let activity_at = row.find("Building").unwrap();
        let chip_at = row.find("k7p9m2xq").unwrap();
        assert!(activity_at < chip_at, "activity must be left of the chip: {row:?}");
    }

    #[test]
    fn omits_the_chip_when_right_is_empty() {
        let row = dump("preparing launch", "", 40);
        assert!(row.contains("preparing launch"));
    }

    #[test]
    fn bar_fills_white_background_across_the_row() {
        let backend = TestBackend::new(30, 1);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| render(f, Rect::new(0, 0, 30, 1), "x", "y"))
            .unwrap();
        let buf = term.backend().buffer();
        // Every cell carries the white background, including the gap.
        for x in 0..30 {
            assert_eq!(buf[(x, 0)].bg, WHITE, "cell {x} should have white bg");
        }
    }
}
