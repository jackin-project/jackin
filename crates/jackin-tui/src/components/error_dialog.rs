//! Single-button error dialog component.

use crossterm::event::KeyEvent;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use super::button_strip::{ButtonStrip, ButtonStripItem};
use super::dialog_layout::{
    DialogBodyScroll, DialogBorder, dialog_inner_chunks, render_dialog_shell,
    render_scrollable_dialog_body,
};
use crate::ansi;
use crate::keymap::{KeyBinding, KeyChord, Keymap, LogicalKey, Visibility};
use crate::theme::{LINK_FG, PHOSPHOR_DARK, PHOSPHOR_GREEN, WHITE};
use crate::{HintSpan, ModalOutcome, centered_rect};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorPopupAction {
    Dismiss,
}

const ERROR_POPUP_BINDINGS: &[KeyBinding<ErrorPopupAction>] = &[
    KeyBinding {
        chords: &[
            KeyChord::plain(LogicalKey::Enter),
            KeyChord::plain(LogicalKey::Esc),
        ],
        action: ErrorPopupAction::Dismiss,
        hint: Some("dismiss"),
        visibility: Visibility::Shown,
        glyph: Some("↵/Esc"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(LogicalKey::Char('o')),
            KeyChord::plain(LogicalKey::Char('O')),
        ],
        action: ErrorPopupAction::Dismiss,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
];

pub static ERROR_POPUP_KEYMAP: Keymap<ErrorPopupAction> = Keymap::new(ERROR_POPUP_BINDINGS);

#[must_use]
pub fn error_popup_hint_spans() -> Vec<HintSpan<'static>> {
    ERROR_POPUP_KEYMAP.hint_spans()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorPopupRow {
    pub label: &'static str,
    pub value: String,
    pub href: Option<String>,
    pub badge: Option<String>,
    pub highlighted: bool,
    pub strong: bool,
}

impl ErrorPopupRow {
    #[must_use]
    pub fn new(label: &'static str, value: impl Into<String>) -> Self {
        Self {
            label,
            value: value.into(),
            href: None,
            badge: None,
            highlighted: false,
            strong: false,
        }
    }

    #[must_use]
    pub fn hyperlink(mut self, href: impl Into<String>) -> Self {
        self.href = Some(href.into());
        self
    }

    #[must_use]
    pub fn badge(mut self, badge: impl Into<String>) -> Self {
        self.badge = Some(badge.into());
        self
    }

    #[must_use]
    pub const fn highlighted(mut self, highlighted: bool) -> Self {
        self.highlighted = highlighted;
        self
    }

    #[must_use]
    pub const fn strong(mut self, strong: bool) -> Self {
        self.strong = strong;
        self
    }
}

#[derive(Debug, Clone)]
pub struct ErrorPopupState {
    pub title: String,
    pub message: String,
    pub rows: Vec<ErrorPopupRow>,
    pub scroll: DialogBodyScroll,
    cached_rows: Option<(u16, u16)>,
}

impl ErrorPopupState {
    #[must_use]
    pub fn new(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            rows: Vec::new(),
            scroll: DialogBodyScroll::new(),
            cached_rows: None,
        }
    }

    #[must_use]
    pub fn with_rows(mut self, rows: Vec<ErrorPopupRow>) -> Self {
        self.rows = rows;
        self
    }

    #[must_use]
    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<()> {
        match ERROR_POPUP_KEYMAP.dispatch(KeyChord::from(key)) {
            Some(ErrorPopupAction::Dismiss) => ModalOutcome::Cancel,
            None => ModalOutcome::Continue,
        }
    }

    #[must_use]
    pub fn row_value_rects(&self, inner: Rect) -> Vec<Rect> {
        row_value_rects(inner, self)
    }

    #[must_use]
    pub fn row_value_rect_groups(&self, inner: Rect) -> Vec<Vec<Rect>> {
        row_value_rect_groups(inner, self)
    }
}

#[must_use]
pub fn estimated_message_rows(state: &ErrorPopupState, inner_width: u16) -> u16 {
    estimated_plain_message_rows(state, inner_width)
        .saturating_add(estimated_row_rows(state, inner_width))
}

#[must_use]
fn estimated_plain_message_rows(state: &ErrorPopupState, inner_width: u16) -> u16 {
    if let Some((cached_width, rows)) = state.cached_rows
        && cached_width == inner_width
    {
        return rows;
    }
    let width = usize::from(inner_width.max(1));
    let mut rows: u32 = 0;
    for line in state.message.lines() {
        let len = line.chars().count().max(1);
        rows = rows.saturating_add(u32::try_from(len.div_ceil(width)).unwrap_or(u32::MAX));
    }
    u16::try_from(rows.max(1)).unwrap_or(u16::MAX)
}

#[must_use]
fn estimated_row_rows(state: &ErrorPopupState, inner_width: u16) -> u16 {
    state
        .rows
        .iter()
        .map(|row| {
            u16::try_from(error_row_value_chunks(row, inner_width).len()).unwrap_or(u16::MAX)
        })
        .fold(0u16, u16::saturating_add)
}

#[must_use]
pub fn required_height(state: &ErrorPopupState, inner_width: u16, max_rows: u16) -> u16 {
    // 2 borders + 1 leading + body + 1 spacer + 1 button + 1 trailing = body + 6
    let body = estimated_message_rows(state, inner_width);
    body.saturating_add(6).min(max_rows.max(7))
}

pub fn render_error_dialog(frame: &mut ratatui::Frame<'_>, area: Rect, state: &ErrorPopupState) {
    let inner_width = area.width.saturating_sub(2);
    let height = required_height(state, inner_width, area.height);
    let dialog_area = centered_rect(area.width, height, area);
    render_error_dialog_in(frame, dialog_area, state);
}

pub fn render_error_dialog_in(frame: &mut ratatui::Frame<'_>, area: Rect, state: &ErrorPopupState) {
    let inner = render_dialog_shell(frame, area, Some(&state.title), DialogBorder::Danger);
    let body_rows = estimated_message_rows(state, inner.width).min(inner.height.saturating_sub(4));
    let chunks = dialog_inner_chunks(inner, Some(body_rows));
    let lines = error_dialog_lines(state, chunks[1].width);
    let mut scroll = state.scroll.clone();
    render_scrollable_dialog_body(frame, area, chunks[1], &lines, &mut scroll);

    let ok = [ButtonStripItem::new("OK")];
    ButtonStrip::new(&ok).render(chunks[3], frame.buffer_mut());
}

fn error_dialog_lines(state: &ErrorPopupState, width: u16) -> Vec<Line<'static>> {
    let mut lines = state
        .message
        .lines()
        .flat_map(|line| wrap_plain_message_line(line, width))
        .collect::<Vec<_>>();
    if lines.is_empty() {
        lines.push(Line::raw(""));
    }
    lines.extend(
        state
            .rows
            .iter()
            .flat_map(|row| error_row_lines(row, width)),
    );
    lines
}

fn wrap_plain_message_line(line: &str, width: u16) -> Vec<Line<'static>> {
    let width = usize::from(width.max(1));
    let mut rest = line;
    let mut lines = Vec::new();
    while !rest.is_empty() {
        let chunk = crate::take_display_cols(rest, width);
        if chunk.is_empty() {
            break;
        }
        rest = &rest[chunk.len()..];
        lines.push(Line::from(Span::styled(chunk, Style::default().fg(WHITE))));
    }
    if lines.is_empty() {
        lines.push(Line::raw(""));
    }
    lines
}

fn error_row_lines(row: &ErrorPopupRow, width: u16) -> Vec<Line<'static>> {
    let prefix_cols = error_row_prefix_cols(row);
    let chunks = error_row_value_chunks(row, width);
    chunks
        .into_iter()
        .enumerate()
        .map(|(idx, value)| {
            let mut spans = Vec::new();
            if idx == 0 {
                spans.push(Span::raw("  "));
                spans.push(Span::styled(row.label.to_owned(), crate::theme::DIM));
                spans.push(Span::styled(": ", Style::default().fg(PHOSPHOR_DARK)));
            } else {
                spans.push(Span::raw(" ".repeat(prefix_cols)));
            }
            spans.push(Span::styled(value, error_row_value_style(row)));
            if idx == 0
                && let Some(badge) = &row.badge
            {
                spans.push(Span::raw("  "));
                spans.push(Span::styled(
                    badge.clone(),
                    Style::default()
                        .fg(PHOSPHOR_GREEN)
                        .add_modifier(Modifier::BOLD),
                ));
            }
            Line::from(spans)
        })
        .collect()
}

fn error_row_value_style(row: &ErrorPopupRow) -> Style {
    if row.href.is_some() || row.highlighted {
        Style::default()
            .fg(LINK_FG)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
    } else if row.strong {
        crate::theme::BOLD_WHITE
    } else {
        Style::default().fg(WHITE)
    }
}

fn error_row_prefix_cols(row: &ErrorPopupRow) -> usize {
    2usize
        .saturating_add(crate::display_cols(row.label))
        .saturating_add(2)
}

fn error_row_value_chunks(row: &ErrorPopupRow, width: u16) -> Vec<String> {
    let prefix_cols = error_row_prefix_cols(row);
    let badge_cols = row
        .badge
        .as_ref()
        .map_or(0, |badge| 2usize.saturating_add(crate::display_cols(badge)));
    let first_cols = usize::from(width)
        .saturating_sub(prefix_cols)
        .saturating_sub(badge_cols)
        .max(1);
    let continuation_cols = usize::from(width).saturating_sub(prefix_cols).max(1);
    let mut rest = row.value.as_str();
    let mut chunks = Vec::new();
    let mut cols = first_cols;
    while !rest.is_empty() {
        let chunk = crate::take_display_cols(rest, cols);
        if chunk.is_empty() {
            break;
        }
        rest = &rest[chunk.len()..];
        chunks.push(chunk);
        cols = continuation_cols;
    }
    if chunks.is_empty() {
        chunks.push(String::new());
    }
    chunks
}

#[must_use]
pub fn row_value_rects(inner: Rect, state: &ErrorPopupState) -> Vec<Rect> {
    row_value_rect_groups(inner, state)
        .into_iter()
        .filter_map(|mut rects| rects.drain(..).next())
        .collect()
}

#[must_use]
pub fn row_value_rect_groups(inner: Rect, state: &ErrorPopupState) -> Vec<Vec<Rect>> {
    row_value_rect_entries(inner, state)
        .into_iter()
        .map(|entries| entries.into_iter().map(|(_, rect)| rect).collect())
        .collect()
}

fn row_value_rect_entries(inner: Rect, state: &ErrorPopupState) -> Vec<Vec<(usize, Rect)>> {
    let content_rows =
        estimated_message_rows(state, inner.width).min(inner.height.saturating_sub(4));
    let chunks = dialog_inner_chunks(inner, Some(content_rows));
    let message_rows = estimated_plain_message_rows(state, inner.width);
    let mut logical_y = message_rows;
    state
        .rows
        .iter()
        .map(|row| {
            let chunks_for_row = error_row_value_chunks(row, chunks[1].width);
            let x = chunks[1]
                .x
                .saturating_add(u16::try_from(error_row_prefix_cols(row)).unwrap_or(u16::MAX));
            let rects = chunks_for_row
                .iter()
                .enumerate()
                .filter_map(|(idx, chunk)| {
                    let absolute_y =
                        logical_y.saturating_add(u16::try_from(idx).unwrap_or(u16::MAX));
                    if absolute_y < state.scroll.scroll_y {
                        return None;
                    }
                    let row_y = absolute_y.saturating_sub(state.scroll.scroll_y);
                    if row_y >= chunks[1].height {
                        return None;
                    }
                    let y = chunks[1].y.saturating_add(row_y);
                    let chunk_width = u16::try_from(crate::display_cols(chunk))
                        .unwrap_or(u16::MAX)
                        .max(1);
                    Some((
                        idx,
                        Rect {
                            x,
                            y,
                            width: chunks[1].right().saturating_sub(x).min(chunk_width),
                            height: 1,
                        },
                    ))
                })
                .collect::<Vec<_>>();
            logical_y =
                logical_y.saturating_add(u16::try_from(chunks_for_row.len()).unwrap_or(u16::MAX));
            rects
        })
        .collect()
}

#[must_use]
pub fn hyperlink_regions(inner: Rect, state: &ErrorPopupState) -> Vec<(Rect, String)> {
    let rects = row_value_rect_groups(inner, state);
    state
        .rows
        .iter()
        .zip(rects)
        .filter_map(|(row, rects)| {
            row.href
                .as_ref()
                .and_then(|href| rects.into_iter().next().map(|rect| (rect, href.clone())))
        })
        .collect()
}

#[must_use]
pub fn hyperlink_overlay(inner: Rect, state: &ErrorPopupState) -> Vec<u8> {
    let rects = row_value_rect_entries(inner, state);
    let mut out = Vec::new();
    for (row, rects) in state.rows.iter().zip(rects) {
        let Some(href) = row.href.as_ref() else {
            continue;
        };
        let value_chunks = error_row_value_chunks(row, inner.width);
        for (idx, rect) in rects {
            let Some(chunk) = value_chunks.get(idx) else {
                continue;
            };
            let visible = crate::display_cols_slice(chunk, 0, usize::from(rect.width));
            if visible.is_empty() {
                continue;
            }
            ansi::move_to(&mut out, rect.y.saturating_add(1), rect.x.saturating_add(1));
            ansi::emit_osc8_open(&mut out, href);
            ansi::fg(&mut out, crate::LINK_FG);
            out.extend_from_slice(b"\x1b[1;4m");
            out.extend_from_slice(visible.as_bytes());
            ansi::emit_osc8_close(&mut out);
            out.extend_from_slice(ansi::RESET.as_bytes());
        }
    }
    out
}

#[cfg(test)]
mod tests;
