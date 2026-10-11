// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Cursor` plan info and credit grants.

use super::cursor_dashboard_post;
use jackin_protocol::control::{Money, QuotaBucketView, UsageSnapshotStatus};
use jackin_usage_provider_core::{bucket, format_cents, humanize_plan_label, json_number};

pub fn fetch_cursor_plan_info(base: &str, token: &str) -> Result<Option<String>, String> {
    let value = cursor_dashboard_post(base, token, "GetPlanInfo")?;
    Ok(parse_cursor_plan_info(&value))
}

pub fn parse_cursor_plan_info(value: &serde_json::Value) -> Option<String> {
    // Live `GetPlanInfo` nests the label under `planInfo` (e.g.
    // `{"planInfo": {"planName": "ultra"}}`); the flat top-level keys are the
    // fallback for older/assumed shapes. Each shape is a full attempt so a
    // blank nested label still falls back to the flat keys.
    value
        .get("planInfo")
        .or_else(|| value.get("plan_info"))
        .and_then(cursor_plan_label_from)
        .or_else(|| cursor_plan_label_from(value))
        .map(humanize_plan_label)
}

pub(crate) fn cursor_plan_label_from(node: &serde_json::Value) -> Option<&str> {
    node.get("planName")
        .or_else(|| node.get("plan_name"))
        .or_else(|| node.get("plan"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|plan| !plan.is_empty())
}

/// Credit-grant balance in cents (explicit minor units → safe [`Money`]).
pub fn fetch_cursor_credit_grants(base: &str, token: &str) -> Result<i64, String> {
    let value = cursor_dashboard_post(base, token, "GetCreditGrantsBalance")?;
    Ok(parse_cursor_credit_grants(&value))
}

pub fn parse_cursor_credit_grants(value: &serde_json::Value) -> i64 {
    // The server returns either a top-level total or the total plus its
    // itemized breakdown — never add both. A non-empty `grants[]` wins and the
    // top-level total is ignored, so the usual total+breakdown shape cannot
    // double-count.
    if let Some(grants) = value
        .get("grants")
        .and_then(serde_json::Value::as_array)
        .filter(|grants| !grants.is_empty())
    {
        return grants
            .iter()
            .filter_map(|grant| {
                grant
                    .get("amountCents")
                    .or_else(|| grant.get("amount"))
                    .and_then(json_number)
            })
            .filter(|value| value.is_finite() && *value >= 0.0)
            .map(|value| value.round() as i64)
            .sum();
    }
    [
        "grantTotal",
        "grant_total",
        "totalCents",
        "balanceCents",
        "balance",
    ]
    .into_iter()
    .filter_map(|key| value.get(key).and_then(json_number))
    .find(|value| value.is_finite() && *value >= 0.0)
    .map_or(0, |value| value.round() as i64)
}

/// One Credits row: grant cents + Stripe balance cents (both explicit minor
/// units). A balance, not spend — no headline slot.
///
/// Assumption (research-unverified, near-certain): Cursor credit grants bill
/// in USD, so the structured [`Money`] carries `USD`/exponent 2 and the label
/// renders `$`. Revisit if a non-USD Cursor ledger is ever observed.
pub fn cursor_credits_bucket(grant_cents: i64, stripe_cents: i64) -> QuotaBucketView {
    let total = grant_cents.saturating_add(stripe_cents).max(0);
    let mut view = bucket(
        "Credits",
        Some(format_cents(total)),
        None,
        None,
        None,
        None,
        UsageSnapshotStatus::Fresh,
    );
    view.used_money = Some(Money::new(total, "USD", 2));
    view
}
