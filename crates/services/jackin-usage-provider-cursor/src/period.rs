// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::cursor_dashboard_url_with_base;
use jackin_protocol::control::{
    CanonicalRemainingPercent, CanonicalUsedPercent, PercentObservationOrigin, QuotaBucketView,
    StatusSlot, UsageSnapshotStatus,
};
use jackin_usage_provider_core::{
    bucket, format_currency, json_number, provider_http_client, timed_bucket, with_status_slot,
    with_used_percent,
};

pub fn cursor_dashboard_post(
    base: &str,
    token: &str,
    method: &str,
) -> Result<serde_json::Value, String> {
    let client = provider_http_client()?;
    let response = client
        .post(cursor_dashboard_url_with_base(base, method))
        .bearer_auth(token)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(
            reqwest::header::HeaderName::from_static("connect-protocol-version"),
            "1",
        )
        .body("{}")
        .send()
        .map_err(|error| format!("Cursor {method} request failed: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("Cursor {method} HTTP {status}"));
    }
    response
        .json::<serde_json::Value>()
        .map_err(|error| format!("Cursor {method} decode failed: {error}"))
}

#[derive(Debug, Clone)]
pub struct CursorPeriodUsage {
    pub(crate) enabled: bool,
    pub(crate) total_percent_used: Option<f64>,
    pub(crate) limit: Option<f64>,
    pub(crate) total_spend: Option<f64>,
    pub(crate) is_team: bool,
}

pub fn fetch_cursor_period_usage(base: &str, token: &str) -> Result<CursorPeriodUsage, String> {
    let value = cursor_dashboard_post(base, token, "GetCurrentPeriodUsage")?;
    parse_cursor_period_usage(&value)
        .ok_or_else(|| "Cursor period usage was not recognized".to_owned())
}

pub fn parse_cursor_period_usage(value: &serde_json::Value) -> Option<CursorPeriodUsage> {
    let usage = value.get("usage").unwrap_or(value);
    let plan = usage.get("planUsage")?;
    let total_percent_used = ["totalPercentUsed", "total_percent_used", "percentUsed"]
        .into_iter()
        .filter_map(|key| plan.get(key).and_then(json_number))
        .find(|value| value.is_finite() && *value >= 0.0);
    let limit = ["limit", "allowance"]
        .into_iter()
        .filter_map(|key| plan.get(key).and_then(json_number))
        .find(|value| value.is_finite() && *value > 0.0);
    let total_spend = ["totalSpend", "total_spend", "spend"]
        .into_iter()
        .filter_map(|key| plan.get(key).and_then(json_number))
        .find(|value| value.is_finite() && *value >= 0.0);
    let spend_limit = usage.get("spendLimitUsage");
    let is_team = spend_limit
        .and_then(|node| node.get("limitType").or_else(|| node.get("limit_type")))
        .and_then(serde_json::Value::as_str)
        .is_some_and(|limit| limit.eq_ignore_ascii_case("team"))
        || spend_limit
            .and_then(|node| node.get("pooledLimit").or_else(|| node.get("pooled_limit")))
            .and_then(json_number)
            .is_some_and(|pooled| pooled > 0.0);
    Some(CursorPeriodUsage {
        enabled: usage
            .get("enabled")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true),
        total_percent_used,
        limit,
        total_spend,
        is_team,
    })
}

pub fn cursor_needs_request_fallback(usage: &CursorPeriodUsage) -> bool {
    usage.limit.is_none()
}

pub fn cursor_period_buckets(
    usage: &CursorPeriodUsage,
    cycle_end: Option<i64>,
    now: i64,
) -> Vec<QuotaBucketView> {
    let mut buckets = Vec::new();
    if let Some(used) = usage.total_percent_used {
        let used_percent = CanonicalUsedPercent::new(used).ok();
        buckets.push(with_status_slot(
            with_used_percent(
                timed_bucket(
                    "Billing cycle",
                    used_percent.map(CanonicalUsedPercent::label),
                    Some("100%".to_owned()),
                    None,
                    cycle_end,
                    now,
                    None,
                    UsageSnapshotStatus::Fresh,
                ),
                used_percent.map(|percent| (percent, PercentObservationOrigin::ProviderReported)),
            ),
            Some(StatusSlot::Weekly),
        ));
    }
    if let Some(spend) = usage.total_spend {
        let limit_label = usage.limit.map(format_currency);
        let remaining = usage.limit.and_then(|limit| {
            (limit > 0.0)
                .then(|| CanonicalRemainingPercent::new((limit - spend) / limit * 100.0).ok())
                .flatten()
                .map(|percent| (percent, PercentObservationOrigin::Derived))
        });
        buckets.push(with_status_slot(
            bucket(
                "Spend (actual)",
                Some(format!("{} spent", format_currency(spend))),
                limit_label,
                remaining,
                None,
                None,
                UsageSnapshotStatus::Fresh,
            ),
            Some(StatusSlot::Spend),
        ));
    }
    buckets
}
