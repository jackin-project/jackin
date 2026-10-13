// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Cursor` period usage fetch and buckets.

use super::super::{
    QuotaBucketView, StatusSlot, UsageSnapshotStatus, bucket, format_currency, json_number,
    provider_http_client, timed_bucket, with_status_slot,
};
use super::cursor_dashboard_url_with_base;

/// Connect-protocol POST: JSON body `{}`, bearer auth, protocol version 1.
pub(crate) fn cursor_dashboard_post(
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

/// Current-period allowance/spend. `total_spend`/`limit` are display-scale
/// numbers (major units); the scale is unverified, so they stay labels and
/// never become structured [`Money`].
#[derive(Debug, Clone)]
pub(crate) struct CursorPeriodUsage {
    pub(crate) enabled: bool,
    pub(crate) total_percent_used: Option<f64>,
    pub(crate) limit: Option<f64>,
    pub(crate) total_spend: Option<f64>,
    pub(crate) is_team: bool,
}

pub(crate) fn fetch_cursor_period_usage(
    base: &str,
    token: &str,
) -> Result<CursorPeriodUsage, String> {
    let value = cursor_dashboard_post(base, token, "GetCurrentPeriodUsage")?;
    parse_cursor_period_usage(&value)
        .ok_or_else(|| "Cursor period usage was not recognized".to_owned())
}

pub(crate) fn parse_cursor_period_usage(value: &serde_json::Value) -> Option<CursorPeriodUsage> {
    // Live `GetCurrentPeriodUsage` returns `planUsage` at the top level; older
    // captures nested it under `usage`. Accept both, preferring nested.
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

/// `planUsage` present but limit-less: fall back to request-based/REST meters
/// instead of rendering a limit the server never stated.
pub(crate) fn cursor_needs_request_fallback(usage: &CursorPeriodUsage) -> bool {
    usage.limit.is_none()
}

pub(crate) fn cursor_period_buckets(
    usage: &CursorPeriodUsage,
    cycle_end: Option<i64>,
    now: i64,
) -> Vec<QuotaBucketView> {
    let mut buckets = Vec::new();
    if let Some(used) = usage.total_percent_used {
        #[expect(
            clippy::cast_sign_loss,
            reason = "filtered non-negative; clamped 0..=100"
        )]
        let remaining = Some(100u8.saturating_sub(used.round().clamp(0.0, 100.0) as u8));
        buckets.push(with_status_slot(
            timed_bucket(
                "Billing cycle",
                Some(format!("{used}% used")),
                Some("100%".to_owned()),
                remaining,
                cycle_end,
                now,
                None,
                UsageSnapshotStatus::Fresh,
            ),
            // The billing-cycle meter is the primary quota (a cycle meter, so
            // the Weekly slot per the Grok precedent for cycle meters).
            Some(StatusSlot::Weekly),
        ));
    }
    if let Some(spend) = usage.total_spend {
        let limit_label = usage.limit.map(format_currency);
        let remaining = usage.limit.and_then(|limit| {
            if limit > 0.0 {
                #[expect(clippy::cast_sign_loss, reason = "clamped to 0.0..=100.0")]
                {
                    Some(((limit - spend).clamp(0.0, limit) / limit * 100.0).round() as u8)
                }
            } else {
                None
            }
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
            // Labels only (unverified scale): a detail Spend row without
            // structured money, so no headline figure is derived from it.
            Some(StatusSlot::Spend),
        ));
    }
    buckets
}
