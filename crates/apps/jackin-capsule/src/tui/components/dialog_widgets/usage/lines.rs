// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Info-line composition core: row dispatch and content primitives.

use super::{
    is_overview_provider_label, is_overview_provider_row, is_quota_bucket_row,
    usage_identity_lines, usage_legacy_overview_provider_lines, usage_overview_provider_lines,
    usage_quota_bucket_compact_lines, usage_quota_bucket_lines, usage_separator_line,
};
use ratatui::{
    style::Style,
    text::{Line, Span},
};

pub(crate) fn usage_info_lines(
    state: &crate::tui::components::container_info_surface::ContainerInfoState,
) -> Vec<Line<'static>> {
    // Width 0 disables right-alignment so content-size/height measurement
    // reflects the intrinsic line width, not a padded-to-panel width.
    usage_info_lines_impl(state, false, 0)
}

pub(crate) fn usage_info_lines_for_width(
    state: &crate::tui::components::container_info_surface::ContainerInfoState,
    width: u16,
) -> Vec<Line<'static>> {
    // Below 64 cols the two-column rows can no longer right-align without the
    // left and right halves overlapping, so fall back to a single-column list.
    usage_info_lines_impl(state, width < 64, width)
}

pub(crate) fn usage_info_lines_impl(
    state: &crate::tui::components::container_info_surface::ContainerInfoState,
    list_layout: bool,
    width: u16,
) -> Vec<Line<'static>> {
    let mut lines = Vec::with_capacity(state.rows().len().saturating_mul(2).saturating_add(1));
    let context = UsageLineContext {
        provider: usage_row_value(
            state,
            crate::tui::components::dialog::USAGE_IDENTITY_PROVIDER_ROW,
        ),
        account: usage_row_value(
            state,
            crate::tui::components::dialog::USAGE_IDENTITY_ACCOUNT_ROW,
        ),
        activity: usage_row_value(
            state,
            crate::tui::components::dialog::USAGE_IDENTITY_ACTIVITY_ROW,
        ),
        list_layout,
        width: width as usize,
    };
    if list_layout {
        lines.push(Line::from(""));
    } else {
        lines.push(usage_separator_line(context.width));
    }
    if let Some(provider) = context.provider {
        usage_identity_lines(
            provider,
            context.account,
            context.activity,
            context.width,
            &mut lines,
        );
    }
    for row in state.rows() {
        usage_lines_for_row(
            row.label(),
            row.value(),
            row.accent_color(),
            context,
            &mut lines,
        );
    }
    lines
}

#[derive(Clone, Copy)]
pub(crate) struct UsageLineContext<'a> {
    provider: Option<&'a str>,
    account: Option<&'a str>,
    activity: Option<&'a str>,
    list_layout: bool,
    /// Panel inner width for right-aligned header fields; 0 disables alignment.
    width: usize,
}

pub(crate) const USAGE_CONTENT_PAD_LEFT: usize = 2;
pub(crate) const USAGE_CONTENT_PAD_RIGHT: usize = 2;
pub(crate) const USAGE_METER_FILLED: char = '█';
pub(crate) const USAGE_METER_EMPTY: char = '░';

pub(crate) fn usage_content_width(width: usize) -> usize {
    if width == 0 {
        return 0;
    }
    width
        .saturating_sub(USAGE_CONTENT_PAD_LEFT + USAGE_CONTENT_PAD_RIGHT)
        .max(1)
}

pub(crate) fn usage_content_indent() -> Span<'static> {
    Span::raw(" ".repeat(USAGE_CONTENT_PAD_LEFT))
}

pub(crate) fn usage_meter_char(ch: char) -> bool {
    matches!(ch, USAGE_METER_FILLED | USAGE_METER_EMPTY | '·')
}

pub(crate) fn usage_row_value<'a>(
    state: &'a crate::tui::components::container_info_surface::ContainerInfoState,
    label: &str,
) -> Option<&'a str> {
    state
        .rows()
        .iter()
        .find(|row| row.label() == label)
        .map(crate::tui::components::container_info_surface::ContainerInfoRow::value)
}

pub(crate) fn usage_line_width(line: &Line<'_>) -> usize {
    line.spans
        .iter()
        .map(|span| termrock::text::display_cols(span.content.as_ref()))
        .sum()
}

pub(crate) fn usage_lines_for_row(
    label: &str,
    value: &str,
    accent: Option<ratatui::style::Color>,
    context: UsageLineContext<'_>,
    lines: &mut Vec<Line<'static>>,
) {
    match label {
        crate::tui::components::dialog::USAGE_IDENTITY_PROVIDER_ROW
        | crate::tui::components::dialog::USAGE_IDENTITY_ACCOUNT_ROW
        | crate::tui::components::dialog::USAGE_IDENTITY_ACTIVITY_ROW => {}
        "Focused agent" | "Focused account" => {
            lines.push(Line::from(vec![
                usage_content_indent(),
                Span::styled(
                    value.to_owned(),
                    termrock::style::DesignSystem::default()
                        .style(termrock::style::Role::TextStrong),
                ),
            ]));
        }
        "Provider" | "Account" | "Status" | "Updated" | "Focused" | "Started" | "Today"
        | "Since start" => {}
        bucket if is_quota_bucket_row(bucket, value) => {
            if context.list_layout {
                usage_quota_bucket_compact_lines(bucket, value, context.width, lines);
            } else {
                usage_quota_bucket_lines(bucket, value, accent, context.width, lines);
            }
        }
        // Overview-tab rows only: the provider tab always carries the
        // identity header (`context.provider`), so a meter-less bucket whose
        // label head collides with a provider name (e.g. the Antigravity
        // "Gemini · Weekly" fallback row) must not take the overview arm and
        // render garbled (S4/S5 parity).
        _ if context.provider.is_none() && is_overview_provider_label(label) => {
            usage_overview_provider_lines(label, value, context.width, lines);
        }
        _ if context.provider.is_none() && is_overview_provider_row(value) => {
            usage_legacy_overview_provider_lines(label, value, lines);
        }
        _ => lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                format!("{label} "),
                termrock::style::DesignSystem::default().style(termrock::style::Role::TextMuted),
            ),
            Span::styled(
                value.to_owned(),
                Style::default().fg(termrock::style::DesignSystem::default()
                    .style(termrock::style::Role::Text)
                    .fg
                    .unwrap_or_default()),
            ),
        ])),
    }
}
