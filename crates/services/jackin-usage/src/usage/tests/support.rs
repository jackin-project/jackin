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

pub(super) fn codex_minimal_limits_value() -> serde_json::Value {
    serde_json::json!({
        "rateLimits": {
            "primary": {
                "usedPercent": 25.0,
                "windowDurationMins": 300,
                "resetsAt": 1_781_189_520_i64
            }
        }
    })
}

pub(super) fn test_jwt(payload: serde_json::Value) -> String {
    let header = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode("{}");
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload.to_string());
    format!("{header}.{payload}.signature")
}

pub(super) const AMP_DAILY_FIXTURE: &str = "Signed in as user@example.com (example)\n\
     Amp Free: 61% remaining today (resets daily)\n\
     Individual credits: $9.86 remaining\n\
     Workspace example: $5.33 remaining";

pub(super) const AMP_TWO_WORKSPACE_FIXTURE: &str = "Amp Free: 61% remaining today (resets daily)\n\
     Individual credits: $9.86 remaining\n\
     Workspace alpha: $5.33 remaining\n\
     Workspace beta: $2.25 remaining";

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

pub(super) const AMP_TIER_FIXTURE: &str = "Signed in as user@example.com (example)\n\
     Amp Free: 61% remaining today (resets daily)\n\
     Amp Pro Tier: agent usage $80.00 of $100.00 remaining, orb usage 7.5h of 10h a1.small orb hours remaining, period 2026-09-01 to 2026-10-01, resets upon renewal in 12 days\n\
     Individual credits: $9.86 remaining";
