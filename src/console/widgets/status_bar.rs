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

use super::{DEBUG_AMBER, LINK_BLUE, WHITE};

/// Render the white status bar into `area`. `left` is the current-activity
/// text (black, bold). `right` is the primary link chip (blue, bold,
/// right-aligned) — typically a clickable id; pass an empty string to omit.
/// `right_debug`, when present, is a second chip rendered in amber to the
/// right of `right` — used for the debug-mode run id so debug is
/// unmistakable.
pub(crate) fn render(
    frame: &mut Frame,
    area: Rect,
    left: &str,
    right: &str,
    right_debug: Option<&str>,
) {
    // White band across the whole row first, so the inter-chunk gap is also
    // white rather than the terminal default.
    frame.render_widget(
        Block::default().style(Style::default().bg(WHITE).fg(Color::Black)),
        area,
    );

    let mut right_spans: Vec<Span<'static>> = Vec::new();
    if !right.is_empty() {
        right_spans.push(Span::styled(
            format!(" {right} "),
            Style::default()
                .bg(WHITE)
                .fg(LINK_BLUE)
                .add_modifier(Modifier::BOLD),
        ));
    }
    if let Some(debug) = right_debug.filter(|debug| !debug.is_empty()) {
        right_spans.push(Span::styled(
            format!(" {debug} "),
            Style::default()
                .bg(WHITE)
                .fg(DEBUG_AMBER)
                .add_modifier(Modifier::BOLD),
        ));
    }
    let right_w = u16::try_from(
        right_spans
            .iter()
            .map(|span| span.content.chars().count())
            .sum::<usize>(),
    )
    .unwrap_or(u16::MAX);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(right_w)])
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

    if !right_spans.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(right_spans)).alignment(Alignment::Right),
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
        term.draw(|f| render(f, Rect::new(0, 0, w, 1), left, right, None))
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
        term.draw(|f| render(f, Rect::new(0, 0, 30, 1), "x", "y", None))
            .unwrap();
        let buf = term.backend().buffer();
        // Every cell carries the white background, including the gap.
        for x in 0..30 {
            assert_eq!(buf[(x, 0)].bg, WHITE, "cell {x} should have white bg");
        }
    }

    #[test]
    fn debug_chip_renders_in_amber_to_the_right_of_the_instance_chip() {
        let backend = TestBackend::new(60, 1);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render(
                f,
                Rect::new(0, 0, 60, 1),
                "building",
                "s9994y2n",
                Some("jk-run-3d7e23"),
            )
        })
        .unwrap();
        let buf = term.backend().buffer();
        let row: String = (0..60).map(|x| buf[(x, 0)].symbol().to_string()).collect();
        assert!(row.contains("s9994y2n"), "instance chip missing: {row:?}");
        assert!(row.contains("jk-run-3d7e23"), "debug run-id chip missing: {row:?}");
        // The run id sits to the right of the instance id.
        assert!(
            row.find("s9994y2n").unwrap() < row.find("jk-run-3d7e23").unwrap(),
            "run id must be right of the instance id: {row:?}"
        );
        // The run-id cells render amber (debug accent), the instance blue.
        let amber = super::DEBUG_AMBER;
        assert!(
            (0..60).any(|x| buf[(x, 0)].fg == amber),
            "run-id chip must use the debug amber accent"
        );
    }
}
