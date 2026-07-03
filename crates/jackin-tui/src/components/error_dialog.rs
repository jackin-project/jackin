//! Single-button error dialog component.

use crossterm::event::KeyEvent;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget, Wrap};

use super::button_strip::{ButtonStrip, ButtonStripItem};
use super::dialog_layout::{DialogBorder, dialog_inner_chunks, render_dialog_shell};
use crate::ansi;
use crate::keymap::{KeyBinding, KeyChord, Keymap, LogicalKey, Visibility};
use crate::theme::{LINK_FG, PHOSPHOR_DARK, WHITE};
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
}

impl ErrorPopupRow {
    #[must_use]
    pub fn new(label: &'static str, value: impl Into<String>) -> Self {
        Self {
            label,
            value: value.into(),
            href: None,
        }
    }

    #[must_use]
    pub fn hyperlink(mut self, href: impl Into<String>) -> Self {
        self.href = Some(href.into());
        self
    }
}

#[derive(Debug, Clone)]
pub struct ErrorPopupState {
    pub title: String,
    pub message: String,
    pub rows: Vec<ErrorPopupRow>,
    cached_rows: Option<(u16, u16)>,
}

impl ErrorPopupState {
    #[must_use]
    pub fn new(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            rows: Vec::new(),
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
}

#[must_use]
pub fn estimated_message_rows(state: &ErrorPopupState, inner_width: u16) -> u16 {
    estimated_plain_message_rows(state, inner_width)
        .saturating_add(u16::try_from(state.rows.len()).unwrap_or(u16::MAX))
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
    let message_rows = estimated_plain_message_rows(state, inner.width);
    let visible_message_rows = message_rows.min(chunks[1].height);
    let message_area = Rect {
        height: visible_message_rows,
        ..chunks[1]
    };

    Paragraph::new(state.message.as_str())
        .style(Style::default().fg(WHITE))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: false })
        .render(message_area, frame.buffer_mut());
    render_error_rows(chunks[1], message_rows, state, frame.buffer_mut());

    let ok = [ButtonStripItem::new("OK")];
    ButtonStrip::new(&ok).render(chunks[3], frame.buffer_mut());
}

fn render_error_rows(
    content_area: Rect,
    message_rows: u16,
    state: &ErrorPopupState,
    buf: &mut ratatui::buffer::Buffer,
) {
    for (idx, row) in state.rows.iter().enumerate() {
        let y = content_area
            .y
            .saturating_add(message_rows)
            .saturating_add(u16::try_from(idx).unwrap_or(u16::MAX));
        if y >= content_area.y.saturating_add(content_area.height) {
            break;
        }
        let line = error_row_line(row);
        Paragraph::new(line).render(
            Rect {
                x: content_area.x,
                y,
                width: content_area.width,
                height: 1,
            },
            buf,
        );
    }
}

fn error_row_line(row: &ErrorPopupRow) -> Line<'static> {
    let value_style = if row.href.is_some() {
        Style::default()
            .fg(LINK_FG)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
    } else {
        Style::default().fg(WHITE)
    };
    Line::from(vec![
        Span::raw("  "),
        Span::styled(row.label.to_owned(), crate::theme::DIM),
        Span::styled(": ", Style::default().fg(PHOSPHOR_DARK)),
        Span::styled(row.value.clone(), value_style),
    ])
}

#[must_use]
pub fn row_value_rects(inner: Rect, state: &ErrorPopupState) -> Vec<Rect> {
    let content_rows =
        estimated_message_rows(state, inner.width).min(inner.height.saturating_sub(4));
    let chunks = dialog_inner_chunks(inner, Some(content_rows));
    let message_rows = estimated_plain_message_rows(state, inner.width);
    state
        .rows
        .iter()
        .enumerate()
        .filter_map(|(idx, row)| {
            let y = chunks[1]
                .y
                .saturating_add(message_rows)
                .saturating_add(u16::try_from(idx).unwrap_or(u16::MAX));
            if y >= chunks[1].y.saturating_add(chunks[1].height) {
                return None;
            }
            let prefix_cols = 2usize
                .saturating_add(crate::display_cols(row.label))
                .saturating_add(2);
            let x = chunks[1]
                .x
                .saturating_add(u16::try_from(prefix_cols).unwrap_or(u16::MAX));
            Some(Rect {
                x,
                y,
                width: chunks[1].right().saturating_sub(x),
                height: 1,
            })
        })
        .collect()
}

#[must_use]
pub fn hyperlink_regions(inner: Rect, state: &ErrorPopupState) -> Vec<(Rect, String)> {
    let rects = row_value_rects(inner, state);
    state
        .rows
        .iter()
        .zip(rects)
        .filter_map(|(row, rect)| row.href.as_ref().map(|href| (rect, href.clone())))
        .collect()
}

#[must_use]
pub fn hyperlink_overlay(inner: Rect, state: &ErrorPopupState) -> Vec<u8> {
    let rects = row_value_rects(inner, state);
    let mut out = Vec::new();
    for (row, rect) in state.rows.iter().zip(rects) {
        let Some(href) = row.href.as_ref() else {
            continue;
        };
        let visible = crate::display_cols_slice(&row.value, 0, usize::from(rect.width));
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
    out
}

#[cfg(test)]
mod tests;
