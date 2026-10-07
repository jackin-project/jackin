// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Cursor` usage events fetch and buckets.

use super::{CursorEnterpriseScope, cursor_teams_events_url};
use jackin_protocol::control::{QuotaBucketView, UsageSnapshotStatus};
use jackin_usage_provider_core::{
    bucket, compact_count, format_currency, json_number, provider_http_client,
};

/// Hourly-aggregated usage events. The overview snapshot never fetches these
/// (aggregation delay + hammering); the fetch exists for detail drill-down.
#[derive(Debug, Clone, Default)]
pub struct CursorUsageEvents {
    pub(crate) event_count: usize,
    pub(crate) total_tokens: Option<i64>,
    pub(crate) total_charged: Option<f64>,
    pub(crate) total_estimated: Option<f64>,
}

pub fn fetch_cursor_usage_events(
    scope: &CursorEnterpriseScope,
    start_ms: i64,
    end_ms: i64,
) -> Result<CursorUsageEvents, String> {
    let client = provider_http_client()?;
    let mut body = serde_json::Map::from_iter([
        (
            "startDate".to_owned(),
            serde_json::Value::Number(start_ms.into()),
        ),
        (
            "endDate".to_owned(),
            serde_json::Value::Number(end_ms.into()),
        ),
    ]);
    if let Some(team_id) = scope.team_id.as_deref() {
        body.insert(
            "teamId".to_owned(),
            serde_json::Value::String(team_id.to_owned()),
        );
    }
    let response = client
        .post(cursor_teams_events_url())
        .bearer_auth(&scope.admin_token)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(reqwest::header::ACCEPT, "application/json")
        .json(&body)
        .send()
        .map_err(|error| format!("Cursor usage events request failed: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("Cursor usage events HTTP {status}"));
    }
    let value = response
        .json::<serde_json::Value>()
        .map_err(|error| format!("Cursor usage events decode failed: {error}"))?;
    Ok(parse_cursor_usage_events(&value))
}

pub fn parse_cursor_usage_events(value: &serde_json::Value) -> CursorUsageEvents {
    let events = value
        .get("events")
        .or_else(|| value.get("usageEvents"))
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let sum = |keys: &[&str]| {
        let total: f64 = events
            .iter()
            .filter_map(|event| {
                keys.iter()
                    .filter_map(|key| event.get(*key).and_then(json_number))
                    .find(|value| value.is_finite() && *value >= 0.0)
            })
            .sum();
        (total > 0.0).then_some(total)
    };
    CursorUsageEvents {
        event_count: events.len(),
        total_tokens: sum(&["tokens", "totalTokens"]).map(|total| total.round() as i64),
        total_charged: sum(&["chargedAmount", "charged_amount", "cost", "charged"]),
        total_estimated: sum(&["estimatedCost", "estimated_cost", "estimated"]),
    }
}

pub fn cursor_events_buckets(events: &CursorUsageEvents) -> Vec<QuotaBucketView> {
    let mut buckets = Vec::new();
    if let Some(charged) = events.total_charged {
        buckets.push(bucket(
            "Events · Charged (actual)",
            Some(format_currency(charged)),
            None,
            None,
            None,
            Some(&format!(
                "{} events · hourly aggregated",
                events.event_count
            )),
            UsageSnapshotStatus::Fresh,
        ));
    }
    if let Some(estimated) = events.total_estimated {
        buckets.push(bucket(
            "Events · Estimated",
            Some(format_currency(estimated)),
            None,
            None,
            None,
            Some("estimate · not billed"),
            UsageSnapshotStatus::Fresh,
        ));
    }
    if let Some(tokens) = events.total_tokens {
        buckets.push(bucket(
            "Events · Tokens",
            Some(compact_count(u64::try_from(tokens.max(0)).unwrap_or(0))),
            None,
            None,
            None,
            Some(&format!(
                "{} events · hourly aggregated",
                events.event_count
            )),
            UsageSnapshotStatus::Fresh,
        ));
    }
    buckets
}

// ---------------------------------------------------------------------------
// Snapshots
// ---------------------------------------------------------------------------
