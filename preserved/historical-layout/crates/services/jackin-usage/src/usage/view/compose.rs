// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Usage view composition.

use super::super::{
    FocusedAccountHeader, FocusedUsageView, QuotaBucketView, UsageConfidence, UsageSnapshotStatus,
    UsageSource, UsageSurface,
};
use super::{provider_tabs, status_bar_label};

pub(crate) struct UsageViewInput<'a> {
    pub(crate) agent: &'a str,
    pub(crate) provider: Option<&'a str>,
    pub(crate) surface: UsageSurface,
    pub(crate) account_label: String,
    pub(crate) username: Option<String>,
    pub(crate) plan_label: Option<String>,
    pub(crate) credential_origin: Option<String>,
    pub(crate) buckets: Vec<QuotaBucketView>,
    pub(crate) status: UsageSnapshotStatus,
    pub(crate) source: UsageSource,
    pub(crate) confidence: UsageConfidence,
    pub(crate) now: i64,
    pub(crate) last_error: Option<String>,
}

pub(crate) fn usage_view(input: UsageViewInput<'_>) -> FocusedUsageView {
    let headline = status_bar_label(
        input.surface,
        &input.account_label,
        input.status,
        &input.buckets,
    );
    let mut view = FocusedUsageView {
        focused_agent: Some(input.agent.to_owned()),
        focused_provider: input
            .provider
            .map(str::to_owned)
            .or_else(|| Some(input.surface.label().to_owned())),
        account: FocusedAccountHeader {
            provider_label: input.surface.account_label().to_owned(),
            account_label: input.account_label,
            username: input.username,
            plan_label: input.plan_label,
            credential_origin: input.credential_origin,
        },
        buckets: input.buckets,
        status: input.status,
        source: input.source,
        confidence: input.confidence,
        fetched_at_epoch: input.now,
        updated_label: match input.status {
            UsageSnapshotStatus::Fresh => "Updated now",
            UsageSnapshotStatus::Stale => "Stale",
            UsageSnapshotStatus::NeedsLogin => "Needs login",
            UsageSnapshotStatus::NeedsSecret => "Needs secret",
            UsageSnapshotStatus::Unsupported => "Unsupported",
            UsageSnapshotStatus::Unavailable => "Unavailable",
            UsageSnapshotStatus::Error => "Error",
        }
        .to_owned(),
        status_bar_label: headline,
        tabs: Vec::new(),
        last_error: input.last_error,
    };
    // A freshly built view tabs its own account; the cache enriches the strip
    // to every admitted account before display.
    view.tabs = provider_tabs(&[&view]);
    view
}
