// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Cursor` request usage fetch and buckets.

use super::super::{QuotaBucketView, UsageSnapshotStatus, bucket, compact_count, json_number};
use super::cursor_rest_get;

/// Request allowance (`/api/usage?user=`): used/total request counts.
#[derive(Debug, Clone)]
pub(crate) struct CursorRequestUsage {
    pub(crate) used: i64,
    pub(crate) limit: i64,
}

pub(crate) fn fetch_cursor_request_usage(
    user_id: &str,
    token: &str,
) -> Result<CursorRequestUsage, String> {
    let value = cursor_rest_get(user_id, token, &format!("/api/usage?user={user_id}"))?;
    parse_cursor_request_usage(&value)
        .ok_or_else(|| "Cursor request usage was not recognized".to_owned())
}

pub(crate) fn parse_cursor_request_usage(value: &serde_json::Value) -> Option<CursorRequestUsage> {
    // Model-keyed (`gpt-4`, …): scan top-level objects for a request counter.
    // Multi-model responses are pinned, not arbitrary: the alphabetically
    // first model key carrying a counter wins, so the pick is deterministic
    // regardless of JSON/map iteration order. (Summing across models would
    // risk double-counting one shared allowance.)
    let entry = value
        .as_object()?
        .iter()
        .filter(|(_, node)| {
            node.get("maxRequestUsage")
                .or_else(|| node.get("max_request_usage"))
                .is_some()
        })
        .min_by(|(left, _), (right, _)| left.cmp(right))?
        .1;
    let limit = entry
        .get("maxRequestUsage")
        .or_else(|| entry.get("max_request_usage"))
        .and_then(json_number)
        .filter(|limit| *limit > 0.0)
        .map(|limit| limit.round() as i64)?;
    let used = [
        "numRequestsTotal",
        "numRequests",
        "num_requests_total",
        "used",
    ]
    .into_iter()
    .filter_map(|key| entry.get(key).and_then(json_number))
    .find(|value| value.is_finite() && *value >= 0.0)
    .map_or(0, |value| value.round() as i64);
    Some(CursorRequestUsage { used, limit })
}

pub(crate) fn cursor_request_bucket(usage: &CursorRequestUsage) -> QuotaBucketView {
    #[expect(clippy::cast_sign_loss, reason = "clamped to 0.0..=100.0")]
    let remaining = Some(
        ((usage.limit - usage.used).clamp(0, usage.limit) as f64 / usage.limit as f64 * 100.0)
            .round() as u8,
    );
    bucket(
        "Requests",
        Some(compact_count(u64::try_from(usage.used.max(0)).unwrap_or(0))),
        Some(compact_count(
            u64::try_from(usage.limit.max(0)).unwrap_or(0),
        )),
        remaining,
        None,
        None,
        UsageSnapshotStatus::Fresh,
    )
}
