// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Provider tabs.

use super::super::{
    CachedUsage, FocusedUsageView, HashMap, UsageProviderTab, UsageSurface, account_key_hash,
};
use super::{compact_account_identity, usage_tab_source_label, usage_tab_status_label};

/// Stable canonical account id keying usage tabs: the same
/// [`account_key_hash`] the durable snapshot store uses as its stable
/// multi-account id, so tabs, overview rows, and stored snapshots correlate.
/// Same-provider accounts share a display label but never an id.
pub(crate) fn usage_account_tab_id(provider_label: &str, account_label: &str) -> String {
    account_key_hash(provider_label, account_label)
}

/// One tab per distinct admitted account, keyed by
/// [`usage_account_tab_id`]. Duplicate snapshots for one account collapse to
/// the newest fetch; the strip sorts by display label, then account, then id,
/// so any provider (Cursor, `OpenRouter`, Copilot, Antigravity, Gemini,
/// `OpenCode`, omp, Hermes, …) tabs without a hardcoded surface list. Empty
/// input stays empty.
pub(crate) fn provider_tabs(views: &[&FocusedUsageView]) -> Vec<UsageProviderTab> {
    let mut keyed: Vec<(String, &FocusedUsageView)> = views
        .iter()
        .map(|view| {
            (
                usage_account_tab_id(&view.account.provider_label, &view.account.account_label),
                *view,
            )
        })
        .collect();
    // Newest fetch first per account, so `dedup_by` (which keeps the first of
    // each run) keeps the latest snapshot; full ties are interchangeable.
    keyed.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then(right.1.fetched_at_epoch.cmp(&left.1.fetched_at_epoch))
    });
    keyed.dedup_by(|left, right| left.0 == right.0);
    let mut tabs: Vec<UsageProviderTab> = keyed
        .into_iter()
        .map(|(id, view)| account_tab(id, view))
        .collect();
    tabs.sort_by(|left, right| {
        left.label
            .cmp(&right.label)
            .then(left.account_label.cmp(&right.account_label))
            .then(left.id.cmp(&right.id))
    });
    tabs
}

/// Tab enrichment is pure reconstruction: one tab per admitted account
/// snapshot, no hardcoded surface list, no fuzzy-label latest-wins. Active
/// marking is [`mark_active_tab`]'s exact-id job, applied after.
pub(crate) fn enrich_provider_tabs(
    view: &mut FocusedUsageView,
    snapshots: &HashMap<String, CachedUsage>,
) {
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
    }
}

/// Display label for an account tab: the snapshot's provider label (falling
/// back to the focused provider when the snapshot carries only the generic
/// `Usage` placeholder), suffixed with the compact account identity so
/// same-provider accounts render individually visible tabs. Matching still
/// keys on the id, never this label.
pub(crate) fn account_tab_label(view: &FocusedUsageView) -> String {
    account_tab_label_for_parts(
        &view.account.provider_label,
        &view.account.account_label,
        view.focused_provider.as_deref(),
    )
}

/// Shared tab-label rule (live cache and snapshot store): provider head with
/// a ` · {account}` suffix so same-provider accounts stay individually
/// visible.
pub(crate) fn account_tab_label_for_parts(
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
