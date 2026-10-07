// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Cursor` usage summary and stripe balance.

use super::cursor_rest_get;
use jackin_protocol::control::{QuotaBucketView, StatusSlot, UsageSnapshotStatus};
use jackin_usage_provider_core::{
    bucket, epoch_seconds_from_maybe_ms, format_currency, humanize_plan_label, json_number,
    parse_iso_epoch, quota_pace_label, timed_bucket, with_status_slot,
};

/// Structured usage summary (`/api/usage-summary`): exact cycle bounds,
/// percent buckets, and dollar figures. Dollar fields stay labels (scale
/// unverified); only the cycle percent backs a meter.
#[derive(Debug, Clone)]
pub(crate) struct CursorUsageSummary {
    pub(crate) cycle_start: Option<i64>,
    pub(crate) cycle_end: Option<i64>,
    pub(crate) membership: Option<String>,
    pub(crate) total_percent: Option<f64>,
    pub(crate) auto_percent: Option<f64>,
    pub(crate) api_percent: Option<f64>,
    pub(crate) on_demand: Option<f64>,
    pub(crate) overall_raw: Option<f64>,
    pub(crate) team_on_demand: Option<f64>,
    pub(crate) team_pooled: Option<f64>,
}

pub(crate) fn fetch_cursor_usage_summary(
    user_id: &str,
    token: &str,
) -> Result<CursorUsageSummary, String> {
    let value = cursor_rest_get(user_id, token, "/api/usage-summary")?;
    parse_cursor_usage_summary(&value)
        .ok_or_else(|| "Cursor usage summary was not recognized".to_owned())
}

pub(crate) fn parse_cursor_usage_summary(value: &serde_json::Value) -> Option<CursorUsageSummary> {
    let epoch = |key: &str| {
        value.get(key).and_then(|node| {
            node.as_str()
                .and_then(|text| parse_iso_epoch(text.trim()))
                .or_else(|| {
                    node.as_i64().map(epoch_seconds_from_maybe_ms).or_else(|| {
                        json_number(node).map(|n| epoch_seconds_from_maybe_ms(n.floor() as i64))
                    })
                })
        })
    };
    let individual = value.get("individualUsage");
    let plan = individual.and_then(|node| node.get("plan"));
    let percent = |node: Option<&serde_json::Value>, keys: &[&str]| {
        node.and_then(|node| {
            keys.iter()
                .filter_map(|key| node.get(*key).and_then(json_number))
                .find(|value| value.is_finite() && *value >= 0.0)
        })
    };
    let money = |node: Option<&serde_json::Value>, keys: &[&str]| {
        node.and_then(|node| {
            keys.iter()
                .filter_map(|key| node.get(*key).and_then(json_number))
                .find(|value| value.is_finite() && *value >= 0.0)
        })
    };
    let team = value.get("teamUsage");
    Some(CursorUsageSummary {
        cycle_start: epoch("billingCycleStart").or_else(|| epoch("billing_cycle_start")),
        cycle_end: epoch("billingCycleEnd").or_else(|| epoch("billing_cycle_end")),
        membership: value
            .get("membershipType")
            .or_else(|| value.get("membership_type"))
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|membership| !membership.is_empty())
            .map(humanize_plan_label),
        total_percent: percent(plan, &["totalPercentUsed", "total_percent_used"]),
        auto_percent: percent(plan, &["autoPercentUsed", "auto_percent_used"]),
        api_percent: percent(plan, &["apiPercentUsed", "api_percent_used"]),
        on_demand: money(individual, &["onDemand", "on_demand"]),
        overall_raw: money(individual, &["overall"]),
        team_on_demand: money(team, &["onDemand", "on_demand"]),
        team_pooled: money(team, &["pooled"]),
    })
}

pub(crate) fn cursor_summary_buckets(
    summary: &CursorUsageSummary,
    now: i64,
) -> Vec<QuotaBucketView> {
    let mut buckets = Vec::new();
    if let Some(used) = summary.total_percent {
        #[expect(
            clippy::cast_sign_loss,
            reason = "filtered non-negative; clamped 0..=100"
        )]
        let remaining = Some(100u8.saturating_sub(used.round().clamp(0.0, 100.0) as u8));
        let window = match (summary.cycle_start, summary.cycle_end) {
            (Some(start), Some(end)) if end > start => Some(end - start),
            _ => None,
        };
        let pace = quota_pace_label(remaining, summary.cycle_end, window, now);
        buckets.push(with_status_slot(
            timed_bucket(
                "Billing cycle",
                Some(format!("{used}% used")),
                Some("100%".to_owned()),
                remaining,
                summary.cycle_end,
                now,
                pace.as_deref(),
                UsageSnapshotStatus::Fresh,
            ),
            Some(StatusSlot::Weekly),
        ));
    }
    for (label, percent) in [("Auto", summary.auto_percent), ("API", summary.api_percent)] {
        if let Some(used) = percent {
            #[expect(
                clippy::cast_sign_loss,
                reason = "filtered non-negative; clamped 0..=100"
            )]
            let remaining = Some(100u8.saturating_sub(used.round().clamp(0.0, 100.0) as u8));
            buckets.push(timed_bucket(
                label,
                Some(format!("{used}% used")),
                Some("100%".to_owned()),
                remaining,
                summary.cycle_end,
                now,
                None,
                UsageSnapshotStatus::Fresh,
            ));
        }
    }
    if let Some(on_demand) = summary.on_demand {
        buckets.push(bucket(
            "On-demand (actual)",
            Some(format!("{} spent", format_currency(on_demand))),
            None,
            None,
            None,
            None,
            UsageSnapshotStatus::Fresh,
        ));
    }
    // `overall` has no documented unit: preserved raw, never interpreted as
    // money or percent.
    if let Some(overall) = summary.overall_raw {
        buckets.push(bucket(
            "Overall",
            Some(overall.to_string()),
            None,
            None,
            None,
            None,
            UsageSnapshotStatus::Fresh,
        ));
    }
    if let Some(on_demand) = summary.team_on_demand {
        buckets.push(bucket(
            "Team · On-demand",
            Some(format_currency(on_demand)),
            None,
            None,
            None,
            None,
            UsageSnapshotStatus::Fresh,
        ));
    }
    if let Some(pooled) = summary.team_pooled {
        buckets.push(bucket(
            "Team · Pooled",
            Some(format_currency(pooled)),
            None,
            None,
            None,
            None,
            UsageSnapshotStatus::Fresh,
        ));
    }
    buckets
}

/// Stripe balance in cents (`/api/auth/stripe`): explicit minor units.
pub(crate) fn fetch_cursor_stripe_balance(user_id: &str, token: &str) -> Result<i64, String> {
    let value = cursor_rest_get(user_id, token, "/api/auth/stripe")?;
    Ok(parse_cursor_stripe_balance(&value))
}

pub(crate) fn parse_cursor_stripe_balance(value: &serde_json::Value) -> i64 {
    ["balanceCents", "balance_cents", "balance", "total"]
        .into_iter()
        .filter_map(|key| value.get(key).and_then(json_number))
        .find(|value| value.is_finite() && *value >= 0.0)
        .map_or(0, |value| value.round() as i64)
}

// ---------------------------------------------------------------------------
// Enterprise Admin (separate scope)
// ---------------------------------------------------------------------------
