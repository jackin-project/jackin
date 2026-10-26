// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::list_geometry;

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::tui::state::ManagerState;

pub(crate) fn render_account_list_with_geometry(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &ManagerState<'_>,
    now_epoch: i64,
) -> Option<super::UsageListGeometry> {
    let screen = state.usage.screen.as_ref()?;
    let focused = true;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(if focused {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::DarkGray)
        });
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let geometry = list_geometry(screen, inner, now_epoch, state.usage.visible);
    let paragraph = Paragraph::new(geometry.lines.clone()).wrap(Wrap { trim: false });
    let paragraph = if geometry.scroll {
        paragraph.scroll((screen.list_scroll, 0))
    } else {
        paragraph
    };
    let paragraph = if let Some(style) = geometry.style {
        paragraph.style(style)
    } else {
        paragraph
    };
    frame.render_widget(paragraph, inner);
    Some(geometry)
}
