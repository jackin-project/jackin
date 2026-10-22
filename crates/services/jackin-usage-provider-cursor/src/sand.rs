// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::cursor_dashboard_post;
use jackin_protocol::control::{
    CanonicalUsedPercent, PercentObservationOrigin, QuotaBucketView, UsageSnapshotStatus,
};
use jackin_usage_provider_core::{
    epoch_seconds_from_maybe_ms, json_number, parse_iso_epoch, timed_bucket, with_used_percent,
};

#[derive(Debug, Clone)]
pub struct CursorSandUsage {
    pub(crate) usage_percent: f64,
    pub(crate) reset_at: Option<i64>,
}

pub fn fetch_cursor_sand_usage(base: &str, token: &str) -> Result<Option<CursorSandUsage>, String> {
    let value = cursor_dashboard_post(base, token, "GetSandUsageStatus")?;
    Ok(parse_cursor_sand_usage(&value))
}

pub fn parse_cursor_sand_usage(value: &serde_json::Value) -> Option<CursorSandUsage> {
    if value
        .get("usesPooledEnterpriseAllowance")
        .and_then(serde_json::Value::as_bool)
        == Some(true)
    {
        return None;
    }
    if value
        .get("includedLimitZero")
        .and_then(serde_json::Value::as_bool)
        == Some(true)
        || value
            .get("hasNonZeroIncludedLimit")
            .and_then(serde_json::Value::as_bool)
            == Some(false)
    {
        return None;
    }
    let usage_percent = ["usagePercent", "usage_percent", "percentUsed"]
        .into_iter()
        .filter_map(|key| value.get(key).and_then(json_number))
        .find(|value| value.is_finite() && *value >= 0.0)?;
    let reset_at = [
        "nextResetTimestampUtc",
        "next_reset",
        "resetsAt",
        "reset_at",
    ]
    .into_iter()
    .filter_map(|key| value.get(key))
    .find_map(|node| {
        node.as_str()
            .and_then(|text| parse_iso_epoch(text.trim()))
            .or_else(|| json_number(node).map(|n| epoch_seconds_from_maybe_ms(n.floor() as i64)))
    });
    Some(CursorSandUsage {
        usage_percent,
        reset_at,
    })
}

pub fn cursor_sand_bucket(sand: &CursorSandUsage, now: i64) -> QuotaBucketView {
    let used = CanonicalUsedPercent::new(sand.usage_percent).ok();
    let view = timed_bucket(
        "Grok Bot",
        used.map(CanonicalUsedPercent::label),
        Some("100%".to_owned()),
        None,
        sand.reset_at,
        now,
        None,
        UsageSnapshotStatus::Fresh,
    );
    with_used_percent(
        view,
        used.map(|percent| (percent, PercentObservationOrigin::ProviderReported)),
    )
}
