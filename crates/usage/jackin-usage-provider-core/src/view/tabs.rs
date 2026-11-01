// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::super::{
    CachedUsage, FocusedUsageView, HashMap, UsageProviderTab, UsageSurface, account_key_hash,
};
use super::{compact_account_identity, usage_tab_source_label, usage_tab_status_label};
use jackin_protocol::usage_broker::UsageAccountCapability;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum UsageTabIdentity {
    Bound(UsageAccountCapability),
    Unbound(String),
}

pub fn usage_account_tab_id(provider_label: &str, account_label: &str) -> String {
    account_key_hash(provider_label, account_label)
}

pub(crate) fn usage_tab_id_for_view(view: &FocusedUsageView) -> String {
    view.account.account_capability.as_ref().map_or_else(
        || usage_account_tab_id(&view.account.provider_label, &view.account.account_label),
        UsageAccountCapability::opaque_identity_key,
    )
}

fn usage_tab_identity(view: &FocusedUsageView) -> UsageTabIdentity {
    view.account.account_capability.clone().map_or_else(
        || {
            UsageTabIdentity::Unbound(usage_account_tab_id(
                &view.account.provider_label,
                &view.account.account_label,
            ))
        },
        UsageTabIdentity::Bound,
    )
}

pub fn provider_tabs(views: &[&FocusedUsageView]) -> Vec<UsageProviderTab> {
    let mut keyed: Vec<(UsageTabIdentity, String, &FocusedUsageView)> = views
        .iter()
        .map(|view| (usage_tab_identity(view), usage_tab_id_for_view(view), *view))
        .collect();
    keyed.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then(right.2.fetched_at_epoch.cmp(&left.2.fetched_at_epoch))
    });
    keyed.dedup_by(|left, right| left.0 == right.0);
    let mut tabs: Vec<UsageProviderTab> = keyed
        .into_iter()
        .map(|(_, id, view)| account_tab(id, view))
        .collect();
    tabs.sort_by(|left, right| {
        left.label
            .cmp(&right.label)
            .then(left.account_label.cmp(&right.account_label))
            .then(left.id.cmp(&right.id))
    });
    tabs
}

pub fn enrich_provider_tabs(view: &mut FocusedUsageView, snapshots: &HashMap<String, CachedUsage>) {
    let views: Vec<&FocusedUsageView> = snapshots.values().map(|cached| &cached.view).collect();
    view.tabs = provider_tabs(&views);
}

pub(crate) fn account_tab(id: String, view: &FocusedUsageView) -> UsageProviderTab {
    UsageProviderTab {
        id,
        label: account_tab_label(view),
        status_label: usage_tab_status_label(view),
        account_label: compact_account_identity(&view.account.account_label).to_owned(),
        plan_label: view.account.plan_label.clone(),
        source_label: Some(usage_tab_source_label(view)),
        active: false,
        account_capability: view.account.account_capability.clone(),
    }
}

pub(crate) fn account_tab_label(view: &FocusedUsageView) -> String {
    account_tab_label_for_parts(
        &view.account.provider_label,
        &view.account.account_label,
        view.focused_provider.as_deref(),
    )
}

pub fn account_tab_label_for_parts(
    provider_label: &str,
    account_label: &str,
    focused_provider: Option<&str>,
) -> String {
    let provider = provider_label.trim();
    let provider = if !provider.is_empty()
        && !provider.eq_ignore_ascii_case(UsageSurface::Unsupported.label())
    {
        provider.to_owned()
    } else {
        focused_provider
            .map(str::trim)
            .filter(|label| !label.is_empty())
            .unwrap_or(UsageSurface::Unsupported.label())
            .to_owned()
    };
    let account = compact_account_identity(account_label);
    if account == "account unavailable" {
        provider
    } else {
        format!("{provider} · {account}")
    }
}
