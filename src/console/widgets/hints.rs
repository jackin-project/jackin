//! Shared footer-hint renderer for the host (ratatui) TUI surfaces.
//!
//! The hint vocabulary (`HintSpan`) lives in `jackin-tui` so the host cockpit
//! and the in-container multiplexer cannot drift. This is the host-side
//! renderer; the capsule has its own ANSI renderer over the same spans. Both
//! follow one styling rule: `Key` is white + bold, `Text` is phosphor green,
//! `Sep` is a gray `" · "`, and `GroupSep` is three spaces — centered in the
//! row. Keep new hint rows going through here rather than hand-building spans.

use jackin_tui::{BORDER_GRAY, HintSpan};
use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::{PHOSPHOR_GREEN, WHITE};

const SEP_GRAY: Color = Color::Rgb(BORDER_GRAY.r, BORDER_GRAY.g, BORDER_GRAY.b);

/// Render `spans` as a centered footer hint row into `area` (typically a
/// single-row rect at the bottom of the screen).
pub fn render(frame: &mut Frame<'_>, area: Rect, spans: &[HintSpan<'_>]) {
    if area.height == 0 {
        return;
    }
    frame.render_widget(Paragraph::new(line(spans)).alignment(Alignment::Center), area);
}

/// Build the styled hint line from shared spans. Pure so it can be unit-tested
/// and reused by callers that compose the line into a larger layout.
#[must_use]
pub fn line(spans: &[HintSpan<'_>]) -> Line<'static> {
    let key = Style::default().fg(WHITE).add_modifier(Modifier::BOLD);
    let text = Style::default().fg(PHOSPHOR_GREEN);
    let sep = Style::default().fg(SEP_GRAY);
    let mut out: Vec<Span<'static>> = Vec::with_capacity(spans.len());
    for span in spans {
        match span {
            HintSpan::Key(k) => out.push(Span::styled((*k).to_string(), key)),
            HintSpan::Text(t) => out.push(Span::styled(format!(" {t}"), text)),
            HintSpan::Sep => out.push(Span::styled(" · ".to_string(), sep)),
            HintSpan::GroupSep => out.push(Span::raw("   ")),
        }
    }
    Line::from(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_styles_keys_and_text_distinctly() {
        let spans = [
            HintSpan::Key("Esc"),
            HintSpan::Text("close"),
            HintSpan::GroupSep,
            HintSpan::Key("↑↓"),
            HintSpan::Text("scroll"),
        ];
        let rendered = line(&spans);
        // Key keeps its glyphs verbatim; Text gets a leading space.
        let joined: String = rendered.spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(joined, "Esc close   ↑↓ scroll");
        assert!(rendered.spans[0].style.add_modifier.contains(Modifier::BOLD));
        assert!(!rendered.spans[1].style.add_modifier.contains(Modifier::BOLD));
    }
}
