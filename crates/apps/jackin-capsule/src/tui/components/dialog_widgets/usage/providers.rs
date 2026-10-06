// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Identity header and overview-provider section line builders.

use super::{
    USAGE_CONTENT_PAD_LEFT, USAGE_CONTENT_PAD_RIGHT, usage_content_indent, usage_separator_line,
};
use ratatui::{
    style::Style,
    text::{Line, Span},
};

pub(crate) fn is_overview_provider_row(value: &str) -> bool {
    value.split(" || ").count() == 3
}

pub(crate) fn is_overview_provider_label(label: &str) -> bool {
    // Tab labels carry a ` · {account}` suffix so same-provider accounts stay
    // individually visible; match on the provider head.
    let head = label.split(" · ").next().unwrap_or(label);
    matches!(
        head,
        "OpenAI"
            | "Anthropic"
            | "Amp"
            | "xAI"
            | "Z.AI"
            | "Kimi"
            | "MiniMax"
            | "Cursor"
            | "OpenRouter"
            | "Muse"
            | "Copilot"
            | "Antigravity"
            | "Gemini"
            | "OpenCode"
    )
}

pub(crate) fn usage_legacy_overview_provider_lines(
    label: &str,
    value: &str,
    lines: &mut Vec<Line<'static>>,
) {
    let parts = value.split(" || ").collect::<Vec<_>>();
    if parts.len() != 3 {
        return;
    }
    let account = parts[0];
    let plan = parts[1];
    let status = parts[2];
    lines.push(Line::from(vec![
        usage_content_indent(),
        Span::styled(
            label.to_owned(),
            termrock::style::DesignSystem::default().style(termrock::style::Role::TextStrong),
        ),
        Span::raw("  "),
        Span::styled(
            account.to_owned(),
            Style::default().fg(termrock::style::DesignSystem::default()
                .style(termrock::style::Role::Text)
                .fg
                .unwrap_or_default()),
        ),
        Span::raw("  "),
        Span::styled(
            plan.to_owned(),
            termrock::style::DesignSystem::default().style(termrock::style::Role::TextMuted),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::raw(" ".repeat(USAGE_CONTENT_PAD_LEFT + 2)),
        Span::styled(
            status.to_owned(),
            termrock::style::DesignSystem::default().style(termrock::style::Role::TextMuted),
        ),
    ]));
}

pub(crate) fn usage_overview_provider_lines(
    label: &str,
    value: &str,
    width: usize,
    lines: &mut Vec<Line<'static>>,
) {
    let value = value.trim();
    let (summary, reset) = value.split_once(" · ").unwrap_or((value, ""));
    let left = if summary.ends_with("% left") {
        format!("{label:<11}{summary:>9}")
    } else {
        format!("{label:<11}{summary}")
    };
    let (reset, local_timestamp) = usage_overview_reset_columns(reset);
    let Some(local_timestamp) = local_timestamp else {
        lines.push(usage_header_two_column(
            &left,
            Style::default().fg(termrock::style::DesignSystem::default()
                .style(termrock::style::Role::Text)
                .fg
                .unwrap_or_default()),
            reset,
            termrock::style::DesignSystem::default().style(termrock::style::Role::TextMuted),
            width,
        ));
        return;
    };
    let left_cols = termrock::text::display_cols(&left);
    let reset_cols = termrock::text::display_cols(reset);
    let local_cols = termrock::text::display_cols(local_timestamp);
    let available = width.saturating_sub(USAGE_CONTENT_PAD_LEFT + USAGE_CONTENT_PAD_RIGHT);
    let left_gap = 3;
    let right_gap = available
        .checked_sub(left_cols + left_gap + reset_cols + local_cols)
        .filter(|gap| *gap >= 1)
        .unwrap_or(3);
    lines.push(Line::from(vec![
        usage_content_indent(),
        Span::styled(
            left,
            Style::default().fg(termrock::style::DesignSystem::default()
                .style(termrock::style::Role::Text)
                .fg
                .unwrap_or_default()),
        ),
        Span::raw(" ".repeat(left_gap)),
        Span::styled(
            reset.to_owned(),
            termrock::style::DesignSystem::default().style(termrock::style::Role::TextMuted),
        ),
        Span::raw(" ".repeat(right_gap)),
        Span::styled(
            local_timestamp.to_owned(),
            termrock::style::DesignSystem::default().style(termrock::style::Role::TextMuted),
        ),
    ]));
}

pub(crate) fn usage_overview_reset_columns(reset: &str) -> (&str, Option<&str>) {
    let reset = reset.trim();
    if let Some((prefix, suffix)) = reset.rsplit_once(" (")
        && suffix.ends_with(')')
    {
        let timestamp = &reset[reset.len() - suffix.len() - 2..];
        return (prefix.trim(), Some(timestamp));
    }
    (reset, None)
}

pub(crate) fn usage_identity_lines(
    provider: &str,
    account: Option<&str>,
    activity: Option<&str>,
    width: usize,
    lines: &mut Vec<Line<'static>>,
) {
    let account = account.map(str::trim).filter(|value| !value.is_empty());
    lines.push(usage_header_two_column(
        provider,
        termrock::style::DesignSystem::default().style(termrock::style::Role::TextStrong),
        account.unwrap_or(""),
        termrock::style::DesignSystem::default().style(termrock::style::Role::TextStrong),
        width,
    ));

    if let Some(activity) = activity.map(str::trim).filter(|value| !value.is_empty()) {
        lines.push(usage_header_two_column(
            activity,
            termrock::style::DesignSystem::default().style(termrock::style::Role::TextMuted),
            "",
            termrock::style::DesignSystem::default().style(termrock::style::Role::TextMuted),
            width,
        ));
    }

    lines.push(usage_separator_line(width));
}

/// Build a header line with `left` flush-left and `right` flush-right to
/// `width`. Falls back to a fixed three-space gap when `width` is 0 (the
/// measurement path) or too narrow to right-align without overlap.
pub(crate) fn usage_header_two_column(
    left: &str,
    left_style: Style,
    right: &str,
    right_style: Style,
    width: usize,
) -> Line<'static> {
    let left_cols = termrock::text::display_cols(left);
    let right_cols = termrock::text::display_cols(right);
    let gap = width
        .checked_sub(USAGE_CONTENT_PAD_LEFT + USAGE_CONTENT_PAD_RIGHT + left_cols + right_cols)
        .filter(|gap| *gap >= 1)
        .unwrap_or(3);
    let mut spans = vec![
        usage_content_indent(),
        Span::styled(left.to_owned(), left_style),
    ];
    if !right.is_empty() {
        spans.push(Span::raw(" ".repeat(gap)));
        spans.push(Span::styled(right.to_owned(), right_style));
    }
    Line::from(spans)
}
