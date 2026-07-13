// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Generic non-interactive status popup.

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Style,
    widgets::{Paragraph, Wrap},
};

use crate::theme::WHITE;

use super::dialog_layout::{DialogBorder, dialog_inner_chunks, render_dialog_shell};

#[derive(Debug, Clone)]
pub struct StatusPopupState {
    title: String,
    message: String,
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
    if area.width < 8 || area.height < 7 {
        return;
    }
    let inner = render_dialog_shell(frame, area, Some(&state.title), DialogBorder::Default);
    let chunks = dialog_inner_chunks(inner, None);

    frame.render_widget(
        Paragraph::new(state.message.as_str())
            .style(Style::default().fg(WHITE))
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: false }),
        chunks[1],
    );
    frame.render_widget(
        Paragraph::new("Please wait")
            .style(crate::theme::DIM)
            .alignment(Alignment::Center),
        chunks[3],
    );
}
