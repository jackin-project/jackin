// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Single-button error dialog component.

use std::cell::Cell;

use crossterm::event::KeyEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap};

use crate::keymap::{KeyBinding, KeyChord, Keymap, LogicalKey, Visibility};
use crate::theme::{DANGER_RED, WHITE};
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

#[derive(Debug, Clone)]
pub struct ErrorPopupState {
    pub title: String,
    pub message: String,
    cached_rows: Cell<Option<(u16, u16)>>,
}

impl ErrorPopupState {
    #[must_use]
    pub fn new(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            cached_rows: Cell::new(None),
        }
    }

    #[must_use]
    pub fn handle_key(&self, key: KeyEvent) -> ModalOutcome<()> {
        match ERROR_POPUP_KEYMAP.dispatch(KeyChord::from(key)) {
            Some(ErrorPopupAction::Dismiss) => ModalOutcome::Cancel,
            None => ModalOutcome::Continue,
        }
    }
}

#[derive(Debug)]
pub struct ErrorDialog<'a> {
    state: &'a ErrorPopupState,
}

impl<'a> ErrorDialog<'a> {
    #[must_use]
    pub const fn new(state: &'a ErrorPopupState) -> Self {
        Self { state }
    }
}

impl Widget for ErrorDialog<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let title = format!(" {} ", self.state.title);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(DANGER_RED))
            .title(Span::styled(title, crate::theme::DANGER));
        let inner = block.inner(area);
        Clear.render(area, buf);
        block.render(area, buf);

        // Canonical dialog layout: leading spacer + body + spacer + button + trailing spacer.
        let body_rows =
            estimated_message_rows(self.state, inner.width).min(inner.height.saturating_sub(4));
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),         // leading spacer
                Constraint::Length(body_rows), // message body
                Constraint::Length(1),         // spacer
                Constraint::Length(1),         // OK button
                Constraint::Length(1),         // trailing spacer
            ])
            .split(inner);

        Paragraph::new(self.state.message.as_str())
            .style(Style::default().fg(WHITE))
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: false })
            .render(chunks[1], buf);

        let focused_style = Style::default()
            .bg(WHITE)
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD);
        Paragraph::new(Line::from(Span::styled("  OK  ", focused_style)))
            .alignment(Alignment::Center)
            .render(chunks[3], buf);
    }
}

#[must_use]
pub fn estimated_message_rows(state: &ErrorPopupState, inner_width: u16) -> u16 {
    if let Some((cached_width, rows)) = state.cached_rows.get()
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
    let result = u16::try_from(rows.max(1)).unwrap_or(u16::MAX);
    state.cached_rows.set(Some((inner_width, result)));
    result
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
    frame.render_widget(ErrorDialog::new(state), area);
}

#[cfg(test)]
mod tests;
