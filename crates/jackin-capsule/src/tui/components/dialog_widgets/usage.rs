// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Canonical usage rendering. Geometry comes from typed values, never labels.

use crate::tui::components::dialog::{UsageDialogState, UsageDialogTab, UsageDialogTarget};
use jackin_protocol::usage_broker::{
    UsageAccountV2, UsageCalendarPeriodV2, UsageFreshnessPhaseV2, UsageFreshnessV2,
    UsageIssueRecoverabilityV2, UsageIssueV2, UsageLifecycleV2, UsageLimitWindowV2,
    UsageMetricGroupV2, UsageMetricPeriodV2, UsageMetricValueV2, UsageProjectionRefreshStateV2,
    UsageProjectionV2, UsageProviderV2, UsageQuotaStateV2, UsageUnresolvedGrantV2,
};
use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
};

pub(crate) fn usage_dialog_inner_area(area: Rect) -> Rect {
    Rect::new(
        area.x.saturating_add(1),
        area.y.saturating_add(1),
        area.width.saturating_sub(2),
        area.height.saturating_sub(2),
    )
}

pub(crate) fn usage_tab_strip_labels(
    projection: Option<&UsageProjectionV2>,
    destination: Option<&UsageDialogTarget>,
    selected: UsageDialogTab,
) -> Vec<(String, bool)> {
    let mut tabs = vec![("Overview".to_owned(), selected == UsageDialogTab::Overview)];
    if let Some(projection) = projection {
        for provider in &projection.providers {
            for account in &provider.accounts {
                tabs.push((
                    if provider.accounts.len() == 1 || account.display_label.trim().is_empty() {
                        provider.display_name.clone()
                    } else {
                        format!("{} · {}", provider.display_name, account.display_label)
                    },
                    selected == UsageDialogTab::Provider
                        && destination.is_some_and(|target| {
                            matches!(target, UsageDialogTarget::Account(target)
                                if target.provider_id == provider.provider_id
                                    && target.canonical_account_id == account.canonical_account_id)
                        }),
                ));
            }
        }
        for grant in &projection.unresolved_grants {
            tabs.push((
                format!("{} · {}", grant.surface_id, grant.configured_account_id),
                selected == UsageDialogTab::Provider
                    && destination.is_some_and(|target| {
                        matches!(target, UsageDialogTarget::UnresolvedGrant { configured_account_id, surface_id }
                            if configured_account_id == &grant.configured_account_id && surface_id == &grant.surface_id)
                    }),
            ));
        }
    }
    tabs
}

pub(crate) fn usage_tab_strip_width(tabs: &[(String, bool)]) -> usize {
    let gap = usize::from(termrock::widgets::TAB_GAP);
    tabs.iter()
        .map(|(label, _)| termrock::text::display_cols(label) + 2 + gap)
        .sum()
}

pub(crate) fn usage_tab_strip_area(inner: Rect, tabs: &[(String, bool)]) -> Rect {
    let width = usage_tab_strip_width(tabs)
        .saturating_sub(usize::from(termrock::widgets::TAB_GAP))
        .min(usize::from(inner.width));
    Rect::new(
        inner.x.saturating_add(
            u16::try_from((usize::from(inner.width).saturating_sub(width)) / 2).unwrap_or(0),
        ),
        inner.y,
        u16::try_from(width).unwrap_or(inner.width),
        inner.height.min(2),
    )
}

/// Keep the selected canonical destination visible, preserving original ids.
fn visible_tabs(tabs: &[(String, bool)], width: u16) -> Vec<termrock::widgets::Tab<'_, usize>> {
    let active = tabs.iter().position(|(_, active)| *active).unwrap_or(0);
    let gap = usize::from(termrock::widgets::TAB_GAP);
    let budget = usize::from(width);
    let prefix_width = tabs
        .iter()
        .take(active + 1)
        .map(|(label, _)| termrock::text::display_cols(label) + 2 + gap)
        .sum::<usize>()
        .saturating_sub(gap);
    let overview_width = tabs.first().map_or(0, |(label, _)| {
        termrock::text::display_cols(label) + 2 + gap
    });
    let mut ids = Vec::new();
    if prefix_width <= budget || active == 0 {
        ids.extend(0..tabs.len());
    } else {
        if overview_width + termrock::text::display_cols(&tabs[active].0) + 2 <= budget {
            ids.push(0);
        }
        ids.extend(active..tabs.len());
    }
    let mut used = 0;
    let mut visible = Vec::new();
    for id in ids {
        let (label, selected) = &tabs[id];
        let needed = termrock::text::display_cols(label) + 2;
        let spacing = if visible.is_empty() { 0 } else { gap };
        if used + spacing + needed > budget && !visible.is_empty() {
            break;
        }
        visible.push(termrock::widgets::Tab::new(id, label.as_str()).active(*selected));
        used += spacing + needed;
    }
    visible
}

fn tabs_theme() -> termrock::style::DesignSystem {
    let theme = termrock::style::DesignSystem::default();
    let active = theme
        .style(termrock::style::Role::TabActiveHovered)
        .add_modifier(ratatui::style::Modifier::UNDERLINED);
    let inactive = theme
        .style(termrock::style::Role::TabInactiveHovered)
        .add_modifier(ratatui::style::Modifier::UNDERLINED);
    theme
        .with_role(termrock::style::Role::TabActiveHovered, active)
        .with_role(termrock::style::Role::TabInactiveHovered, inactive)
}

fn tabs_state(
    tabs: &[(String, bool)],
    focused: bool,
    hovered: Option<usize>,
) -> termrock::widgets::TabsState<usize> {
    let mut state = termrock::widgets::TabsState::new();
    state.set_selected(tabs.iter().position(|(_, active)| *active));
    state.focused = focused;
    state.hovered = hovered;
    state
}

pub(crate) fn render_usage_tabs(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    tabs: &[(String, bool)],
    focused: bool,
    hovered: Option<usize>,
) {
    let visible = visible_tabs(tabs, area.width);
    let theme = tabs_theme();
    frame.render_stateful_widget(
        &termrock::widgets::Tabs::new(&visible, &theme).gap(termrock::widgets::TAB_GAP),
        area,
        &mut tabs_state(tabs, focused, hovered),
    );
}

/// Hit regions come from the same responsive widget paint as the visible strip.
pub(crate) fn usage_tab_strip_index_at(
    tabs: &[(String, bool)],
    area: Rect,
    col: u16,
) -> Option<usize> {
    if area.height == 0 || col < area.x || col >= area.right() {
        return None;
    }
    let visible = visible_tabs(tabs, area.width);
    let theme = tabs_theme();
    let mut state = tabs_state(tabs, true, None);
    let mut buffer = ratatui::buffer::Buffer::empty(area);
    termrock::widgets::Tabs::new(&visible, &theme)
        .gap(termrock::widgets::TAB_GAP)
        .paint(area, &mut buffer, &mut state);
    state
        .regions
        .into_iter()
        .find(|region| col >= region.area.x && col < region.area.right())
        .map(|region| region.id)
}

pub(crate) fn usage_panel_title(state: &UsageDialogState, width: u16) -> String {
    if width < 68
        && let Some((provider, _)) = selected_account(state)
    {
        return format!("Usage: {}", provider.display_name);
    }
    "Usage".to_owned()
}

pub(crate) fn usage_body_rect(area: Rect) -> Rect {
    let inner = usage_dialog_inner_area(area);
    let tab_height = inner.height.min(2);
    Rect {
        y: inner.y.saturating_add(tab_height),
        height: inner.height.saturating_sub(tab_height),
        ..inner
    }
}

pub(crate) fn usage_info_required_height(state: &UsageDialogState) -> u16 {
    u16::try_from(usage_info_lines(state).len())
        .unwrap_or(u16::MAX)
        .saturating_add(4)
        .max(7)
}

pub(crate) fn usage_scroll_inputs(area: Rect, state: &UsageDialogState) -> (usize, usize, Rect) {
    let body = usage_body_rect(area);
    let lines = usage_info_lines_for_width(state, body.width);
    (
        lines.iter().map(usage_line_width).max().unwrap_or(0),
        lines.len(),
        Rect {
            height: body.height.saturating_add(2),
            ..area
        },
    )
}

pub(crate) fn usage_line_width(line: &Line<'_>) -> usize {
    line.spans
        .iter()
        .map(|span| termrock::text::display_cols(span.content.as_ref()))
        .sum()
}

pub(crate) fn usage_content_width(width: usize) -> usize {
    if width == 0 {
        32
    } else {
        width.saturating_sub(4)
    }
}

fn selected_account(state: &UsageDialogState) -> Option<(&UsageProviderV2, &UsageAccountV2)> {
    let UsageDialogTarget::Account(account) = state.destination.as_ref()? else {
        return None;
    };
    account.account(state.projection.as_deref()?)
}

pub(crate) fn usage_info_lines(state: &UsageDialogState) -> Vec<Line<'static>> {
    usage_info_lines_for_width(state, 0)
}

pub(crate) fn usage_info_lines_for_width(
    state: &UsageDialogState,
    width: u16,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if state.refresh_unavailable {
        text_line(
            &mut lines,
            "Refresh",
            "This usage view is read-only in this Capsule.",
        );
    }
    if let Some(error) = &state.transport_error {
        text_line(&mut lines, "Refresh error", error);
    }
    if let Some(notice) = &state.notice {
        text_line(&mut lines, "Notice", notice);
    }
    let Some(projection) = state.projection.as_deref() else {
        text_line(
            &mut lines,
            "Usage",
            "Canonical usage publication unavailable",
        );
        return lines;
    };
    if projection.refresh_state == UsageProjectionRefreshStateV2::Refreshing {
        text_line(&mut lines, "Refresh", "refreshing");
    }
    text_line(
        &mut lines,
        "Published",
        &epoch_label(projection.generated_at_epoch),
    );
    append_issues(&mut lines, &projection.issues);
    if let Some((provider, account)) = selected_account(state) {
        append_provider(&mut lines, provider);
        append_account(&mut lines, account, width);
    } else if let Some(grant) = state
        .destination
        .as_ref()
        .and_then(|target| target.unresolved_grant(projection))
    {
        append_unresolved_grant(&mut lines, grant);
    } else {
        if projection.providers.is_empty()
            && projection.unresolved.is_empty()
            && projection.unresolved_grants.is_empty()
        {
            text_line(&mut lines, "Accounts", "No authorized usage accounts");
        }
        for provider in &projection.providers {
            append_provider(&mut lines, provider);
            if provider.accounts.is_empty() {
                text_line(&mut lines, "Accounts", "No resolved accounts");
            }
            for account in &provider.accounts {
                append_account(&mut lines, account, width);
            }
        }
        for grant in &projection.unresolved_grants {
            append_unresolved_grant(&mut lines, grant);
        }
        for unresolved in &projection.unresolved {
            heading(
                &mut lines,
                &format!(
                    "{} · Unresolved ({})",
                    unresolved.provider_id, unresolved.capability_id
                ),
            );
            text_line(&mut lines, "Lifecycle", lifecycle_label(unresolved.state));
            append_issues(&mut lines, &unresolved.issues);
        }
    }
    lines
}

fn append_unresolved_grant(lines: &mut Vec<Line<'static>>, grant: &UsageUnresolvedGrantV2) {
    heading(
        lines,
        &format!("{} · Unresolved grant", grant.configured_account_id),
    );
    text_line(lines, "Configured account", &grant.configured_account_id);
    text_line(lines, "Surface", &grant.surface_id);
    append_issues(lines, &grant.issues);
}

fn heading(lines: &mut Vec<Line<'static>>, label: &str) {
    if label.trim().is_empty() {
        return;
    }
    if !lines.is_empty() {
        lines.push(Line::from(""));
    }
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            label.to_owned(),
            termrock::style::DesignSystem::default().style(termrock::style::Role::TextStrong),
        ),
    ]));
}

fn text_line(lines: &mut Vec<Line<'static>>, label: &str, value: &str) {
    if value.trim().is_empty() {
        return;
    }
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            format!("{label}: "),
            termrock::style::DesignSystem::default().style(termrock::style::Role::TextMuted),
        ),
        Span::raw(value.to_owned()),
    ]));
}

fn append_provider(lines: &mut Vec<Line<'static>>, provider: &UsageProviderV2) {
    heading(lines, &provider.display_name);
    append_freshness(lines, "Provider freshness", &provider.freshness);
    append_issues(lines, &provider.issues);
}

fn append_account(lines: &mut Vec<Line<'static>>, account: &UsageAccountV2, width: u16) {
    heading(lines, &account.display_label);
    if let Some(plan) = &account.plan_label {
        text_line(lines, "Plan", plan);
    }
    if let Some(username) = &account.username {
        text_line(lines, "Username", username);
    }
    if let Some(auth_origin) = &account.auth_origin {
        text_line(lines, "Auth", auth_origin);
    }
    text_line(lines, "Lifecycle", lifecycle_label(account.lifecycle));
    if let Some(status) = &account.status_label {
        text_line(lines, "Status", status);
    }
    append_freshness(lines, "Freshness", &account.freshness);
    if let Some(epoch) = account.credential_expires_at_epoch {
        text_line(lines, "Credential expires", &epoch_label(epoch));
    }
    for window in &account.windows {
        append_window(lines, window, width);
    }
    for group in &account.metric_groups {
        append_group(lines, group, width);
    }
    append_issues(lines, &account.issues);
}

fn append_freshness(lines: &mut Vec<Line<'static>>, label: &str, freshness: &UsageFreshnessV2) {
    text_line(
        lines,
        label,
        &freshness_label(freshness.phase, freshness.is_stale),
    );
    if let Some(epoch) = freshness.last_good_at_epoch {
        text_line(lines, "Last good", &epoch_label(epoch));
    }
    if let Some(epoch) = freshness.retry_at_epoch {
        text_line(lines, "Retry at", &epoch_label(epoch));
    }
}

fn append_window(lines: &mut Vec<Line<'static>>, window: &UsageLimitWindowV2, width: u16) {
    heading(lines, &window.label);
    // The host's value label is authoritative even when exact counts are present.
    text_line(lines, "Value", &window.value_label);
    text_line(lines, "Quota", quota_label(window.quota_state));
    if let Some(count) = &window.count_quota {
        text_line(
            lines,
            "Count",
            &jackin_usage::usage::usage_count_quota_summary(count),
        );
    }
    let geometry = remaining_geometry(
        window.count_quota.as_ref(),
        window.remaining_percent.map(|percent| percent.get()),
        window.used_percent.map(|percent| percent.get()),
    );
    append_meter(lines, geometry, window.quota_state, width);
    append_raw_percent(
        lines,
        window.remaining_percent.map(|percent| percent.get()),
        window.remaining_raw_percent,
        "remaining",
    );
    append_raw_percent(
        lines,
        window.used_percent.map(|percent| percent.get()),
        window.used_raw_percent,
        "used",
    );
    text_line(lines, "Reset", &window.reset_label);
    if let Some(epoch) = window.reset_at_epoch {
        text_line(lines, "Reset at", &epoch_label(epoch));
    }
    if let Some(pace) = &window.pace_label {
        text_line(lines, "Pace", pace);
    }
}

/// Every quota meter represents remaining allowance. Exact counts take precedence.
fn remaining_geometry(
    count: Option<&jackin_protocol::control::CountQuota>,
    remaining: Option<u8>,
    used: Option<u8>,
) -> Option<u8> {
    match count {
        Some(count) => count.remaining_percent(),
        None => remaining.or_else(|| used.map(|used| 100u8.saturating_sub(used))),
    }
}

fn append_meter(
    lines: &mut Vec<Line<'static>>,
    geometry: Option<u8>,
    quota: UsageQuotaStateV2,
    width: u16,
) {
    if !matches!(
        quota,
        UsageQuotaStateV2::Available | UsageQuotaStateV2::Warning | UsageQuotaStateV2::Exhausted
    ) {
        return;
    }
    let Some(percent) = geometry else {
        return;
    };
    let meter_width = usage_content_width(usize::from(width));
    let filled = meter_width.saturating_mul(usize::from(percent.min(100))) / 100;
    let color = match quota {
        UsageQuotaStateV2::Warning => jackin_tui::tokens::DEBUG_AMBER,
        UsageQuotaStateV2::Exhausted => termrock::style::DesignSystem::default()
            .style(termrock::style::Role::Danger)
            .fg
            .unwrap_or_default(),
        _ => termrock::style::DesignSystem::default()
            .style(termrock::style::Role::Accent)
            .fg
            .unwrap_or_default(),
    };
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            format!(
                "{}{}",
                "█".repeat(filled),
                "░".repeat(meter_width.saturating_sub(filled))
            ),
            Style::default().fg(color),
        ),
    ]));
}

fn append_raw_percent(
    lines: &mut Vec<Line<'static>>,
    geometry: Option<u8>,
    raw: Option<i32>,
    side: &str,
) {
    if let Some(raw) = raw
        && geometry.is_none_or(|percent| i32::from(percent) != raw)
    {
        text_line(lines, "Raw", &format!("{side} {raw}%"));
    }
}

fn append_group(lines: &mut Vec<Line<'static>>, group: &UsageMetricGroupV2, width: u16) {
    heading(lines, &group.label);
    for (label, value) in [
        ("Service", &group.scope.service),
        ("Model", &group.scope.model),
        ("Pool", &group.scope.pool),
        ("Key", &group.scope.key_id),
    ] {
        if let Some(value) = value {
            text_line(lines, label, value);
        }
    }
    text_line(
        lines,
        "Freshness",
        &freshness_label(group.phase, group.is_stale),
    );
    text_line(lines, "Quota", quota_label(group.quota_state));
    if let Some(summary) = metric_value_summary(&group.value) {
        text_line(lines, "Value", &summary);
    }
    if let UsageMetricValueV2::Window {
        count_quota,
        remaining_percent,
        remaining_raw_percent,
        used_percent,
        used_raw_percent,
        ..
    } = &group.value
    {
        let geometry = remaining_geometry(
            count_quota.as_ref(),
            remaining_percent.map(|percent| percent.get()),
            used_percent.map(|percent| percent.get()),
        );
        append_meter(lines, geometry, group.quota_state, width);
        append_raw_percent(
            lines,
            remaining_percent.map(|percent| percent.get()),
            *remaining_raw_percent,
            "remaining",
        );
        append_raw_percent(
            lines,
            used_percent.map(|percent| percent.get()),
            *used_raw_percent,
            "used",
        );
    }
    if let Some(epoch) = group.observed_at_epoch {
        text_line(lines, "Observed", &epoch_label(epoch));
    }
    text_line(lines, "Fetched", &epoch_label(group.fetched_at_epoch));
    if let Some(epoch) = group.last_success_at_epoch {
        text_line(lines, "Last good", &epoch_label(epoch));
    }
    if let Some(epoch) = group.reset_at_epoch {
        text_line(lines, "Reset at", &epoch_label(epoch));
    }
    if let Some(epoch) = group.renews_at_epoch {
        text_line(lines, "Renews at", &epoch_label(epoch));
    }
    if let UsageMetricValueV2::Balance {
        expires_at_epoch: Some(epoch),
        ..
    } = &group.value
    {
        text_line(lines, "Balance expires", &epoch_label(*epoch));
    }
    append_issues(lines, &group.issues);
}

fn metric_value_summary(value: &UsageMetricValueV2) -> Option<String> {
    let mut parts = Vec::new();
    match value {
        UsageMetricValueV2::Window {
            count_quota,
            remaining_percent,
            used_percent,
            period,
            unit,
            ..
        } => {
            if let Some(count) = count_quota {
                parts.push(jackin_usage::usage::usage_count_quota_summary(count));
            } else if let Some(percent) = remaining_percent {
                parts.push(format!("{}% left", percent.get()));
            } else if let Some(percent) = used_percent {
                parts.push(format!("{}% used", percent.get()));
            }
            if count_quota.is_none()
                && let Some(unit) = unit
            {
                parts.push(unit.clone());
            }
            match period {
                UsageMetricPeriodV2::Rolling { window_secs } => {
                    parts.push(format!("rolling {window_secs}s"))
                }
                UsageMetricPeriodV2::Calendar { granularity } => parts.push(
                    match granularity {
                        UsageCalendarPeriodV2::Daily => "daily",
                        UsageCalendarPeriodV2::Weekly => "weekly",
                        UsageCalendarPeriodV2::Monthly => "monthly",
                    }
                    .to_owned(),
                ),
                UsageMetricPeriodV2::ProviderDefined => {
                    parts.push("provider-defined period".to_owned())
                }
                UsageMetricPeriodV2::Unknown => {}
            }
        }
        UsageMetricValueV2::Balance { amount, .. } => parts.push(amount.to_string()),
        UsageMetricValueV2::SpendCap {
            cap,
            spent,
            remaining,
        } => {
            parts.push(
                cap.as_ref()
                    .map_or_else(|| "uncapped".to_owned(), |cap| format!("cap {cap}")),
            );
            if let Some(spent) = spent {
                parts.push(format!("spent {spent}"));
            }
            if let Some(remaining) = remaining {
                parts.push(format!("remaining {remaining}"));
            }
        }
        UsageMetricValueV2::TokenTotals {
            input,
            output,
            cached,
            reasoning,
            interval_label,
        } => {
            for (count, label) in [
                (input, "input"),
                (output, "output"),
                (cached, "cached"),
                (reasoning, "reasoning"),
            ] {
                if let Some(count) = count {
                    parts.push(format!("{label} {count}"));
                }
            }
            if let Some(label) = interval_label {
                parts.push(label.clone());
            }
        }
        UsageMetricValueV2::RateLimit {
            limit,
            remaining,
            window_label,
        } => {
            if let Some(limit) = limit {
                parts.push(format!("limit {limit}"));
            }
            if let Some(remaining) = remaining {
                parts.push(format!("remaining {remaining}"));
            }
            if let Some(label) = window_label {
                parts.push(label.clone());
            }
        }
        UsageMetricValueV2::Plan { plan_label, tier } => {
            if let Some(plan) = plan_label {
                parts.push(plan.clone());
            }
            if let Some(tier) = tier {
                parts.push(format!("tier {tier}"));
            }
        }
    }
    parts.retain(|part| !part.is_empty());
    (!parts.is_empty()).then(|| parts.join(" · "))
}

fn append_issues(lines: &mut Vec<Line<'static>>, issues: &[UsageIssueV2]) {
    for issue in issues {
        let recovery = match issue.recoverability {
            UsageIssueRecoverabilityV2::Retryable => "retryable",
            UsageIssueRecoverabilityV2::ActionRequired => "action required",
            UsageIssueRecoverabilityV2::Unsupported => "unsupported",
            UsageIssueRecoverabilityV2::Terminal => "terminal",
        };
        text_line(
            lines,
            "Issue",
            &format!("{} · {} · {recovery}", issue.code, issue.message),
        );
        if let Some(epoch) = issue.retry_at_epoch {
            text_line(lines, "Retry at", &epoch_label(epoch));
        }
    }
}

fn lifecycle_label(lifecycle: UsageLifecycleV2) -> &'static str {
    match lifecycle {
        UsageLifecycleV2::Available => "available",
        UsageLifecycleV2::AgentUninitialized => "agent uninitialized",
        UsageLifecycleV2::NeedsLogin => "needs login",
        UsageLifecycleV2::NeedsSecret => "needs secret",
        UsageLifecycleV2::Unsupported => "unsupported",
        UsageLifecycleV2::Unavailable => "unavailable",
        UsageLifecycleV2::Error => "error",
    }
}

fn quota_label(quota: UsageQuotaStateV2) -> &'static str {
    match quota {
        UsageQuotaStateV2::Available => "available",
        UsageQuotaStateV2::NotStarted => "not started",
        UsageQuotaStateV2::Warning => "warning",
        UsageQuotaStateV2::Exhausted => "exhausted",
        UsageQuotaStateV2::Unsupported => "unsupported",
        UsageQuotaStateV2::Unavailable => "unavailable",
        UsageQuotaStateV2::NoPermission => "no permission",
        UsageQuotaStateV2::Unknown => "unknown",
        UsageQuotaStateV2::NotApplicable => "not applicable",
        UsageQuotaStateV2::Error => "error",
    }
}

fn freshness_label(phase: UsageFreshnessPhaseV2, stale: bool) -> String {
    let label = match phase {
        UsageFreshnessPhaseV2::Current => "current",
        UsageFreshnessPhaseV2::Stale => "stale",
        UsageFreshnessPhaseV2::Refreshing => "refreshing",
        UsageFreshnessPhaseV2::Failed => "failed",
    };
    if stale {
        format!("{label} · retained last-good data")
    } else {
        label.to_owned()
    }
}

fn epoch_label(epoch: i64) -> String {
    chrono::DateTime::from_timestamp(epoch, 0).map_or_else(
        || format!("{epoch} (Unix seconds)"),
        |time| time.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
    )
}

#[cfg(test)]
mod tab_state_tests {
    use super::*;

    #[test]
    fn selected_account_tab_owns_focus_before_and_after_paint() {
        let tabs = vec![("Overview".to_owned(), false), ("Account".to_owned(), true)];
        let mut state = tabs_state(&tabs, true, None);
        assert_eq!(state.focused_tab(), Some(&1));
        let area = Rect::new(0, 0, 60, 2);
        let mut buffer = ratatui::buffer::Buffer::empty(area);
        let visible = visible_tabs(&tabs, area.width);
        termrock::widgets::Tabs::new(&visible, &tabs_theme()).paint(area, &mut buffer, &mut state);
        assert_eq!(state.focused_tab(), Some(&1));
    }
}
