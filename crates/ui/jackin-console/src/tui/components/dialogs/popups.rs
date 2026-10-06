// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Error and status popups.

use jackin_oppicker::ModalOutcome;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Text},
};
use termrock::{
    input::{KeyCode, KeyEvent},
    widgets::{Dialog, PanelChrome},
};

#[derive(Debug, Clone)]
pub struct ErrorPopupState {
    pub title: String,
    pub message: String,
}

impl ErrorPopupState {
    #[must_use]
    pub fn new(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<()> {
        if matches!(
            key.code,
            KeyCode::Enter | KeyCode::Esc | KeyCode::Char('o' | 'O')
        ) {
            ModalOutcome::Cancel
        } else {
            ModalOutcome::Continue
        }
    }

    #[must_use]
    pub fn required_height(&self, inner_width: u16, max_rows: u16) -> u16 {
        let width = usize::from(inner_width.max(1));
        let rows = self
            .message
            .lines()
            .map(|line| termrock::text::display_cols(line).max(1).div_ceil(width))
            .sum::<usize>();
        u16::try_from(rows.saturating_add(4))
            .unwrap_or(u16::MAX)
            .min(max_rows.max(3))
    }
}

pub fn render_error_dialog(frame: &mut Frame<'_>, area: Rect, state: &ErrorPopupState) {
    let theme = termrock::style::DesignSystem::default();
    frame.render_widget(
        Dialog::new(&state.title, Text::from(state.message.clone()), &theme)
            .style(
                Style::default().fg(termrock::style::DesignSystem::default()
                    .style(termrock::style::Role::Danger)
                    .fg
                    .unwrap_or_default()),
            )
            .emphasis(PanelChrome::Focused),
        area,
    );
}

#[derive(Debug, Clone)]
pub struct StatusPopupState {
    pub title: String,
    pub message: String,
}

impl StatusPopupState {
    #[must_use]
    pub fn new(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
        }
    }
}

pub fn render_status_popup(frame: &mut Frame<'_>, area: Rect, state: &StatusPopupState) {
    let theme = termrock::style::DesignSystem::default();
    frame.render_widget(
        Dialog::new(
            &state.title,
            Text::from(vec![
                Line::from(state.message.clone()),
                Line::default(),
                Line::from("Please wait"),
            ]),
            &theme,
        )
        .style(Style::default())
        .emphasis(PanelChrome::Focused),
        area,
    );
}
