// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Usage publication age and issues.

use super::{UsageScreenState, issue_text, past_age_label};

use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

pub(crate) fn append_publication_age(
    lines: &mut Vec<Line<'static>>,
    screen: &UsageScreenState,
    now_epoch: i64,
) {
    if let Some(generated_at) = screen.generated_at_epoch {
        lines.push(Line::from(format!(
            "Snapshot  {}",
            past_age_label(now_epoch.saturating_sub(generated_at).max(0))
        )));
    }
}

pub(crate) fn empty_publication_lines(
    screen: &UsageScreenState,
    now_epoch: i64,
) -> Vec<Line<'static>> {
    let message = if screen.loading() {
        "Refreshing usage…"
    } else if screen.notice.is_some() || publication_has_issues(screen) {
        "Usage unavailable."
    } else {
        "No providers configured."
    };
    let mut lines = vec![Line::from(message), Line::from("")];
    append_publication_age(&mut lines, screen, now_epoch);
    append_publication_issues(&mut lines, screen, now_epoch);
    if let Some(notice) = &screen.notice {
        lines.push(Line::from(notice.clone()));
    }
    lines.push(Line::from("Press R to refresh."));
    lines
}

pub(crate) fn publication_has_issues(screen: &UsageScreenState) -> bool {
    !screen.projection_issues.is_empty()
        || screen
            .canonical_projection
            .as_ref()
            .is_some_and(|projection| {
                projection
                    .providers
                    .iter()
                    .any(|provider| !provider.issues.is_empty())
            })
}

pub(crate) fn append_publication_issues(
    lines: &mut Vec<Line<'static>>,
    screen: &UsageScreenState,
    now_epoch: i64,
) {
    for issue in &screen.projection_issues {
        lines.push(Line::from(Span::styled(
            issue_text(issue, now_epoch),
            Style::default().fg(Color::Yellow),
        )));
    }
    if let Some(projection) = &screen.canonical_projection {
        for provider in &projection.providers {
            if screen
                .visible_order()
                .iter()
                .any(|&index| screen.accounts[index].provider_id == provider.provider_id)
            {
                continue;
            }
            for issue in &provider.issues {
                lines.push(Line::from(Span::styled(
                    format!(
                        "{}: {}",
                        provider.display_name,
                        issue_text(issue, now_epoch)
                    ),
                    Style::default().fg(Color::Yellow),
                )));
            }
        }
    }
}
