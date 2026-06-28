//! Footer hint bar component.

use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::HintSpan;

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
    let key = crate::theme::BOLD_WHITE;
    let text = crate::theme::GREEN;
    let dim = crate::theme::DIM;
    let sep = crate::theme::BORDER;
    let mut out: Vec<Span<'static>> = Vec::with_capacity(spans.len());
    for span in spans {
        match span {
            HintSpan::Key(k) => out.push(Span::styled((*k).to_owned(), key)),
            HintSpan::DynKey(k) => out.push(Span::styled(k.clone(), key)),
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

    let key = crate::theme::BOLD_WHITE;
    let text = crate::theme::GREEN;
    let dim = crate::theme::DIM;
    let sep_style = crate::theme::BORDER;

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
                cur_w += span.display_cols();
                cur.push(Span::styled((*k).to_owned(), key));
            }
            HintSpan::DynKey(k) => {
                cur_w += span.display_cols();
                cur.push(Span::styled(k.clone(), key));
            }
            HintSpan::Text(t) => {
                cur_w += span.display_cols();
                cur.push(Span::styled(format!(" {t}"), text));
            }
            HintSpan::Dyn(t) => {
                cur_w += span.display_cols();
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
mod tests;
