// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

pub(super) fn account_snapshot_view(
    provider_label: &str,
    account_label: &str,
    plan_label: Option<&str>,
    fetched_at_epoch: i64,
) -> FocusedUsageView {
    let mut view = FocusedUsageView::unavailable("none", fetched_at_epoch);
    view.account.provider_label = provider_label.to_owned();
    view.account.account_label = account_label.to_owned();
    view.account.plan_label = plan_label.map(str::to_owned);
    view.status = UsageSnapshotStatus::Fresh;
    view
}
pub(super) fn codex_cached_usage_view() -> FocusedUsageView {
    usage_view(UsageViewInput {
        agent: "codex",
        provider: Some("OpenAI"),
        surface: UsageSurface::Codex,
        account_label: "codex@example.com".to_owned(),
        username: None,
        plan_label: Some("Pro 20x".to_owned()),
        credential_origin: None,
        buckets: vec![QuotaBucketView {
            used_money: None,
            limit_money: None,
            severity: UsageSeverity::default(),
            label: "Session".to_owned(),
            used_label: Some("63% used".to_owned()),
            limit_label: Some("100%".to_owned()),
            remaining_percent: Some(37),
            reset_label: Some("Resets in 2h".to_owned()),
            resets_at: None,
            status_slot: None,
            pace_label: None,
            status: UsageSnapshotStatus::Fresh,
        }],
        status: UsageSnapshotStatus::Fresh,
        source: UsageSource::ProviderApi,
        confidence: UsageConfidence::Authoritative,
        now: 123,
        last_error: None,
    })
}
pub(super) fn presentation_bucket(
    label: &str,
    remaining: Option<u8>,
    slot: Option<StatusSlot>,
    status: UsageSnapshotStatus,
) -> QuotaBucketView {
    QuotaBucketView {
        label: label.to_owned(),
        used_label: None,
        limit_label: None,
        remaining_percent: remaining,
        reset_label: None,
        resets_at: None,
        status_slot: slot,
        pace_label: None,
        status,
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::Normal,
    }
}

pub(super) fn detail_view(
    buckets: Vec<QuotaBucketView>,
    last_error: Option<&str>,
    status: UsageSnapshotStatus,
) -> FocusedUsageView {
    FocusedUsageView {
        focused_agent: Some("codex".to_owned()),
        focused_provider: Some("OpenAI".to_owned()),
        account: FocusedAccountHeader {
            provider_label: "OpenAI".to_owned(),
            account_label: "operator@example.com".to_owned(),
            username: Some("operator".to_owned()),
            plan_label: Some("Pro 20x".to_owned()),
            credential_origin: Some("OAuth · ~/.codex/auth.json".to_owned()),
        },
        buckets,
        status,
        updated_label: "Updated 2m ago".to_owned(),
        last_error: last_error.map(str::to_owned),
        ..FocusedUsageView::unavailable("seed", 1_781_185_560)
    }
}
