// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::cursor_rest_get;
use jackin_protocol::control::{
    CountQuota, CountQuotaPeriod, CountQuotaProvenance, CountQuotaUnit, QuotaBucketView,
    UsageSnapshotStatus,
};
use jackin_usage_provider_core::{bucket, compact_count};

#[derive(Debug, Clone)]
pub struct CursorRequestUsage {
    pub(crate) used: Option<u64>,
    pub(crate) limit: Option<u64>,
    pub(crate) remaining: Option<u64>,
    pub(crate) diagnostic: Option<String>,
}

pub fn fetch_cursor_request_usage(
    user_id: &str,
    token: &str,
) -> Result<CursorRequestUsage, String> {
    let value = cursor_rest_get(user_id, token, &format!("/api/usage?user={user_id}"))?;
    parse_cursor_request_usage(&value)
        .ok_or_else(|| "Cursor request usage was not recognized".to_owned())
}

pub fn parse_cursor_request_usage(value: &serde_json::Value) -> Option<CursorRequestUsage> {
    let entry = value
        .as_object()?
        .iter()
        .filter(|(_, node)| has_cursor_count_field(node))
        .min_by(|(left, _), (right, _)| left.cmp(right))?
        .1;
    let limit_value = first_cursor_field(entry, &["maxRequestUsage", "max_request_usage"]);
    let used_value = first_cursor_field(
        entry,
        &[
            "numRequestsTotal",
            "numRequests",
            "num_requests_total",
            "used",
        ],
    );
    let remaining_value = first_cursor_field(
        entry,
        &["remainingRequests", "remaining_requests", "remaining"],
    );
    let limit = limit_value.and_then(cursor_count);
    let used = used_value.and_then(cursor_count);
    let remaining = remaining_value.and_then(cursor_count);
    let invalid = [
        ("limit", limit_value, limit),
        ("used", used_value, used),
        ("remaining", remaining_value, remaining),
    ]
    .into_iter()
    .filter_map(|(name, raw, parsed)| {
        (raw.is_some_and(|value| !value.is_null()) && parsed.is_none()).then_some(name)
    })
    .collect::<Vec<_>>();
    Some(CursorRequestUsage {
        used,
        limit,
        remaining,
        diagnostic: (!invalid.is_empty())
            .then(|| format!("invalid count fields: {}", invalid.join(", "))),
    })
}

pub fn cursor_request_bucket(usage: &CursorRequestUsage) -> QuotaBucketView {
    let has_count = usage.used.is_some() || usage.limit.is_some() || usage.remaining.is_some();
    let mut view = bucket(
        "Requests",
        usage.used.map(compact_count),
        usage.limit.map(compact_count),
        None,
        None,
        usage.diagnostic.as_deref(),
        if has_count {
            UsageSnapshotStatus::Fresh
        } else {
            UsageSnapshotStatus::Unavailable
        },
    );
    if has_count {
        view.count_quota = Some(CountQuota::new(
            usage.used,
            usage.limit,
            usage.remaining,
            CountQuotaUnit::Requests,
            CountQuotaPeriod::Unknown,
            CountQuotaProvenance::ProviderReported,
        ));
    }
    view
}

fn has_cursor_count_field(entry: &serde_json::Value) -> bool {
    [
        &["maxRequestUsage", "max_request_usage"][..],
        &[
            "numRequestsTotal",
            "numRequests",
            "num_requests_total",
            "used",
        ][..],
        &["remainingRequests", "remaining_requests", "remaining"][..],
    ]
    .into_iter()
    .flatten()
    .any(|key| entry.get(key).is_some())
}

fn first_cursor_field<'a>(
    entry: &'a serde_json::Value,
    keys: &[&str],
) -> Option<&'a serde_json::Value> {
    keys.iter().find_map(|key| entry.get(key))
}

fn cursor_count(value: &serde_json::Value) -> Option<u64> {
    value.as_u64().or_else(|| {
        value
            .as_str()
            .and_then(|text| text.trim().parse::<u64>().ok())
    })
}
