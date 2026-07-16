// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Usage-dialog rendering helpers extracted from `dialog_widgets.rs`.
//!
//! `dialog_widgets.rs` is the coordinator; this sub-module owns the per-line /
//! per-section content composition for the usage dialog. The helpers are
//! consumed by `render_usage_info` (in the coordinator) and the test helpers
//! in `tests.rs`; the API surface is re-exported at the parent so the test
//! glob continues to read `dialog_widgets::usage_xxx`.

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
};

use termrock::components::tab_strip::TabStrip;
use termrock::style::{DIM, PHOSPHOR_GREEN, WHITE};

pub(crate) fn usage_dialog_inner_area(area: Rect) -> Rect {
    Rect {
        x: area.x.saturating_add(1),
        y: area.y.saturating_add(1),
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    }
}

pub(crate) fn usage_tab_strip_area(inner: Rect, tabs: &[(String, bool)]) -> Rect {
    let strip_width = usage_tab_strip_width(tabs)
        .saturating_sub(usize::from(termrock::TAB_GAP))
        .min(usize::from(inner.width));
    let strip_offset = usize::from(inner.width).saturating_sub(strip_width) / 2;
    Rect {
        x: inner
            .x
            .saturating_add(u16::try_from(strip_offset).unwrap_or(u16::MAX)),
        y: inner.y,
        width: u16::try_from(strip_width)
            .unwrap_or(inner.width)
            .max(1)
            .min(inner.width),
        height: inner.height.min(2),
    }
}

pub(crate) fn usage_tab_strip_index_at(
    tabs: &[(String, bool)],
    tab_area: Rect,
    col: u16,
) -> Option<usize> {
    let tab_refs = tabs
        .iter()
        .map(|(label, active)| (label.as_str(), *active))
        .collect::<Vec<_>>();
    TabStrip::new(&tab_refs).hit_index_at(tab_area, col, tab_area.y)
}

pub(crate) fn usage_tab_strip_labels(
    view: &jackin_protocol::control::FocusedUsageView,
    selected: crate::tui::components::dialog::UsageDialogTab,
) -> Vec<(String, bool)> {
    let overview_active = selected == crate::tui::components::dialog::UsageDialogTab::Overview;
    let mut tabs = vec![("Overview".to_owned(), overview_active)];
    tabs.extend(view.tabs.iter().map(|tab| {
        (
            usage_provider_display_label(&tab.label).to_owned(),
            !overview_active && tab.active,
        )
    }));
    tabs
}

pub(crate) fn usage_provider_display_label(label: &str) -> &str {
    match label {
        "Codex" | "OpenAI / Codex" => "OpenAI",
        "Claude" | "Anthropic / Claude" => "Anthropic",
        "Grok Build" | "xAI / Grok" => "xAI",
        "GLM / Z.AI" => "Z.AI",
        other => other,
    }
}

pub(crate) fn usage_tab_strip_width(tabs: &[(String, bool)]) -> usize {
    let gap = usize::from(termrock::TAB_GAP);
    tabs.iter()
        .map(|(label, _)| termrock::display_cols(label) + 2 + gap)
        .sum()
}

/// Panel title. In the narrow list layout the provider-detail panel reads
/// `Usage: <provider>` (matching the narrow preview); the wide layout and the
/// Overview/Instance panels keep their own titles.
pub(crate) fn usage_panel_title(
    state: &crate::tui::components::container_info_surface::ContainerInfoState,
    width: u16,
) -> String {
    let base = state.title();
    // Below 68 cols the full `Usage` title plus the provider detail no longer
    // fits the panel border, so collapse to the short `Usage: <provider>` form.
    // This trips a few cols before the body switches to the single-column list
    // layout (< 64) so the title is already compact when the rows reflow.
    if width >= 68 || base != "Usage" {
        return base.to_owned();
    }
    if let Some(header) = usage_row_value(state, "Header") {
        let short = header.rsplit(" / ").next().unwrap_or(header).trim();
        if !short.is_empty() {
            return format!("Usage: {short}");
        }
    }
    base.to_owned()
}

pub(crate) fn usage_info_required_height(
    state: &crate::tui::components::container_info_surface::ContainerInfoState,
) -> u16 {
    // Add the fixed chrome rows that frame the content (borders, title, and
    // padding) on top of the content-line count, then keep a 7-row floor so the
    // box stays usable when a provider has only a line or two to show.
    u16::try_from(usage_info_lines(state).len())
        .unwrap_or(u16::MAX)
        .saturating_add(5)
        .max(7)
}

/// The usage-dialog body rect (border **and** the 2-row tab strip removed).
/// Single source of truth for body geometry so the renderer and every
/// scroll-bound computation agree on the viewport (Bug 2). The tab strip is a
/// fixed 2 rows — `usage_tab_strip_area`'s height is `inner.height.min(2)`,
/// independent of tab count — so the body needs no tab list to compute.
pub(crate) fn usage_body_rect(box_rect: Rect) -> Rect {
    let inner = usage_dialog_inner_area(box_rect);
    let tab_h = inner.height.min(2);
    Rect {
        x: inner.x,
        y: inner.y.saturating_add(tab_h),
        width: inner.width,
        height: inner.height.saturating_sub(tab_h),
    }
}

/// Content size + the rect to feed the generic scroll helpers
/// (`dialog_scroll_axes` / `clamp_dialog_scroll`), derived from the **same**
/// width-wrapped line set the renderer uses, so the scroll bound can never
/// under- or over-shoot the rendered body (Bug 2).
///
/// Returns `(content_width, content_height, scroll_rect)` where `content_height`
/// is the wrapped line count at the body width, and `scroll_rect` is sized so
/// that `viewport_height(scroll_rect) == body.height` and
/// `viewport_width(scroll_rect) == body.width` (those helpers subtract the
/// 1-cell border; `scroll_rect.height = body.height + 2` re-adds exactly that so
/// the true body viewport — box minus border minus tab strip — is what clamps).
pub(crate) fn usage_scroll_inputs(
    box_rect: Rect,
    state: &crate::tui::components::container_info_surface::ContainerInfoState,
) -> (usize, usize, Rect) {
    let body = usage_body_rect(box_rect);
    let lines = usage_info_lines_for_width(state, body.width);
    let content_width = lines.iter().map(usage_line_width).max().unwrap_or(0);
    let content_height = lines.len();
    let scroll_rect = Rect {
        height: body.height.saturating_add(2),
        ..box_rect
    };
    (content_width, content_height, scroll_rect)
}

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
        updated: usage_row_value(state, "Updated"),
        account: usage_row_value(state, "Account"),
        username: usage_row_value(state, "Username"),
        plan: usage_row_value(state, "Plan"),
        auth: usage_row_value(state, "Auth"),
        list_layout,
        width: width as usize,
    };
    if list_layout {
        lines.push(Line::from(""));
    } else {
        lines.push(usage_separator_line(context.width));
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
    updated: Option<&'a str>,
    account: Option<&'a str>,
    username: Option<&'a str>,
    plan: Option<&'a str>,
    auth: Option<&'a str>,
    list_layout: bool,
    /// Panel inner width for right-aligned header fields; 0 disables alignment.
    width: usize,
}

const USAGE_CONTENT_PAD_LEFT: usize = 2;
const USAGE_CONTENT_PAD_RIGHT: usize = 2;
const USAGE_METER_FILLED: char = '█';
const USAGE_METER_EMPTY: char = '░';

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
        .map(|span| termrock::display_cols(span.content.as_ref()))
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
        "Header" => {
            usage_header_lines(
                value,
                context.updated,
                context.account,
                context.username,
                context.plan,
                context.auth,
                context.width,
                lines,
            );
        }
        "Focused agent" | "Focused account" => {
            lines.push(Line::from(vec![
                usage_content_indent(),
                Span::styled(
                    value.to_owned(),
                    Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
                ),
            ]));
        }
        "Provider" | "Account" | "Username" | "Plan" | "Auth" | "Status" | "Updated"
        | "Focused" | "Started" | "Today" | "Since start" => {}
        bucket if is_quota_bucket_row(bucket, value) => {
            if context.list_layout {
                usage_quota_bucket_compact_lines(bucket, value, context.width, lines);
            } else {
                usage_quota_bucket_lines(bucket, value, accent, context.width, lines);
            }
        }
        _ if is_overview_provider_label(label) => {
            usage_overview_provider_lines(label, value, context.width, lines);
        }
        _ if is_overview_provider_row(value) => {
            usage_legacy_overview_provider_lines(label, value, lines);
        }
        _ => lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(format!("{label} "), DIM),
            Span::styled(value.to_owned(), Style::default().fg(WHITE)),
        ])),
    }
}

pub(crate) fn is_overview_provider_row(value: &str) -> bool {
    value.split(" || ").count() == 3
}

pub(crate) fn is_overview_provider_label(label: &str) -> bool {
    matches!(
        label,
        "OpenAI" | "Anthropic" | "Amp" | "xAI" | "Z.AI" | "Kimi" | "MiniMax"
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
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(account.to_owned(), Style::default().fg(WHITE)),
        Span::raw("  "),
        Span::styled(plan.to_owned(), DIM),
    ]));
    lines.push(Line::from(vec![
        Span::raw(" ".repeat(USAGE_CONTENT_PAD_LEFT + 2)),
        Span::styled(status.to_owned(), DIM),
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
            Style::default().fg(WHITE),
            reset,
            DIM,
            width,
        ));
        return;
    };
    let left_cols = termrock::display_cols(&left);
    let reset_cols = termrock::display_cols(reset);
    let local_cols = termrock::display_cols(local_timestamp);
    let available = width.saturating_sub(USAGE_CONTENT_PAD_LEFT + USAGE_CONTENT_PAD_RIGHT);
    let left_gap = 3;
    let right_gap = available
        .checked_sub(left_cols + left_gap + reset_cols + local_cols)
        .filter(|gap| *gap >= 1)
        .unwrap_or(3);
    lines.push(Line::from(vec![
        usage_content_indent(),
        Span::styled(left, Style::default().fg(WHITE)),
        Span::raw(" ".repeat(left_gap)),
        Span::styled(reset.to_owned(), DIM),
        Span::raw(" ".repeat(right_gap)),
        Span::styled(local_timestamp.to_owned(), DIM),
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

#[expect(
    clippy::too_many_arguments,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) fn usage_header_lines(
    value: &str,
    updated: Option<&str>,
    account: Option<&str>,
    username: Option<&str>,
    plan: Option<&str>,
    auth: Option<&str>,
    width: usize,
    lines: &mut Vec<Line<'static>>,
) {
    // The email is the account identity; omit it (no "account unavailable")
    // when the provider exposes none — the auth source goes on its own line.
    let account = account.map(str::trim).filter(|value| !value.is_empty());
    lines.push(usage_header_two_column(
        value,
        Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        account.unwrap_or(""),
        Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        width,
    ));

    let updated = updated.map(str::trim).filter(|value| !value.is_empty());
    let username = username.map(str::trim).filter(|value| !value.is_empty());
    let plan = plan.map(str::trim).filter(|value| !value.is_empty());
    let right_parts = [username, plan].into_iter().flatten().collect::<Vec<_>>();
    let right = (!right_parts.is_empty()).then(|| right_parts.join(" \u{b7} "));
    if updated.is_some() || right.is_some() {
        lines.push(usage_header_two_column(
            updated.unwrap_or(""),
            DIM,
            right.as_deref().unwrap_or(""),
            DIM,
            width,
        ));
    }

    // Line 3: the credential source — never the secret.
    if let Some(auth) = auth.map(str::trim).filter(|value| !value.is_empty()) {
        lines.push(Line::from(vec![
            usage_content_indent(),
            Span::styled("Auth: ", DIM),
            Span::styled(auth.to_owned(), DIM),
        ]));
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
    let left_cols = termrock::display_cols(left);
    let right_cols = termrock::display_cols(right);
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
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ),
    ]));

    let Some(first) = value.split(" · ").find(|part| !part.trim().is_empty()) else {
        return;
    };

    let (meter, remaining_label) = usage_meter_parts(first);
    if remaining_label.is_none() {
        lines.push(usage_header_two_column(
            first,
            Style::default().fg(WHITE),
            "",
            DIM,
            width,
        ));
        return;
    }

    let meter = usage_full_width_meter(meter, width);
    lines.push(Line::from(vec![
        usage_content_indent(),
        Span::styled(meter, Style::default().fg(accent.unwrap_or(PHOSPHOR_GREEN))),
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
            Style::default().fg(WHITE),
            &right,
            DIM,
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
        Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        right,
        DIM,
        width,
    ));
    for detail in parts.iter().skip(1) {
        lines.push(usage_header_two_column(
            detail,
            Style::default().fg(WHITE),
            "",
            DIM,
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
    Line::from(vec![Span::styled("─".repeat(target), DIM)])
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
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ", DIM),
        Span::styled(detail, Style::default().fg(WHITE)),
    ]));
}

pub(crate) fn compact_bucket_detail_for_width(label: &str, detail: &str, width: usize) -> String {
    if width == 0 {
        return detail.to_owned();
    }
    let prefix_cols = 2 + termrock::display_cols(label) + 2;
    let Some(detail_cols) = width.checked_sub(prefix_cols) else {
        return String::new();
    };
    truncate_display_with_ellipsis(detail, detail_cols)
}

pub(crate) fn truncate_display_with_ellipsis(value: &str, width: usize) -> String {
    if termrock::display_cols(value) <= width {
        return value.to_owned();
    }
    if width == 0 {
        return String::new();
    }
    if width == 1 {
        return "…".to_owned();
    }
    format!("{}…", termrock::take_display_cols(value, width - 1))
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
