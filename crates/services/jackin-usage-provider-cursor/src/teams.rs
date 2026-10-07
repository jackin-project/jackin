// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Cursor` enterprise team spend.

use jackin_protocol::control::{QuotaBucketView, StatusSlot, UsageSnapshotStatus};
use jackin_usage_provider_core::{
    bucket, epoch_seconds_from_maybe_ms, format_currency, json_number, parse_iso_epoch,
    provider_http_client, timed_bucket, with_status_slot,
};

/// Explicit Enterprise Admin credential. Never derived from a personal key: a
/// personal execution key does not imply reporting access.
// No `Debug`: this carries a live admin token and must never be formatted
// into a log or error.
#[expect(
    missing_debug_implementations,
    reason = "credential type: a live admin token must never be formatted into a log or error"
)]
#[derive(Clone)]
pub struct CursorEnterpriseScope {
    pub(crate) admin_token: String,
    pub(crate) team_id: Option<String>,
}

pub fn cursor_teams_spend_url() -> &'static str {
    "https://api.cursor.com/teams/spend"
}

pub fn cursor_teams_events_url() -> &'static str {
    "https://api.cursor.com/teams/filtered-usage-events"
}

/// One member's spend row: team and member scopes stay distinct buckets.
#[derive(Debug, Clone)]
pub struct CursorMemberSpend {
    pub(crate) label: String,
    pub(crate) charged: Option<f64>,
}

/// Team spend report: actual charged vs estimated model cost, kept separate.
#[derive(Debug, Clone)]
pub struct CursorTeamSpend {
    pub(crate) charged: Option<f64>,
    pub(crate) estimated: Option<f64>,
    pub(crate) period_end: Option<i64>,
    pub(crate) members: Vec<CursorMemberSpend>,
}

pub fn fetch_cursor_team_spend(scope: &CursorEnterpriseScope) -> Result<CursorTeamSpend, String> {
    let client = provider_http_client()?;
    let mut body = serde_json::Map::new();
    if let Some(team_id) = scope.team_id.as_deref() {
        body.insert(
            "teamId".to_owned(),
            serde_json::Value::String(team_id.to_owned()),
        );
    }
    let response = client
        .post(cursor_teams_spend_url())
        .bearer_auth(&scope.admin_token)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(reqwest::header::ACCEPT, "application/json")
        .json(&body)
        .send()
        .map_err(|error| format!("Cursor team spend request failed: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("Cursor team spend HTTP {status}"));
    }
    let value = response
        .json::<serde_json::Value>()
        .map_err(|error| format!("Cursor team spend decode failed: {error}"))?;
    parse_cursor_team_spend(&value).ok_or_else(|| "Cursor team spend was not recognized".to_owned())
}

pub fn parse_cursor_team_spend(value: &serde_json::Value) -> Option<CursorTeamSpend> {
    let money = |node: &serde_json::Value, keys: &[&str]| {
        keys.iter()
            .filter_map(|key| node.get(*key).and_then(json_number))
            .find(|value| value.is_finite() && *value >= 0.0)
    };
    let charged = money(
        value,
        &[
            "chargedAmount",
            "charged_amount",
            "totalCharged",
            "spend",
            "totalSpend",
        ],
    );
    let estimated = money(
        value,
        &["estimatedCost", "estimated_cost", "estimated", "modelCost"],
    );
    let period_end = ["periodEnd", "period_end", "billingCycleEnd"]
        .into_iter()
        .filter_map(|key| value.get(key))
        .find_map(|node| {
            node.as_str()
                .and_then(|text| parse_iso_epoch(text.trim()))
                .or_else(|| {
                    json_number(node).map(|n| epoch_seconds_from_maybe_ms(n.floor() as i64))
                })
        });
    let members: Vec<CursorMemberSpend> = value
        .get("members")
        .or_else(|| value.get("memberSpend"))
        .and_then(serde_json::Value::as_array)
        .map(|members| {
            members
                .iter()
                .filter_map(|member| {
                    let label = ["email", "name", "memberEmail", "userId"]
                        .into_iter()
                        .filter_map(|key| member.get(key).and_then(serde_json::Value::as_str))
                        .map(str::trim)
                        .find(|label| !label.is_empty())?
                        .to_owned();
                    Some(CursorMemberSpend {
                        label,
                        charged: money(
                            member,
                            &["chargedAmount", "charged_amount", "spend", "totalSpend"],
                        ),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    if charged.is_none() && estimated.is_none() && members.is_empty() {
        return None;
    }
    Some(CursorTeamSpend {
        charged,
        estimated,
        period_end,
        members,
    })
}

pub fn cursor_team_spend_buckets(spend: &CursorTeamSpend, now: i64) -> Vec<QuotaBucketView> {
    let mut buckets = Vec::new();
    if let Some(charged) = spend.charged {
        buckets.push(with_status_slot(
            timed_bucket(
                "Team spend (actual)",
                Some(format!("{} spent", format_currency(charged))),
                None,
                None,
                spend.period_end,
                now,
                None,
                UsageSnapshotStatus::Fresh,
            ),
            Some(StatusSlot::Spend),
        ));
    }
    // Estimated model cost is display-only: never the Spend slot, never mixed
    // into the actual charged figure.
    if let Some(estimated) = spend.estimated {
        buckets.push(bucket(
            "Estimated model cost",
            Some(format_currency(estimated)),
            None,
            None,
            None,
            Some("estimate · not billed"),
            UsageSnapshotStatus::Fresh,
        ));
    }
    for member in &spend.members {
        buckets.push(bucket(
            &format!("Team · {}", member.label),
            member.charged.map(format_currency),
            None,
            None,
            None,
            None,
            UsageSnapshotStatus::Fresh,
        ));
    }
    buckets
}
