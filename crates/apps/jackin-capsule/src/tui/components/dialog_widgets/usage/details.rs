// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Bucket detail parsing, quota predicates, and compact narrow rows.

use super::{usage_content_indent, usage_meter_char};
use ratatui::{
    style::Style,
    text::{Line, Span},
};

pub(crate) fn usage_stacked_bucket_detail_rows(
    remaining_label: Option<String>,
    details: &[String],
) -> Vec<(String, String)> {
    let mut left = Vec::new();
    let mut right = Vec::new();
    let mut lasts_until_reset = false;
    if let Some(label) = remaining_label {
        left.push(label);
    }
    for detail in details {
        if detail.starts_with("Resets") || detail.starts_with("Runs out") {
            right.push(detail.clone());
        } else if !left.iter().any(|existing| existing == detail) {
            if detail == "On pace" || detail.ends_with(" in reserve") {
                lasts_until_reset = true;
            }
            left.push(detail.clone());
        }
    }
    if lasts_until_reset
        && right.iter().any(|detail| detail.starts_with("Resets"))
        && !right.iter().any(|detail| detail.starts_with("Runs out"))
    {
        right.push("Lasts until reset".to_owned());
    } else if right.is_empty() && left.len() > 1 {
        right.push(String::new());
    }
    let len = left.len().max(right.len());
    (0..len)
        .map(|index| {
            (
                left.get(index).cloned().unwrap_or_default(),
                right.get(index).cloned().unwrap_or_default(),
            )
        })
        .filter(|(left, right)| !left.is_empty() || !right.is_empty())
        .collect()
}

pub(crate) fn usage_quota_bucket_compact_lines(
    label: &str,
    value: &str,
    width: usize,
    lines: &mut Vec<Line<'static>>,
) {
    let details = usage_quota_bucket_detail_parts(label, value);
    let detail = if details.is_empty() {
        "status unavailable".to_owned()
    } else {
        // Narrow layout keeps only remaining + reset (e.g. "37% left · Resets
        // in 1h 21m"); pace and other tokens drop out to fit the width.
        let remaining = details.first().cloned();
        let reset = details.iter().find(|part| part.contains("Resets")).cloned();
        let kept = remaining.into_iter().chain(reset).collect::<Vec<_>>();
        if kept.is_empty() {
            details.join(" · ")
        } else {
            kept.join(" · ")
        }
    };
    let detail = compact_bucket_detail_for_width(label, &detail, width);
    lines.push(Line::from(vec![
        usage_content_indent(),
        Span::styled(
            label.to_owned(),
            termrock::style::DesignSystem::default().style(termrock::style::Role::TextStrong),
        ),
        Span::styled(
            "  ",
            termrock::style::DesignSystem::default().style(termrock::style::Role::TextMuted),
        ),
        Span::styled(
            detail,
            Style::default().fg(termrock::style::DesignSystem::default()
                .style(termrock::style::Role::Text)
                .fg
                .unwrap_or_default()),
        ),
    ]));
}

pub(crate) fn compact_bucket_detail_for_width(label: &str, detail: &str, width: usize) -> String {
    if width == 0 {
        return detail.to_owned();
    }
    let prefix_cols = 2 + termrock::text::display_cols(label) + 2;
    let Some(detail_cols) = width.checked_sub(prefix_cols) else {
        return String::new();
    };
    truncate_display_with_ellipsis(detail, detail_cols)
}

pub(crate) fn truncate_display_with_ellipsis(value: &str, width: usize) -> String {
    if termrock::text::display_cols(value) <= width {
        return value.to_owned();
    }
    if width == 0 {
        return String::new();
    }
    if width == 1 {
        return "…".to_owned();
    }
    format!("{}…", termrock::text::take_display_cols(value, width - 1))
}

pub(crate) fn usage_quota_bucket_detail_parts(label: &str, value: &str) -> Vec<String> {
    let parts = value
        .split(" · ")
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>();
    if parts.is_empty() {
        return Vec::new();
    }

    let (_meter, remaining_label) = usage_meter_parts(parts[0]);
    let details = remaining_label
        .into_iter()
        .chain(parts.iter().skip(1).copied())
        .flat_map(|detail| detail.split(" · "))
        .filter(|detail| !detail.trim().is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if label == "Extra usage" {
        usage_extra_usage_details(details)
    } else {
        details
    }
}

pub(crate) fn usage_extra_usage_details(details: Vec<String>) -> Vec<String> {
    let mut used = Vec::new();
    let mut rest = Vec::new();
    for detail in details {
        if detail.ends_with("% used") {
            used.push(detail);
        } else {
            rest.push(detail);
        }
    }
    used.extend(rest);
    used
}

pub(crate) fn usage_meter_parts(value: &str) -> (&str, Option<&str>) {
    value
        .split_once(' ')
        .filter(|(meter, _)| meter.chars().all(usage_meter_char))
        .map_or((value, None), |(meter, label)| (meter, Some(label)))
}

pub(crate) fn is_quota_bucket_row(label: &str, value: &str) -> bool {
    is_known_quota_bucket(label) || quota_value_has_meter(value)
}

pub(crate) fn is_known_quota_bucket(label: &str) -> bool {
    matches!(
        label,
        "Session"
            | "Weekly"
            | "Credits"
            | "Sonnet"
            | "Opus"
            | "Daily Routines"
            | "Extra usage"
            | "Tokens"
            | "MCP"
            | "5-hour"
            | "Amp Free"
            | "Individual credits"
            | "Limit Reset Credits"
            | "Rate Limit"
    ) || label.starts_with("Codex Spark")
        || label.ends_with("rate limit")
        || label.ends_with("Coding plan")
}

pub(crate) fn quota_value_has_meter(value: &str) -> bool {
    usage_meter_parts(value).1.is_some()
}
