// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{
    append_account_full_body, append_account_summary_body, append_overview_account,
    append_publication_age, append_publication_issues, credential_expiry_label,
    freshness_age_label, identity_kind_label, non_empty_label, panel, refreshing_line,
    relative_time_label,
};

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

use crate::tui::state::ManagerState;

pub(crate) fn render_detail(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &ManagerState<'_>,
    now_epoch: i64,
) {
    let Some(screen) = state.usage.screen.as_ref() else {
        return;
    };
    let Some(account) = screen.selected_account() else {
        if screen.accounts.is_empty() {
            render_detail_panel(
                frame,
                area,
                "Overview",
                screen,
                super::publication::empty_publication_body_lines(screen, now_epoch),
            );
            return;
        }
        let order = screen.visible_order();
        if order.is_empty() {
            let mut lines = vec![
                Line::from(format!(
                    "No accounts match filter '{}'.",
                    screen.filter.label()
                )),
                Line::from(""),
                Line::from("Press f to cycle the filter."),
            ];
            append_publication_age(&mut lines, screen, now_epoch);
            append_publication_issues(&mut lines, screen, now_epoch);
            if let Some(notice) = &screen.notice {
                lines.push(Line::from(Span::styled(
                    notice.clone(),
                    Style::default().fg(Color::Yellow),
                )));
            }
            render_detail_panel(frame, area, "Overview", screen, lines);
            return;
        }
        let mut lines = vec![Line::from("Status    available"), Line::from("")];
        append_publication_age(&mut lines, screen, now_epoch);
        let width = area.width.saturating_sub(8).max(8) as usize;
        for &index in &order {
            append_overview_account(&mut lines, &screen.accounts[index], width, now_epoch);
        }
        append_publication_issues(&mut lines, screen, now_epoch);
        if let Some(notice) = &screen.notice {
            lines.push(Line::from(Span::styled(
                notice.clone(),
                Style::default().fg(Color::Yellow),
            )));
        }
        render_detail_panel(frame, area, "Overview", screen, lines);
        return;
    };
    let title = if screen.detail { "Account" } else { "Overview" };
    let mut lines = vec![
        Line::from(format!("Provider  {}", account.provider)),
        Line::from(format!("Account   {}", account.account)),
        Line::from(format!("Status    {}", account.status)),
    ];
    append_publication_age(&mut lines, screen, now_epoch);
    if let Some(plan) = non_empty_label(account.plan_label.as_ref()) {
        lines.push(Line::from(format!("Plan      {plan}")));
    }
    if let Some(identity) = account.identity_kind.map(identity_kind_label) {
        lines.push(Line::from(format!("Identity  {identity}")));
    }
    if let Some(expires_at) = account.credential_expires_at_epoch {
        lines.push(Line::from(format!(
            "Credential {}",
            credential_expiry_label(now_epoch, expires_at)
        )));
    }
    let mut freshness = freshness_age_label(now_epoch, account);
    if let Some(retry_at) = account.retry_at_epoch {
        freshness.push_str(&format!(
            " · retry {}",
            relative_time_label(now_epoch, retry_at)
        ));
    }
    lines.push(Line::from(format!("Freshness {freshness}")));
    lines.push(Line::from(""));
    lines.push(Line::from("Limits"));
    let width = area.width.saturating_sub(8).max(8) as usize;
    if screen.detail {
        append_account_full_body(&mut lines, account, width, now_epoch);
    } else {
        append_account_summary_body(&mut lines, account, width, now_epoch);
    }
    append_publication_issues(&mut lines, screen, now_epoch);
    if let Some(notice) = &screen.notice {
        lines.push(Line::from(Span::styled(
            notice.clone(),
            Style::default().fg(Color::Yellow),
        )));
    }
    render_detail_panel(frame, area, title, screen, lines);
}

fn render_detail_panel(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &'static str,
    screen: &super::UsageScreenState,
    lines: Vec<Line<'static>>,
) {
    let block = panel(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let areas = super::geometry::detail_areas(inner);
    let status = if screen.refreshing() || (screen.accounts.is_empty() && screen.loading()) {
        refreshing_line()
    } else {
        Line::default()
    };
    frame.render_widget(Paragraph::new(status), areas.status);
    if areas.body.width > 0 && areas.body.height > 0 {
        frame.render_widget(
            Paragraph::new(lines)
                .scroll((screen.scroll, 0))
                .wrap(Wrap { trim: false }),
            areas.body,
        );
    }
}
