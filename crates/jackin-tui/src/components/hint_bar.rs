//! Footer hint bar component.

use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::HintSpan;
use crate::theme::{BORDER_GRAY, PHOSPHOR_DIM, PHOSPHOR_GREEN, WHITE};

#[derive(Debug, Clone, Copy)]
pub struct HintBar<'a> {
    spans: &'a [HintSpan<'a>],
    wrapped: bool,
}

impl<'a> HintBar<'a> {
    #[must_use]
    pub const fn new(spans: &'a [HintSpan<'a>]) -> Self {
        Self {
            spans,
            wrapped: false,
        }
    }

    #[must_use]
    pub const fn wrapped(mut self) -> Self {
        self.wrapped = true;
        self
    }
}

impl Widget for HintBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 {
            return;
        }
        let text = if self.wrapped {
            wrapped_lines(self.spans, area.width)
        } else {
            vec![line(self.spans)]
        };
        Paragraph::new(text)
            .alignment(Alignment::Center)
            .render(area, buf);
    }
}

pub fn render_hint_bar(frame: &mut ratatui::Frame<'_>, area: Rect, spans: &[HintSpan<'_>]) {
    frame.render_widget(HintBar::new(spans), area);
}

pub fn render_wrapped_hint_bar(frame: &mut ratatui::Frame<'_>, area: Rect, spans: &[HintSpan<'_>]) {
    frame.render_widget(HintBar::new(spans).wrapped(), area);
}

#[must_use]
pub fn line(spans: &[HintSpan<'_>]) -> Line<'static> {
    let key = Style::default().fg(WHITE).add_modifier(Modifier::BOLD);
    let text = Style::default().fg(PHOSPHOR_GREEN);
    let dim = Style::default().fg(PHOSPHOR_DIM);
    let sep = Style::default().fg(BORDER_GRAY);
    let mut out: Vec<Span<'static>> = Vec::with_capacity(spans.len());
    for span in spans {
        match span {
            HintSpan::Key(k) => out.push(Span::styled((*k).to_string(), key)),
            HintSpan::Text(t) => out.push(Span::styled(format!(" {t}"), text)),
            HintSpan::Dyn(t) => out.push(Span::styled(format!(" {t}"), dim)),
            HintSpan::Sep => out.push(Span::styled(" · ", sep)),
            HintSpan::GroupSep => out.push(Span::raw("   ")),
        }
    }
    Line::from(out)
}

#[must_use]
pub fn wrapped_height(spans: &[HintSpan<'_>], width: u16) -> u16 {
    u16::try_from(wrapped_lines(spans, width).len().clamp(1, 64)).unwrap_or(64)
}

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
    let sep_style = Style::default().fg(BORDER_GRAY);

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
                SepKind::Dot => row.push(Span::styled(" · ", sep_style)),
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
    use ratatui::{Terminal, backend::TestBackend};

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
            HintSpan::Key("⇥"),
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
        for line in &lines {
            let width: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
            assert!(width <= 60, "line width {width} exceeds 60: {line:?}");
        }
    }

    #[test]
    fn widget_centers_single_row_hint() {
        let backend = TestBackend::new(24, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        let items = [HintSpan::Key("Esc"), HintSpan::Text("close")];
        terminal
            .draw(|frame| frame.render_widget(HintBar::new(&items), frame.area()))
            .unwrap();
        let row: String = (0..24)
            .map(|x| terminal.backend().buffer()[(x, 0)].symbol().to_string())
            .collect();
        assert!(row.contains("Esc close"), "hint missing: {row:?}");
        assert!(
            row.starts_with("       "),
            "hint should be centered: {row:?}"
        );
    }
}
