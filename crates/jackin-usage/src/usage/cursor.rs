// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `Cursor` usage snapshot: personal allowance vs Enterprise Admin reporting.
//!
//! Two scopes, never mixed (see `ref-contracts-C.md` §2):
//!
//! * Personal (selected account token): `DashboardService` Connect RPC on
//!   `api2.cursor.sh` (`GetCurrentPeriodUsage`, `GetPlanInfo`,
//!   `GetCreditGrantsBalance`, `GetSandUsageStatus`) plus `cursor.com` session
//!   REST (`/api/usage?user=`, `/api/usage-summary`, `/api/auth/stripe`).
//!   A personal key never implies Enterprise reporting access.
//! * Enterprise Admin (`api.cursor.com/teams/spend|filtered-usage-events`):
//!   separate [`CursorEnterpriseScope`] with explicit admin auth; the personal
//!   snapshot never touches admin hosts. The events API is hourly aggregated
//!   and is never polled at the overview interval.
//!
//! Money discipline: actual charged amounts, estimated model cost, included
//! quota, and credit grants are separate buckets. Estimates never back a
//! Spend-slot headline.

use super::*;

const CURSOR_DEFAULT_DASHBOARD_BASE: &str = "https://api2.cursor.sh";
const CURSOR_SESSION_BASE: &str = "https://cursor.com";

// ---------------------------------------------------------------------------
// Auth
// ---------------------------------------------------------------------------

/// Selected-account Cursor credential: bearer + derived user id. The user id
/// comes from the JWT `sub` claim (part after `|`), never from config alone.
// No `Debug`: this carries a live access token and must never be formatted
// into a log or error (Claude credentials omit `Debug` for the same reason).
#[derive(Clone)]
pub(crate) struct CursorAuth {
    pub(crate) access_token: String,
    pub(crate) user_id: Option<String>,
}

pub(crate) fn cursor_auth_path() -> PathBuf {
    env_value("CURSOR_CONFIG_DIR").map_or_else(
        || home_path(".cursor/auth.json"),
        |dir| PathBuf::from(dir).join("auth.json"),
    )
}

/// Pure `auth.json` parse: the ambient loader, per-profile snapshots, and the
/// discovery lane mint broker material from a selected profile root through
/// this, so broker refresh never re-resolves the default home for a
/// non-default registered root. `None` is a present-but-tokenless file.
pub(crate) fn cursor_auth_from_value(value: &serde_json::Value) -> Option<CursorAuth> {
    let access_token = ["accessToken", "access_token"]
        .into_iter()
        .filter_map(|key| value.get(key).and_then(serde_json::Value::as_str))
        .map(str::trim)
        .find(|token| !token.is_empty())?
        .to_owned();
    Some(CursorAuth {
        user_id: cursor_user_id_from_token(&access_token),
        access_token,
    })
}

pub(crate) fn load_cursor_auth() -> Result<CursorAuth, String> {
    let path = cursor_auth_path();
    let value = read_json_file(&path)
        .ok_or_else(|| "Cursor auth.json is missing or unreadable".to_owned())?;
    cursor_auth_from_value(&value).ok_or_else(|| "Cursor access token is missing".to_owned())
}

/// Extract the Cursor user id from a JWT access token: payload `sub`, part
/// after `|`. `None` for opaque (non-JWT) tokens — REST enrichment then stays
/// unavailable rather than guessing an id.
pub(crate) fn cursor_user_id_from_token(token: &str) -> Option<String> {
    let payload = token.split('.').nth(1)?;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload.trim())
        .ok()?;
    let value: serde_json::Value = serde_json::from_slice(&decoded).ok()?;
    value
        .get("sub")
        .and_then(serde_json::Value::as_str)
        .and_then(|sub| sub.split('|').next_back())
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
}

/// Pure `cli-config.json` identity parse (`authInfo`): display label only,
/// never a credential. Shared by ambient loading and discovery so both read
/// the same keys.
pub(crate) fn cursor_cli_identity_from_value(value: &serde_json::Value) -> Option<String> {
    let info = value.get("authInfo")?;
    ["email", "displayName", "display_name", "userId"]
        .into_iter()
        .filter_map(|key| info.get(key).and_then(serde_json::Value::as_str))
        .map(str::trim)
        .find(|identity| !identity.is_empty())
        .map(str::to_owned)
}

/// Local CLI identity (`authInfo` in `cli-config.json`): display label only,
/// never a credential.
pub(crate) fn load_cursor_cli_identity() -> Option<String> {
    let path = env_value("CURSOR_CONFIG_DIR").map_or_else(
        || home_path(".cursor/cli-config.json"),
        |dir| PathBuf::from(dir).join("cli-config.json"),
    );
    let value = read_json_file(&path)?;
    cursor_cli_identity_from_value(&value)
}

/// Display label from one `cli-config.json` value (`authInfo`): email first,
/// then display name. Never a credential.
///
/// Same parse as [`cursor_cli_identity_from_value`]; both names are called
/// by `usage/cursor/tests.rs`, so they reconcile together.
pub(crate) fn cursor_identity_from_cli_config(value: &serde_json::Value) -> Option<String> {
    cursor_cli_identity_from_value(value)
}

// ---------------------------------------------------------------------------
// Personal: DashboardService Connect RPC
// ---------------------------------------------------------------------------

pub(crate) fn cursor_dashboard_base() -> String {
    env_value("CURSOR_API_ENDPOINT").unwrap_or_else(|| CURSOR_DEFAULT_DASHBOARD_BASE.to_owned())
}

/// True when enrichment-gated REST calls are allowed: OAuth-file auth against
/// the default base. Custom bases (and API-key auth) skip session enrichment.
pub(crate) fn cursor_default_base() -> bool {
    env_value("CURSOR_API_ENDPOINT").is_none()
}

/// Pure URL join for a dashboard base: the hermetic seam tests use so a live
/// `CURSOR_API_ENDPOINT` can never break (or leak into) assertions.
pub(crate) fn cursor_dashboard_url_with_base(base: &str, method: &str) -> String {
    format!(
        "{}/aiserver.v1.DashboardService/{method}",
        base.trim_end_matches('/')
    )
}

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

pub(crate) fn fetch_cursor_plan_info(base: &str, token: &str) -> Result<Option<String>, String> {
    let value = cursor_dashboard_post(base, token, "GetPlanInfo")?;
    Ok(parse_cursor_plan_info(&value))
}

pub(crate) fn parse_cursor_plan_info(value: &serde_json::Value) -> Option<String> {
    value
        .get("planName")
        .or_else(|| value.get("plan_name"))
        .or_else(|| value.get("plan"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|plan| !plan.is_empty())
        .map(humanize_plan_label)
}

/// Credit-grant balance in cents (explicit minor units → safe [`Money`]).
pub(crate) fn fetch_cursor_credit_grants(base: &str, token: &str) -> Result<i64, String> {
    let value = cursor_dashboard_post(base, token, "GetCreditGrantsBalance")?;
    Ok(parse_cursor_credit_grants(&value))
}

pub(crate) fn parse_cursor_credit_grants(value: &serde_json::Value) -> i64 {
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
pub(crate) fn cursor_credits_bucket(grant_cents: i64, stripe_cents: i64) -> QuotaBucketView {
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

/// Grok Bot weekly meter. Pooled enterprise allowance or zero allowance means
/// no meter (`None`) — never a 0% row.
#[derive(Debug, Clone)]
pub(crate) struct CursorSandUsage {
    pub(crate) usage_percent: f64,
    pub(crate) reset_at: Option<i64>,
}

pub(crate) fn fetch_cursor_sand_usage(
    base: &str,
    token: &str,
) -> Result<Option<CursorSandUsage>, String> {
    let value = cursor_dashboard_post(base, token, "GetSandUsageStatus")?;
    Ok(parse_cursor_sand_usage(&value))
}

pub(crate) fn parse_cursor_sand_usage(value: &serde_json::Value) -> Option<CursorSandUsage> {
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

pub(crate) fn cursor_sand_bucket(sand: &CursorSandUsage, now: i64) -> QuotaBucketView {
    #[expect(
        clippy::cast_sign_loss,
        reason = "filtered non-negative; clamped 0..=100"
    )]
    let remaining = Some(100u8.saturating_sub(sand.usage_percent.round().clamp(0.0, 100.0) as u8));
    timed_bucket(
        "Grok Bot",
        Some(format!("{}% used", sand.usage_percent)),
        Some("100%".to_owned()),
        remaining,
        sand.reset_at,
        now,
        None,
        UsageSnapshotStatus::Fresh,
    )
}

// ---------------------------------------------------------------------------
// Personal: cursor.com session REST
// ---------------------------------------------------------------------------

pub(crate) fn cursor_session_cookie(user_id: &str, token: &str) -> String {
    format!("WorkosCursorSessionToken={user_id}%3A%3A{token}")
}

pub(crate) fn cursor_rest_get(
    user_id: &str,
    token: &str,
    path: &str,
) -> Result<serde_json::Value, String> {
    let client = provider_http_client()?;
    let response = client
        .get(format!("{CURSOR_SESSION_BASE}{path}"))
        .header(
            reqwest::header::COOKIE,
            cursor_session_cookie(user_id, token),
        )
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .map_err(|error| format!("Cursor REST {path} request failed: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("Cursor REST {path} HTTP {status}"));
    }
    response
        .json::<serde_json::Value>()
        .map_err(|error| format!("Cursor REST {path} decode failed: {error}"))
}

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

/// Explicit Enterprise Admin credential. Never derived from a personal key: a
/// personal execution key does not imply reporting access.
// No `Debug`: this carries a live admin token and must never be formatted
// into a log or error.
#[derive(Clone)]
pub(crate) struct CursorEnterpriseScope {
    pub(crate) admin_token: String,
    pub(crate) team_id: Option<String>,
}

pub(crate) fn cursor_teams_spend_url() -> &'static str {
    "https://api.cursor.com/teams/spend"
}

pub(crate) fn cursor_teams_events_url() -> &'static str {
    "https://api.cursor.com/teams/filtered-usage-events"
}

/// One member's spend row: team and member scopes stay distinct buckets.
#[derive(Debug, Clone)]
pub(crate) struct CursorMemberSpend {
    pub(crate) label: String,
    pub(crate) charged: Option<f64>,
}

/// Team spend report: actual charged vs estimated model cost, kept separate.
#[derive(Debug, Clone)]
pub(crate) struct CursorTeamSpend {
    pub(crate) charged: Option<f64>,
    pub(crate) estimated: Option<f64>,
    pub(crate) period_end: Option<i64>,
    pub(crate) members: Vec<CursorMemberSpend>,
}

pub(crate) fn fetch_cursor_team_spend(
    scope: &CursorEnterpriseScope,
) -> Result<CursorTeamSpend, String> {
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

pub(crate) fn parse_cursor_team_spend(value: &serde_json::Value) -> Option<CursorTeamSpend> {
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

pub(crate) fn cursor_team_spend_buckets(spend: &CursorTeamSpend, now: i64) -> Vec<QuotaBucketView> {
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

/// Hourly-aggregated usage events. The overview snapshot never fetches these
/// (aggregation delay + hammering); the fetch exists for detail drill-down.
#[derive(Debug, Clone, Default)]
pub(crate) struct CursorUsageEvents {
    pub(crate) event_count: usize,
    pub(crate) total_tokens: Option<i64>,
    pub(crate) total_charged: Option<f64>,
    pub(crate) total_estimated: Option<f64>,
}

pub(crate) fn fetch_cursor_usage_events(
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

pub(crate) fn parse_cursor_usage_events(value: &serde_json::Value) -> CursorUsageEvents {
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

pub(crate) fn cursor_events_buckets(events: &CursorUsageEvents) -> Vec<QuotaBucketView> {
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

pub(crate) fn cursor_snapshot(agent: &str, provider: Option<&str>, now: i64) -> FocusedUsageView {
    let auth = match load_cursor_auth() {
        Ok(auth) => auth,
        Err(error) => {
            return cursor_status_view(
                agent,
                provider,
                now,
                UsageSnapshotStatus::NeedsSecret,
                &error,
            );
        }
    };
    cursor_snapshot_with_auth(
        agent,
        provider,
        &auth,
        load_cursor_cli_identity().as_deref(),
        "OAuth · ~/.cursor/auth.json",
        &cursor_dashboard_base(),
        now,
    )
}

/// Broker-refresh entry: `auth.json` at a registered profile root plus the
/// sibling `cli-config.json` identity. Never touches the default home.
pub(crate) fn cursor_profile_snapshot(agent: &str, auth_path: &Path, now: i64) -> FocusedUsageView {
    let auth = match read_json_file(auth_path)
        .ok_or_else(|| "Cursor auth.json is missing or unreadable".to_owned())
        .and_then(|value| {
            cursor_auth_from_value(&value)
                .ok_or_else(|| "Cursor access token is missing".to_owned())
        }) {
        Ok(auth) => auth,
        Err(error) => {
            return cursor_status_view(agent, None, now, UsageSnapshotStatus::NeedsSecret, &error);
        }
    };
    let identity = auth_path
        .parent()
        .and_then(|root| read_json_file(&root.join("cli-config.json")))
        .and_then(|value| cursor_cli_identity_from_value(&value));
    cursor_snapshot_with_auth(
        agent,
        None,
        &auth,
        identity.as_deref(),
        "OAuth · configured profile",
        &cursor_dashboard_base(),
        now,
    )
}

/// Personal snapshot from broker-minted material: the selected profile's
/// token, identity, and origin — never ambient files. The dashboard base is
/// explicit so hermetic tests can point the RPC at a dead port.
pub(crate) fn cursor_snapshot_with_auth(
    agent: &str,
    provider: Option<&str>,
    auth: &CursorAuth,
    identity: Option<&str>,
    credential_origin: &str,
    dashboard_base: &str,
    now: i64,
) -> FocusedUsageView {
    let token = auth.access_token.as_str();
    let (period, period_error) =
        split_fetch(Some(fetch_cursor_period_usage(dashboard_base, token)));
    let (plan, plan_error) = split_fetch(Some(fetch_cursor_plan_info(dashboard_base, token)));
    let (grants, grants_error) =
        split_fetch(Some(fetch_cursor_credit_grants(dashboard_base, token)));
    let (sand, sand_error) = split_fetch(Some(fetch_cursor_sand_usage(dashboard_base, token)));
    // Session-REST enrichment only for OAuth-file auth against the default base.
    let rest = auth.user_id.as_deref().filter(|_| cursor_default_base());
    let (summary, summary_error) =
        split_fetch(rest.map(|user| fetch_cursor_usage_summary(user, token)));
    let needs_requests = period.as_ref().is_none_or(cursor_needs_request_fallback);
    let (requests, requests_error) = split_fetch(
        rest.filter(|_| needs_requests)
            .map(|user| fetch_cursor_request_usage(user, token)),
    );
    let (stripe, stripe_error) =
        split_fetch(rest.map(|user| fetch_cursor_stripe_balance(user, token)));

    // Primary quota first (RPC wins; summary fills gaps, never duplicates the
    // same cycle meter), then requests, credits, and the Grok Bot meter.
    let mut buckets = Vec::new();
    if let Some(usage) = &period {
        buckets.extend(cursor_period_buckets(
            usage,
            summary.as_ref().and_then(|summary| summary.cycle_end),
            now,
        ));
    }
    if let Some(summary) = &summary {
        let has_cycle = buckets.iter().any(|bucket| bucket.label == "Billing cycle");
        buckets.extend(
            cursor_summary_buckets(summary, now)
                .into_iter()
                .filter(|bucket| bucket.label != "Billing cycle" || !has_cycle),
        );
    }
    if let Some(requests) = &requests {
        buckets.push(cursor_request_bucket(requests));
    }
    if grants.is_some() || stripe.is_some() {
        buckets.push(cursor_credits_bucket(
            grants.unwrap_or(0),
            stripe.unwrap_or(0),
        ));
    }
    if let Some(sand) = sand.as_ref().and_then(|sand| sand.as_ref()) {
        buckets.push(cursor_sand_bucket(sand, now));
    }
    if buckets.is_empty() {
        buckets.push(bucket(
            "Billing cycle",
            None,
            None,
            None,
            None,
            period_error
                .as_deref()
                .or(Some("Cursor dashboard unavailable")),
            UsageSnapshotStatus::Stale,
        ));
    }
    let status = if period.is_some() || summary.is_some() || requests.is_some() {
        UsageSnapshotStatus::Fresh
    } else {
        UsageSnapshotStatus::Stale
    };
    // Partial enrichment failures stay visible without erasing good quota.
    let mut failures = Vec::new();
    failures.extend(
        [
            period_error,
            plan_error,
            grants_error,
            sand_error,
            summary_error,
            requests_error,
            stripe_error,
        ]
        .into_iter()
        .flatten(),
    );
    let plan_label = plan.flatten().map(|plan| {
        if period.as_ref().is_some_and(|usage| usage.is_team) {
            format!("{plan} · Team")
        } else {
            plan
        }
    });
    let identity = identity.unwrap_or_default();
    usage_view(UsageViewInput {
        agent,
        provider: provider.or(Some("Cursor")),
        surface: UsageSurface::Cursor,
        account_label: identity.to_owned(),
        username: (!identity.is_empty()).then(|| identity.to_owned()),
        plan_label,
        credential_origin: Some(credential_origin.to_owned()),
        buckets,
        status,
        source: if status == UsageSnapshotStatus::Fresh {
            UsageSource::ProviderApi
        } else {
            UsageSource::None
        },
        confidence: if status == UsageSnapshotStatus::Fresh {
            UsageConfidence::Authoritative
        } else {
            UsageConfidence::None
        },
        now,
        last_error: (!failures.is_empty()).then(|| failures.join("; ")),
    })
}

pub(crate) fn cursor_enterprise_snapshot(
    agent: &str,
    provider: Option<&str>,
    scope: &CursorEnterpriseScope,
    now: i64,
) -> FocusedUsageView {
    // Usage events are hourly aggregated: the overview reads team spend only,
    // never the events endpoint.
    let (spend, spend_error) = split_fetch(Some(fetch_cursor_team_spend(scope)));
    let mut buckets = spend
        .as_ref()
        .map(|spend| cursor_team_spend_buckets(spend, now))
        .unwrap_or_default();
    if buckets.is_empty() {
        buckets.push(bucket(
            "Team spend (actual)",
            None,
            None,
            None,
            None,
            spend_error
                .as_deref()
                .or(Some("Cursor Admin API unavailable")),
            UsageSnapshotStatus::Stale,
        ));
    }
    let status = if spend.is_some() {
        UsageSnapshotStatus::Fresh
    } else {
        UsageSnapshotStatus::Stale
    };
    usage_view(UsageViewInput {
        agent,
        provider: provider.or(Some("Cursor")),
        surface: UsageSurface::Cursor,
        account_label: String::new(),
        username: None,
        plan_label: Some("Cursor Enterprise".to_owned()),
        credential_origin: Some("Admin API · explicit team scope".to_owned()),
        buckets,
        status,
        source: if status == UsageSnapshotStatus::Fresh {
            UsageSource::ProviderApi
        } else {
            UsageSource::None
        },
        confidence: if status == UsageSnapshotStatus::Fresh {
            UsageConfidence::Authoritative
        } else {
            UsageConfidence::None
        },
        now,
        last_error: match status {
            UsageSnapshotStatus::Fresh => None,
            _ => spend_error,
        },
    })
}

fn cursor_status_view(
    agent: &str,
    provider: Option<&str>,
    now: i64,
    status: UsageSnapshotStatus,
    error: &str,
) -> FocusedUsageView {
    usage_view(UsageViewInput {
        agent,
        provider: provider.or(Some("Cursor")),
        surface: UsageSurface::Cursor,
        account_label: String::new(),
        username: None,
        plan_label: None,
        credential_origin: Some("needs ~/.cursor/auth.json".to_owned()),
        buckets: vec![bucket(
            "Billing cycle",
            None,
            None,
            None,
            None,
            Some(error),
            status,
        )],
        status,
        source: UsageSource::None,
        confidence: UsageConfidence::None,
        now,
        last_error: Some(error.to_owned()),
    })
}

#[cfg(test)]
mod tests;
