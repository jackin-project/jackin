// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::super::{
    FocusedUsageView, UsageSnapshotStatus, UsageSource, UsageSurface, resolve_surface,
};
use super::{compact_account_identity, provider_tabs, status_bar_label, usage_account_tab_id};

pub(crate) fn refresh_failed_view_presentation(view: &mut FocusedUsageView) {
    if view.status != UsageSnapshotStatus::Fresh {
        for bucket in &mut view.buckets {
            bucket.status = view.status;
        }
    }
    view.updated_label = match view.status {
        UsageSnapshotStatus::Fresh => "Updated now",
        UsageSnapshotStatus::Stale => "Stale",
        UsageSnapshotStatus::NeedsLogin => "Needs login",
        UsageSnapshotStatus::NeedsSecret => "Needs secret",
        UsageSnapshotStatus::Unsupported => "Unsupported",
        UsageSnapshotStatus::Unavailable => "Unavailable",
        UsageSnapshotStatus::Error => "Error",
    }
    .to_owned();
    view.status_bar_label = status_bar_label(
        resolve_surface(
            view.focused_agent.as_deref().unwrap_or_default(),
            view.focused_provider.as_deref(),
        ),
        &view.account.account_label,
        view.status,
        &view.buckets,
    );
}

pub fn decorate_surface_view(
    view: &mut FocusedUsageView,
    agent: &str,
    focused_provider: Option<&str>,
    surface: UsageSurface,
) {
    view.focused_agent = Some(agent.to_owned());
    view.focused_provider = focused_provider
        .map(str::to_owned)
        .or_else(|| Some(surface.label().to_owned()));
    view.account.provider_label = surface.account_label().to_owned();

    view.tabs = provider_tabs(&[]);
}

pub fn cached_unavailable_view(
    agent: &str,
    focused_provider: Option<&str>,
    now: i64,
) -> FocusedUsageView {
    let surface = resolve_surface(agent, focused_provider);
    let mut view =
        FocusedUsageView::unavailable("usage unavailable: no cached provider snapshot", now);
    decorate_surface_view(&mut view, agent, focused_provider, surface);
    view
}

pub fn cached_refreshing_view(
    agent: &str,
    focused_provider: Option<&str>,
    now: i64,
) -> FocusedUsageView {
    let surface = resolve_surface(agent, focused_provider);
    let mut view = FocusedUsageView::refreshing(focused_provider, now);
    decorate_surface_view(&mut view, agent, focused_provider, surface);
    view
}

pub fn mark_active_tab(view: &mut FocusedUsageView) {
    let focused = usage_account_tab_id(&view.account.provider_label, &view.account.account_label);
    for tab in &mut view.tabs {
        tab.active = tab.id == focused;
    }
}

pub fn preserve_cached_quota_on_failed_refresh(
    view: &mut FocusedUsageView,
    cached: &FocusedUsageView,
) {
    if !matches!(
        view.status,
        UsageSnapshotStatus::Stale | UsageSnapshotStatus::NeedsLogin | UsageSnapshotStatus::Error
    ) || cached.status != UsageSnapshotStatus::Fresh
        || cached.buckets.is_empty()
    {
        return;
    }

    view.status = UsageSnapshotStatus::Stale;
    view.source = UsageSource::Cache;
    view.confidence = cached.confidence;
    view.updated_label = "Stale".to_owned();
    view.buckets = cached
        .buckets
        .iter()
        .cloned()
        .map(|mut bucket| {
            bucket.status = UsageSnapshotStatus::Stale;
            bucket
        })
        .collect();
    if view.account.plan_label.is_none() {
        view.account.plan_label = cached.account.plan_label.clone();
    }
    if compact_account_identity(&view.account.account_label) == "account unavailable" {
        view.account.account_label = cached.account.account_label.clone();
    }
    if let Some(error) = &mut view.last_error {
        error.push_str("; showing last cached quota");
    } else {
        view.last_error = Some("showing last cached quota".to_owned());
    }
    view.status_bar_label = status_bar_label(
        resolve_surface(
            view.focused_agent.as_deref().unwrap_or_default(),
            view.focused_provider.as_deref(),
        ),
        &view.account.account_label,
        view.status,
        &view.buckets,
    );
}
