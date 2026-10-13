// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Usage account list rendering.

use super::{
    append_publication_age, empty_publication_lines, freshness_age_label, meter_line, meter_style,
    non_empty_label, quota_state_label, row_style,
};

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::tui::state::ManagerState;

pub(crate) fn render_account_list(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &ManagerState<'_>,
    now_epoch: i64,
) {
    let Some(screen) = state.usage.screen.as_ref() else {
        return;
    };
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
    if screen.accounts.is_empty() {
        frame.render_widget(
            Paragraph::new(empty_publication_lines(screen, now_epoch))
                .style(Style::default().fg(Color::DarkGray))
                .wrap(Wrap { trim: false }),
            inner,
        );
        return;
    }
    let mut lines = Vec::new();
    append_publication_age(&mut lines, screen, now_epoch);
    let meter_width = inner.width.saturating_sub(8) as usize;
    lines.push(Line::from(Span::styled(
        format!(
            "  s:sort({}) f:filter({}) c:constrained",
            screen.sort.label(),
            screen.filter.label()
        ),
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(""));
    let order = screen.visible_order();
    if order.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("  No accounts match filter '{}'.", screen.filter.label()),
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(Span::styled(
            "  Press f to cycle the filter.",
            Style::default().fg(Color::DarkGray),
        )));
        frame.render_widget(
            Paragraph::new(lines)
                .scroll((screen.scroll, 0))
                .wrap(Wrap { trim: false }),
            inner,
        );
        return;
    }
    lines.push(Line::from(Span::styled(
        format!(
            "{}Overview",
            if screen.overview_selected() {
                "▸  "
            } else {
                "   "
            }
        ),
        row_style(screen.overview_selected()),
    )));
    lines.push(Line::from(""));
    for (pos, &index) in order.iter().enumerate() {
        let account = &screen.accounts[index];
        if pos == 0 || screen.accounts[order[pos - 1]].provider != account.provider {
            lines.push(Line::from(Span::styled(
                format!("  {}", account.provider),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )));
        }
        let selected = pos.saturating_add(1) == screen.selected;
        let cursor = if selected { "▸ " } else { "  " };
        let summary = account.summary_window().map_or_else(
            || account.status.clone(),
            |window| {
                if window.value.trim().is_empty() {
                    quota_state_label(window.quota_state).to_owned()
                } else {
                    window.value.clone()
                }
            },
        );
        lines.push(Line::from(Span::styled(
            format!("  {cursor}{}", account.account),
            row_style(selected),
        )));
        let mut sub = format!(
            "      {} · {} · {}",
            account.status,
            summary,
            freshness_age_label(now_epoch, account)
        );
        if let Some(plan) = non_empty_label(account.plan_label.as_ref()) {
            sub.push_str(&format!(" · {plan}"));
        }
        let issue_count = account.issue_count();
        if issue_count > 0 {
            sub.push_str(&format!(
                " · {} issue{}",
                issue_count,
                if issue_count == 1 { "" } else { "s" }
            ));
        }
        lines.push(Line::from(Span::styled(
            sub,
            Style::default().fg(Color::DarkGray),
        )));
        if let Some(window) = account.summary_window()
            && let Some(bar) = meter_line(meter_width, window.meter_percent())
        {
            lines.push(Line::from(Span::styled(
                bar,
                meter_style(window.quota_state),
            )));
        }
    }
    frame.render_widget(
        Paragraph::new(lines)
            .scroll((screen.scroll, 0))
            .wrap(Wrap { trim: false }),
        inner,
    );
}
