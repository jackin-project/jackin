// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{
    UsageAccount, UsageMetricGroup, UsageWindow, credential_expiry_label, freshness_age_label,
    group_freshness_label, issue_text, metric_group_kind_label, metric_group_value_summary,
    metric_scope_summary, non_empty_label, quota_state_label, raw_percent_note,
    relative_time_label,
};

use jackin_protocol::usage_broker::{UsageMetricValueV1, UsageQuotaStateV1};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders},
};

pub(crate) fn append_account_full_body(
    lines: &mut Vec<Line<'static>>,
    account: &UsageAccount,
    width: usize,
    now_epoch: i64,
) {
    for window in &account.windows {
        lines.push(Line::from(Span::styled(
            format!("  {}", window.label),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )));
        if let Some(bar) = meter_line(width, window.meter_percent()) {
            lines.push(Line::from(Span::styled(
                bar,
                meter_style(window.quota_state),
            )));
        }
        let detail = if window.reset.is_empty() {
            format!("  {}", window.value)
        } else {
            format!("  {} · {}", window.value, window.reset)
        };
        lines.push(Line::from(detail));
        append_window_extra(lines, window);
        lines.push(Line::from(""));
    }
    if !account.metric_groups.is_empty() {
        lines.push(Line::from("Metric groups"));
        for group in &account.metric_groups {
            append_metric_group(lines, group, width, now_epoch);
        }
    }
    if !account.issues.is_empty() || !account.provider_issues.is_empty() {
        lines.push(Line::from("Issues"));
        append_account_issue_lines(lines, account, now_epoch);
        lines.push(Line::from(""));
    }
    lines.push(Line::from(Span::styled(
        "Enter for summary",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(""));
}

pub(crate) fn append_account_summary_body(
    lines: &mut Vec<Line<'static>>,
    account: &UsageAccount,
    width: usize,
    now_epoch: i64,
) {
    for window in &account.windows {
        append_summary_window(lines, window, width);
    }
    for group in &account.metric_groups {
        append_overview_group(lines, group, now_epoch);
    }
    let issue_count = account.issue_count();
    if issue_count > 0 {
        lines.push(Line::from(format!(
            "  {issue_count} issue{} · Enter for detail",
            if issue_count == 1 { "" } else { "s" }
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Enter for full detail",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(""));
}

pub(crate) fn refreshing_line() -> Line<'static> {
    Line::from(Span::styled(
        "Refreshing usage…",
        Style::default().fg(Color::DarkGray),
    ))
}

pub(crate) fn append_window_extra(lines: &mut Vec<Line<'static>>, window: &UsageWindow) {
    if window.quota_state != UsageQuotaStateV1::Available {
        lines.push(Line::from(format!(
            "  quota: {}",
            quota_state_label(window.quota_state)
        )));
    }
    if let Some(note) = raw_percent_note(window) {
        lines.push(Line::from(format!("  {note}")));
    }
    if let Some(pace) = non_empty_label(window.pace_label.as_ref()) {
        lines.push(Line::from(format!("  pace: {pace}")));
    }
}

pub(crate) fn append_account_issue_lines(
    lines: &mut Vec<Line<'static>>,
    account: &UsageAccount,
    now_epoch: i64,
) {
    for issue in &account.issues {
        lines.push(Line::from(Span::styled(
            format!("  {}", issue_text(issue, now_epoch)),
            Style::default().fg(Color::Yellow),
        )));
    }
    for issue in &account.provider_issues {
        lines.push(Line::from(Span::styled(
            format!("  provider: {}", issue_text(issue, now_epoch)),
            Style::default().fg(Color::Yellow),
        )));
    }
}

pub(crate) fn append_summary_window(
    lines: &mut Vec<Line<'static>>,
    window: &UsageWindow,
    width: usize,
) {
    if !window.label.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("  {}", window.label),
            Style::default().fg(Color::DarkGray),
        )));
    }
    if let Some(bar) = meter_line(width, window.meter_percent()) {
        lines.push(Line::from(Span::styled(
            bar,
            meter_style(window.quota_state),
        )));
    }
    let detail = if window.reset.is_empty() {
        format!("  {}", window.value)
    } else {
        format!("  {} · {}", window.value, window.reset)
    };
    lines.push(Line::from(detail));
}

pub(crate) fn append_overview_window(
    lines: &mut Vec<Line<'static>>,
    window: &UsageWindow,
    width: usize,
) {
    if !window.label.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("  {}", window.label),
            Style::default().fg(Color::DarkGray),
        )));
    }
    if let Some(bar) = meter_line(width, window.meter_percent()) {
        lines.push(Line::from(Span::styled(
            bar,
            meter_style(window.quota_state),
        )));
    }
    let detail = if window.reset.is_empty() {
        format!("  {}", window.value)
    } else {
        format!("  {} · {}", window.value, window.reset)
    };
    lines.push(Line::from(detail));
    append_window_extra(lines, window);
}

pub(crate) fn append_overview_group(
    lines: &mut Vec<Line<'static>>,
    group: &UsageMetricGroup,
    now_epoch: i64,
) {
    match metric_group_value_summary(group) {
        Some(summary) => lines.push(Line::from(format!("  {}: {summary}", group.label))),
        None => lines.push(Line::from(format!(
            "  {} ({} · {})",
            group.label,
            metric_group_kind_label(group.kind),
            quota_state_label(group.quota_state),
        ))),
    }
    append_group_schedule_lines(lines, group, now_epoch);
    for issue in &group.issues {
        lines.push(Line::from(Span::styled(
            format!("  [{}] {}", group.label, issue_text(issue, now_epoch)),
            Style::default().fg(Color::Yellow),
        )));
    }
}

pub(crate) fn append_group_schedule_lines(
    lines: &mut Vec<Line<'static>>,
    group: &UsageMetricGroup,
    now_epoch: i64,
) {
    if let Some(reset_at) = group.reset_at_epoch {
        lines.push(Line::from(format!(
            "  resets {}",
            relative_time_label(now_epoch, reset_at)
        )));
    }
    if let Some(renews_at) = group.renews_at_epoch {
        lines.push(Line::from(format!(
            "  renews {}",
            relative_time_label(now_epoch, renews_at)
        )));
    }
    if let UsageMetricValueV1::Balance {
        expires_at_epoch: Some(expires_at),
        ..
    } = &group.value
    {
        lines.push(Line::from(format!(
            "  expires {}",
            relative_time_label(now_epoch, *expires_at)
        )));
    }
}

pub(crate) fn append_metric_group(
    lines: &mut Vec<Line<'static>>,
    group: &UsageMetricGroup,
    width: usize,
    now_epoch: i64,
) {
    lines.push(Line::from(Span::styled(
        format!(
            "  {} ({} · {} · {})",
            group.label,
            metric_group_kind_label(group.kind),
            quota_state_label(group.quota_state),
            group_freshness_label(now_epoch, group),
        ),
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )));
    if let Some(scope) = metric_scope_summary(&group.scope) {
        lines.push(Line::from(Span::styled(
            format!("  scope: {scope}"),
            Style::default().fg(Color::DarkGray),
        )));
    }
    if let Some(bar) = meter_line(width, group.meter_percent()) {
        lines.push(Line::from(Span::styled(
            bar,
            meter_style(group.quota_state),
        )));
    }
    if let Some(summary) = metric_group_value_summary(group) {
        lines.push(Line::from(format!("  {summary}")));
    }
    append_group_schedule_lines(lines, group, now_epoch);
    let mut fetched = format!(
        "  fetched {}",
        relative_time_label(now_epoch, group.fetched_at_epoch)
    );
    if let Some(observed_at) = group.observed_at_epoch {
        fetched.push_str(&format!(
            " · observed {}",
            relative_time_label(now_epoch, observed_at)
        ));
    }
    lines.push(Line::from(Span::styled(
        fetched,
        Style::default().fg(Color::DarkGray),
    )));
    for issue in &group.issues {
        lines.push(Line::from(Span::styled(
            format!("  {}", issue_text(issue, now_epoch)),
            Style::default().fg(Color::Yellow),
        )));
    }
    lines.push(Line::from(""));
}

pub(crate) fn append_overview_account(
    lines: &mut Vec<Line<'static>>,
    account: &UsageAccount,
    width: usize,
    now_epoch: i64,
) {
    lines.push(Line::from(Span::styled(
        format!("{} · {}", account.provider, account.account),
        Style::default().fg(Color::White),
    )));
    lines.push(Line::from(Span::styled(
        format!("  {}", freshness_age_label(now_epoch, account)),
        Style::default().fg(Color::DarkGray),
    )));
    if let Some(plan) = non_empty_label(account.plan_label.as_ref()) {
        lines.push(Line::from(format!("  Plan {plan}")));
    }
    if let Some(expires_at) = account.credential_expires_at_epoch {
        lines.push(Line::from(format!(
            "  Credential {}",
            credential_expiry_label(now_epoch, expires_at)
        )));
    }
    if account.windows.is_empty() && account.metric_groups.is_empty() {
        lines.push(Line::from(format!("  {}", account.status)));
    } else {
        for window in &account.windows {
            append_overview_window(lines, window, width);
        }
        for group in &account.metric_groups {
            append_overview_group(lines, group, now_epoch);
        }
    }
    append_account_issue_lines(lines, account, now_epoch);
    lines.push(Line::from(""));
}

pub(crate) fn panel(title: &'static str) -> Block<'static> {
    Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
}

pub(crate) fn row_style(selected: bool) -> Style {
    if selected {
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    }
}

pub(crate) fn meter_line(width: usize, percent: Option<u8>) -> Option<String> {
    let percent = usize::from(percent?.min(100));
    let filled = width.saturating_mul(percent) / 100;
    Some(format!(
        "  {}{}",
        "█".repeat(filled),
        "░".repeat(width.saturating_sub(filled))
    ))
}

pub(crate) fn meter_style(quota_state: UsageQuotaStateV1) -> Style {
    match quota_state {
        UsageQuotaStateV1::Exhausted | UsageQuotaStateV1::Error => Style::default().fg(Color::Red),
        UsageQuotaStateV1::Warning => Style::default().fg(Color::Yellow),
        _ => Style::default().fg(Color::Green),
    }
}
