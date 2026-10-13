// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn usage_view() -> FocusedUsageView {
    FocusedUsageView {
        focused_agent: Some("codex".to_owned()),
        focused_provider: Some("OpenAI".to_owned()),
        account: FocusedAccountHeader {
            provider_label: "Codex".to_owned(),
            account_label: "alexey@example.com".to_owned(),
            username: None,
            plan_label: Some("Pro 20x".to_owned()),
            credential_origin: None,
        },
        buckets: vec![
            QuotaBucketView {
                used_money: None,
                limit_money: None,
                severity: jackin_protocol::control::UsageSeverity::default(),
                label: "Session".to_owned(),
                used_label: Some("63% used".to_owned()),
                limit_label: Some("100%".to_owned()),
                remaining_percent: Some(37),
                reset_label: Some("Resets in 1h".to_owned()),
                resets_at: None,
                status_slot: None,
                pace_label: None,
                status: UsageSnapshotStatus::Fresh,
            },
            QuotaBucketView {
                used_money: None,
                limit_money: None,
                severity: jackin_protocol::control::UsageSeverity::default(),
                label: "Credits".to_owned(),
                used_label: None,
                limit_label: None,
                remaining_percent: None,
                reset_label: None,
                resets_at: None,
                status_slot: None,
                pace_label: Some("ACP billing unavailable".to_owned()),
                status: UsageSnapshotStatus::Unsupported,
            },
        ],
        status: UsageSnapshotStatus::Fresh,
        source: UsageSource::Cli,
        confidence: UsageConfidence::Authoritative,
        fetched_at_epoch: 1_781_185_560,
        updated_label: "Updated just now".to_owned(),
        status_bar_label: "Codex Session: 63% used · 37% left".to_owned(),
        tabs: Vec::new(),
        last_error: None,
    }
}

pub(super) fn provider_usage_view(
    provider: &str,
    account: &str,
    plan: Option<&str>,
    bucket: &str,
    remaining: u8,
    fetched_at_epoch: i64,
) -> FocusedUsageView {
    FocusedUsageView {
        focused_agent: Some("codex".to_owned()),
        focused_provider: Some(provider.to_owned()),
        account: FocusedAccountHeader {
            provider_label: provider.to_owned(),
            account_label: account.to_owned(),
            username: None,
            plan_label: plan.map(str::to_owned),
            credential_origin: None,
        },
        buckets: vec![QuotaBucketView {
            used_money: None,
            limit_money: None,
            severity: jackin_protocol::control::UsageSeverity::default(),
            label: bucket.to_owned(),
            used_label: Some(format!("{}% used", 100_u8.saturating_sub(remaining))),
            limit_label: Some("100%".to_owned()),
            remaining_percent: Some(remaining),
            reset_label: Some("Resets at 15:00 UTC".to_owned()),
            resets_at: None,
            status_slot: None,
            pace_label: Some("On pace".to_owned()),
            status: UsageSnapshotStatus::Fresh,
        }],
        status: UsageSnapshotStatus::Fresh,
        source: UsageSource::ProviderApi,
        confidence: UsageConfidence::Authoritative,
        fetched_at_epoch,
        updated_label: "Updated just now".to_owned(),
        status_bar_label: format!("{bucket} {remaining}%"),
        tabs: Vec::new(),
        last_error: None,
    }
}
