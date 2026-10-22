// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::control::{
    CanonicalUsedPercent, Money, PercentObservationOrigin, QuotaBucketView, StatusSlot,
    UsageSeverity, UsageSnapshotStatus,
};
use jackin_usage_provider_core::{
    CLAUDE_CODE_USER_AGENT_FALLBACK, ProviderHttpError, bucket, get_json_bearer, humanize_reason,
    humanize_window_label, parse_iso_epoch, severity_from_label, timed_bucket, with_used_percent,
};
use std::collections::BTreeMap;

use super::{
    ClaudeOAuthExtraUsage, ClaudeOAuthMoney, ClaudeOAuthSpend, ClaudeOAuthUsageResponse,
    ClaudeOAuthUsageWindow,
};

pub fn push_claude_dollar_windows(
    buckets: &mut Vec<QuotaBucketView>,
    other: BTreeMap<String, serde_json::Value>,
    now: i64,
) {
    for (key, value) in other {
        let Ok(window) = serde_json::from_value::<ClaudeOAuthUsageWindow>(value) else {
            continue;
        };
        let Some(limit) = window.limit_dollars.filter(|limit| *limit > 0.0) else {
            continue;
        };
        let used = window.used_dollars.unwrap_or(0.0).max(0.0);
        let used_money = Money::new((used * 100.0).round() as i64, "USD", 2);
        let limit_money = Money::new((limit * 100.0).round() as i64, "USD", 2);
        let reset_at = window.resets_at.as_deref().and_then(parse_iso_epoch);
        let mut view = timed_bucket(
            &humanize_window_label(&key),
            Some(format!("{used_money} spent")),
            Some(limit_money.to_string()),
            None,
            reset_at,
            now,
            None,
            UsageSnapshotStatus::Fresh,
        );
        view.used_money = Some(used_money);
        view.limit_money = Some(limit_money);
        buckets.push(view);
    }
}

#[derive(Debug)]
pub struct ClaudeSpend {
    pub used: Money,
    pub limit: Option<Money>,
    pub used_percent: Option<f64>,
    pub enabled: bool,
    pub disabled_reason: Option<String>,
    pub severity: UsageSeverity,
}

pub fn claude_spend_bucket(
    spend: Option<ClaudeOAuthSpend>,
    extra: Option<ClaudeOAuthExtraUsage>,
) -> Option<QuotaBucketView> {
    let spend = normalize_claude_spend(spend, extra)?;
    let used_percent = spend
        .used_percent
        .and_then(|used| CanonicalUsedPercent::new(used).ok());
    let used_label = Some(format!("{} spent", spend.used));
    let limit_label = spend.limit.as_ref().map(Money::to_string);
    let pace = if spend.enabled {
        used_percent.map(CanonicalUsedPercent::label)
    } else {
        Some(match &spend.disabled_reason {
            Some(reason) => format!("disabled · {}", humanize_reason(reason)),
            None => "disabled".to_owned(),
        })
    };
    let view = bucket(
        "Extra usage",
        used_label,
        limit_label,
        None,
        None,
        pace.as_deref(),
        UsageSnapshotStatus::Fresh,
    );
    let mut view = with_used_percent(
        view,
        used_percent.map(|percent| (percent, PercentObservationOrigin::ProviderReported)),
    );
    view.status_slot = Some(StatusSlot::Spend);
    view.severity = spend.severity;
    view.used_money = Some(spend.used);
    view.limit_money = spend.limit;
    Some(view)
}

pub fn normalize_claude_spend(
    spend: Option<ClaudeOAuthSpend>,
    extra: Option<ClaudeOAuthExtraUsage>,
) -> Option<ClaudeSpend> {
    if let Some(spend) = spend
        && let Some(used) = spend.used.and_then(ClaudeOAuthMoney::into_money)
    {
        return Some(ClaudeSpend {
            used,
            limit: spend.limit.and_then(ClaudeOAuthMoney::into_money),
            used_percent: spend.percent,
            enabled: spend.enabled.unwrap_or(true),
            disabled_reason: spend.disabled_reason,
            severity: severity_from_label(spend.severity.as_deref()),
        });
    }
    let extra = extra?;
    let used_credits = extra.used_credits?;
    let exponent = extra.decimal_places.unwrap_or(2);
    let currency = extra.currency.unwrap_or_else(|| "credits".to_owned());
    let used = Money::new(used_credits.round() as i64, &currency, exponent);
    let limit = extra
        .monthly_limit
        .map(|limit| Money::new(limit.round() as i64, &currency, exponent));
    Some(ClaudeSpend {
        used,
        limit,
        used_percent: extra
            .utilization
            .and_then(|points| CanonicalUsedPercent::new(points).ok())
            .map(CanonicalUsedPercent::get),
        enabled: extra.is_enabled.unwrap_or(true),
        disabled_reason: extra.disabled_reason,
        severity: UsageSeverity::Normal,
    })
}

pub fn fetch_claude_oauth_usage(
    access_token: &str,
) -> Result<ClaudeOAuthUsageResponse, ProviderHttpError> {
    get_json_bearer(
        jackin_telemetry::schema::enums::ProviderName::Anthropic,
        "/api/oauth/usage",
        "Claude OAuth usage",
        "https://api.anthropic.com/api/oauth/usage",
        access_token,
        &[
            (reqwest::header::CONTENT_TYPE, "application/json"),
            (
                reqwest::header::HeaderName::from_static("anthropic-beta"),
                "oauth-2025-04-20",
            ),
            (reqwest::header::USER_AGENT, CLAUDE_CODE_USER_AGENT_FALLBACK),
        ],
    )
}
