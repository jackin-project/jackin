// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Footer hint bar component.

use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
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
    Line::from(styled_hint_spans(spans, |color| color))
}

/// The canonical span-to-style mapping for hint rendering.
///
/// `remap` lets compositor surfaces translate shared palette colors to host
/// colors while keeping the same text/style vocabulary.
#[must_use]
pub fn styled_hint_spans(
    spans: &[HintSpan<'_>],
    remap: impl Fn(Color) -> Color,
) -> Vec<Span<'static>> {
    let key = Style::default()
        .fg(remap(crate::theme::WHITE))
        .add_modifier(Modifier::BOLD);
    let text = Style::default().fg(remap(crate::theme::PHOSPHOR_GREEN));
    let dim = Style::default().fg(remap(crate::theme::PHOSPHOR_DIM));
    let sep = Style::default().fg(remap(crate::theme::BORDER_GRAY));
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
    out
}

#[must_use]
pub fn wrapped_height(spans: &[HintSpan<'_>], width: u16) -> u16 {
    u16::try_from(wrapped_lines(spans, width).len().clamp(1, 64)).unwrap_or(64)
}

#[must_use]
pub fn wrapped_lines(spans: &[HintSpan<'_>], width: u16) -> Vec<Line<'static>> {
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
            HintSpan::Key(_) => {
                cur_w += span.display_cols();
                cur.extend(styled_hint_spans(std::slice::from_ref(span), |color| color));
            }
            HintSpan::DynKey(_) => {
                cur_w += span.display_cols();
                cur.extend(styled_hint_spans(std::slice::from_ref(span), |color| color));
            }
            HintSpan::Text(_) => {
                cur_w += span.display_cols();
                cur.extend(styled_hint_spans(std::slice::from_ref(span), |color| color));
            }
            HintSpan::Dyn(_) => {
                cur_w += span.display_cols();
                cur.extend(styled_hint_spans(std::slice::from_ref(span), |color| color));
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
                SepKind::Dot => row.extend(styled_hint_spans(&[HintSpan::Sep], |color| color)),
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
