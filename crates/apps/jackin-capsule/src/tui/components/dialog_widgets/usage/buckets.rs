// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Quota-bucket sections with separators and full-width meter scaling.

use super::{
    USAGE_METER_EMPTY, USAGE_METER_FILLED, usage_content_indent, usage_content_width,
    usage_header_two_column, usage_meter_char, usage_meter_parts, usage_quota_bucket_detail_parts,
    usage_stacked_bucket_detail_rows,
};
use ratatui::{
    style::Style,
    text::{Line, Span},
};

pub(crate) fn usage_quota_bucket_lines(
    label: &str,
    value: &str,
    accent: Option<ratatui::style::Color>,
    width: usize,
    lines: &mut Vec<Line<'static>>,
) {
    if label == "Limit Reset Credits" {
        usage_limit_reset_credit_lines(value, width, lines);
        return;
    }

    let display_label = usage_bucket_display_label(label, value);
    if is_usage_separated_section(label) {
        push_usage_separator(lines, width);
    } else {
        push_usage_section_gap(lines);
    }
    lines.push(Line::from(vec![
        usage_content_indent(),
        Span::styled(
            display_label,
            termrock::style::DesignSystem::default().style(termrock::style::Role::TextStrong),
        ),
    ]));

    let Some(first) = value.split(" · ").find(|part| !part.trim().is_empty()) else {
        return;
    };

    let (meter, remaining_label) = usage_meter_parts(first);
    if remaining_label.is_none() {
        lines.push(usage_header_two_column(
            first,
            Style::default().fg(termrock::style::DesignSystem::default()
                .style(termrock::style::Role::Text)
                .fg
                .unwrap_or_default()),
            "",
            termrock::style::DesignSystem::default().style(termrock::style::Role::TextMuted),
            width,
        ));
        return;
    }

    let meter = usage_full_width_meter(meter, width);
    lines.push(Line::from(vec![
        usage_content_indent(),
        Span::styled(
            meter,
            Style::default().fg(accent.unwrap_or(
                termrock::style::DesignSystem::default()
                    .style(termrock::style::Role::Accent)
                    .fg
                    .unwrap_or_default(),
            )),
        ),
    ]));

    let details = usage_quota_bucket_detail_parts(label, value);
    let rows = if label == "Credits" {
        usage_credit_bucket_detail_rows(remaining_label.map(str::to_owned), &details)
    } else {
        usage_stacked_bucket_detail_rows(remaining_label.map(str::to_owned), &details)
    };
    for (left, right) in rows {
        lines.push(usage_header_two_column(
            &left,
            Style::default().fg(termrock::style::DesignSystem::default()
                .style(termrock::style::Role::Text)
                .fg
                .unwrap_or_default()),
            &right,
            termrock::style::DesignSystem::default().style(termrock::style::Role::TextMuted),
            width,
        ));
    }
}

pub(crate) fn usage_credit_bucket_detail_rows(
    remaining_label: Option<String>,
    details: &[String],
) -> Vec<(String, String)> {
    let left = remaining_label.unwrap_or_default();
    let right = details
        .iter()
        .find(|detail| **detail != left)
        .cloned()
        .unwrap_or_default();
    vec![(left, right)]
        .into_iter()
        .filter(|(left, right)| !left.is_empty() || !right.is_empty())
        .collect()
}

pub(crate) fn usage_limit_reset_credit_lines(
    value: &str,
    width: usize,
    lines: &mut Vec<Line<'static>>,
) {
    push_usage_separator(lines, width);
    let parts = value
        .split(" · ")
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>();
    let right = parts.first().copied().unwrap_or_default();
    lines.push(usage_header_two_column(
        "Limit Reset Credits",
        termrock::style::DesignSystem::default().style(termrock::style::Role::TextStrong),
        right,
        termrock::style::DesignSystem::default().style(termrock::style::Role::TextMuted),
        width,
    ));
    for detail in parts.iter().skip(1) {
        lines.push(usage_header_two_column(
            detail,
            Style::default().fg(termrock::style::DesignSystem::default()
                .style(termrock::style::Role::Text)
                .fg
                .unwrap_or_default()),
            "",
            termrock::style::DesignSystem::default().style(termrock::style::Role::TextMuted),
            width,
        ));
    }
}

pub(crate) fn usage_bucket_display_label(label: &str, value: &str) -> String {
    if label == "Individual credits" && value.starts_with("Individual credits: ") {
        "Credits".to_owned()
    } else {
        label.to_owned()
    }
}

pub(crate) fn is_usage_separated_section(label: &str) -> bool {
    matches!(
        label,
        "Credits" | "Individual credits" | "Limit Reset Credits"
    )
}

pub(crate) fn push_usage_section_gap(lines: &mut Vec<Line<'static>>) {
    if lines
        .last()
        .is_none_or(|line| !usage_line_is_blank(line) && !usage_line_is_separator(line))
    {
        lines.push(Line::from(""));
    }
}

pub(crate) fn push_usage_separator(lines: &mut Vec<Line<'static>>, width: usize) {
    if !lines.last().is_some_and(usage_line_is_separator) {
        lines.push(usage_separator_line(width));
    }
}

pub(crate) fn usage_separator_line(width: usize) -> Line<'static> {
    let target = width.max(1);
    Line::from(vec![Span::styled(
        "─".repeat(target),
        termrock::style::DesignSystem::default().style(termrock::style::Role::TextMuted),
    )])
}

pub(crate) fn usage_line_is_blank(line: &Line<'_>) -> bool {
    line.spans
        .iter()
        .all(|span| span.content.as_ref().trim().is_empty())
}

pub(crate) fn usage_line_is_separator(line: &Line<'_>) -> bool {
    let text = line
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>();
    let trimmed = text.trim();
    !trimmed.is_empty() && trimmed.chars().all(|ch| ch == '─')
}

pub(crate) fn usage_full_width_meter(meter: &str, width: usize) -> String {
    let target = usage_content_width(width).max(1);
    let filled = meter.chars().filter(|ch| *ch == USAGE_METER_FILLED).count();
    let total = meter
        .chars()
        .filter(|ch| usage_meter_char(*ch))
        .count()
        .max(1);
    let filled_cols = if filled >= total {
        target
    } else {
        filled.saturating_mul(target) / total
    };
    let filled_cols = filled_cols.min(target);
    format!(
        "{}{}",
        USAGE_METER_FILLED.to_string().repeat(filled_cols),
        USAGE_METER_EMPTY
            .to_string()
            .repeat(target.saturating_sub(filled_cols))
    )
}
