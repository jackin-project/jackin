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

use super::{PHOSPHOR_DIM, PHOSPHOR_GREEN, WHITE};

const SEP_GRAY: Color = Color::Rgb(BORDER_GRAY.r, BORDER_GRAY.g, BORDER_GRAY.b);

/// Render `spans` as a centered footer hint row into `area` (typically a
/// single-row rect at the bottom of the screen).
pub fn render(frame: &mut Frame<'_>, area: Rect, spans: &[HintSpan<'_>]) {
    if area.height == 0 {
        return;
    }
    frame.render_widget(
        Paragraph::new(line(spans)).alignment(Alignment::Center),
        area,
    );
}

/// Build the styled hint line from shared spans. Pure so it can be unit-tested
/// and reused by callers that compose the line into a larger layout.
#[must_use]
pub fn line(spans: &[HintSpan<'_>]) -> Line<'static> {
    let key = Style::default().fg(WHITE).add_modifier(Modifier::BOLD);
    let text = Style::default().fg(PHOSPHOR_GREEN);
    let dim = Style::default().fg(PHOSPHOR_DIM);
    let sep = Style::default().fg(SEP_GRAY);
    let mut out: Vec<Span<'static>> = Vec::with_capacity(spans.len());
    for span in spans {
        match span {
            HintSpan::Key(k) => out.push(Span::styled((*k).to_string(), key)),
            HintSpan::Text(t) => out.push(Span::styled(format!(" {t}"), text)),
            HintSpan::Dyn(t) => out.push(Span::styled(format!(" {t}"), dim)),
            HintSpan::Sep => out.push(Span::styled(" · ".to_string(), sep)),
            HintSpan::GroupSep => out.push(Span::raw("   ")),
        }
    }
    Line::from(out)
}

/// Render `spans` as a centered, possibly multi-row footer into `area`.
///
/// Hint groups are packed greedily and wrapped to new rows when they overflow
/// the width. Used by the workspace-manager footer, whose hint sets are long
/// enough to need wrapping; the single-row [`render`] suffices everywhere the
/// hint always fits on one line.
pub fn render_wrapped(frame: &mut Frame<'_>, area: Rect, spans: &[HintSpan<'_>]) {
    frame.render_widget(
        Paragraph::new(wrapped_lines(spans, area.width)).alignment(Alignment::Center),
        area,
    );
}

/// Rows the wrapped footer needs to show every span within `width` columns.
#[must_use]
pub fn wrapped_height(spans: &[HintSpan<'_>], width: u16) -> u16 {
    u16::try_from(wrapped_lines(spans, width).len().max(1)).unwrap_or(u16::MAX)
}

/// Greedy line-packer shared by the wrapped footer renderer. A chunk is one
/// logical hint unit (key + optional label); chunks stay on the current row
/// while they fit and wrap otherwise. Separators cost three columns.
fn wrapped_lines(spans: &[HintSpan<'_>], width: u16) -> Vec<Line<'static>> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum SepKind {
        Group,
        Dot,
    }
    struct Chunk {
        spans: Vec<Span<'static>>,
        width: usize,
        sep: SepKind,
    }

    let key = Style::default().fg(WHITE).add_modifier(Modifier::BOLD);
    let text = Style::default().fg(PHOSPHOR_GREEN);
    let dim = Style::default().fg(PHOSPHOR_DIM);
    let sep_style = Style::default().fg(SEP_GRAY);

    let mut chunks: Vec<Chunk> = Vec::new();
    let mut cur: Vec<Span<'static>> = Vec::new();
    let mut cur_w: usize = 0;
    let mut next_sep = SepKind::Group;
    let flush = |chunks: &mut Vec<Chunk>, spans: &mut Vec<Span<'static>>, w: &mut usize, sep| {
        if !spans.is_empty() {
            chunks.push(Chunk {
                spans: std::mem::take(spans),
                width: *w,
                sep,
            });
            *w = 0;
        }
    };
    for span in spans {
        match span {
            HintSpan::Key(k) => {
                cur_w += k.chars().count();
                cur.push(Span::styled((*k).to_string(), key));
            }
            HintSpan::Text(t) => {
                cur_w += 1 + t.chars().count();
                cur.push(Span::styled(format!(" {t}"), text));
            }
            HintSpan::Dyn(t) => {
                cur_w += 1 + t.chars().count();
                cur.push(Span::styled(format!(" {t}"), dim));
            }
            HintSpan::Sep => {
                flush(&mut chunks, &mut cur, &mut cur_w, next_sep);
                next_sep = SepKind::Dot;
            }
            HintSpan::GroupSep => {
                flush(&mut chunks, &mut cur, &mut cur_w, next_sep);
                next_sep = SepKind::Group;
            }
        }
    }
    flush(&mut chunks, &mut cur, &mut cur_w, next_sep);

    let max_w = width as usize;
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut row: Vec<Span<'static>> = Vec::new();
    let mut row_w: usize = 0;
    for chunk in &chunks {
        let needed = if row.is_empty() {
            chunk.width
        } else {
            3 + chunk.width
        };
        if !row.is_empty() && row_w + needed > max_w {
            lines.push(Line::from(std::mem::take(&mut row)));
            row_w = 0;
        }
        if !row.is_empty() {
            match chunk.sep {
                SepKind::Dot => row.push(Span::styled(" \u{b7} ".to_string(), sep_style)),
                SepKind::Group => row.push(Span::raw("   ")),
            }
            row_w += 3;
        }
        row.extend(chunk.spans.iter().cloned());
        row_w += chunk.width;
    }
    if !row.is_empty() {
        lines.push(Line::from(row));
    }
    if lines.is_empty() {
        lines.push(Line::raw(""));
    }
    lines
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
        assert!(
            rendered.spans[0]
                .style
                .add_modifier
                .contains(Modifier::BOLD)
        );
        assert!(
            !rendered.spans[1]
                .style
                .add_modifier
                .contains(Modifier::BOLD)
        );
    }

    #[test]
    fn wrapped_short_fits_one_line() {
        let items = [
            HintSpan::Key("S"),
            HintSpan::Text("save"),
            HintSpan::GroupSep,
            HintSpan::Key("Esc"),
            HintSpan::Text("back"),
        ];
        assert_eq!(wrapped_lines(&items, 80).len(), 1);
        assert_eq!(wrapped_height(&items, 80), 1);
    }

    #[test]
    fn wrapped_long_wraps_within_width() {
        let items = [
            HintSpan::Key("↑↓"),
            HintSpan::Text("navigate"),
            HintSpan::Sep,
            HintSpan::Key("D"),
            HintSpan::Text("remove"),
            HintSpan::Sep,
            HintSpan::Key("R"),
            HintSpan::Text("toggle ro/rw"),
            HintSpan::GroupSep,
            HintSpan::Key("Tab"),
            HintSpan::Text("switch tab"),
            HintSpan::GroupSep,
            HintSpan::Key("S"),
            HintSpan::Text("save settings"),
            HintSpan::GroupSep,
            HintSpan::Key("Esc"),
            HintSpan::Text("back"),
        ];
        let lines = wrapped_lines(&items, 60);
        assert!(lines.len() > 1, "should wrap at 60 cols: {lines:?}");
        for l in &lines {
            let w: usize = l.spans.iter().map(|s| s.content.chars().count()).sum();
            assert!(w <= 60, "line width {w} exceeds 60: {l:?}");
        }
    }

    #[test]
    fn wrapped_empty_is_one_blank_line() {
        let lines = wrapped_lines(&[], 80);
        assert_eq!(lines.len(), 1);
        let joined: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(joined, "");
    }

    #[test]
    fn wrapped_styles_sep_dyn_and_groupsep() {
        let items = [
            HintSpan::Key("E"),
            HintSpan::Text("edit"),
            HintSpan::Sep,
            HintSpan::Dyn("3 changes".to_string()),
            HintSpan::GroupSep,
            HintSpan::Key("Q"),
            HintSpan::Text("quit"),
        ];
        let lines = wrapped_lines(&items, 200);
        let spans = &lines[0].spans;
        // [E, " edit", " · ", " 3 changes", "   ", Q, " quit"]
        assert_eq!(spans[0].style.fg, Some(WHITE));
        assert_eq!(spans[1].style.fg, Some(PHOSPHOR_GREEN));
        assert_eq!(spans[2].content.as_ref(), " \u{b7} ");
        assert_eq!(spans[2].style.fg, Some(SEP_GRAY));
        assert_eq!(spans[3].content.as_ref(), " 3 changes");
        assert_eq!(spans[3].style.fg, Some(PHOSPHOR_DIM));
        assert_eq!(spans[4].content.as_ref(), "   ");
    }
}
