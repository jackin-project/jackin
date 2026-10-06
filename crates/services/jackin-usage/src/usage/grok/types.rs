// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Grok` billing response types.

use super::grok_cycle_label_from_minutes;
use serde::Deserialize;

/// Billing-auth precedence: stored subscription auth outranks ambient inference
/// keys. An `XAI_API_KEY` / deployment key alone is explicitly not billing
/// auth — the REST call never sends it, so env-key-only accounts report an
/// honest billing gap instead of a failed request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GrokBillingAuth {
    Subscription,
    EnvKeyOnly,
    None,
}

pub(crate) fn resolve_grok_billing_auth(
    has_auth: bool,
    has_xai_api_key: bool,
    has_deployment_key: bool,
) -> GrokBillingAuth {
    if has_auth {
        GrokBillingAuth::Subscription
    } else if has_xai_api_key || has_deployment_key {
        GrokBillingAuth::EnvKeyOnly
    } else {
        GrokBillingAuth::None
    }
}

/// Current ACP `x.ai/billing` response (top-level `config` + resolved tier).
#[derive(Debug, Deserialize)]
pub(crate) struct GrokBillingResponse {
    pub(crate) config: Option<GrokBillingConfig>,
    pub(crate) on_demand_enabled: Option<bool>,
    /// Server-resolved subscription tier (already `display.or(machine)` upstream).
    pub(crate) subscription_tier: Option<String>,
}

/// The one `config` object: preferred `creditUsagePercent`/`currentPeriod`,
/// current fallback `monthlyLimit`/`used`/`billingPeriod*`, and confirmed
/// prepaid/on-demand quota bounds.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GrokBillingConfig {
    pub(crate) credit_usage_percent: Option<f64>,
    pub(crate) current_period: Option<GrokCurrentPeriod>,
    pub(crate) monthly_limit: Option<GrokCent>,
    pub(crate) used: Option<GrokCent>,
    pub(crate) on_demand_cap: Option<GrokCent>,
    pub(crate) on_demand_used: Option<GrokCent>,
    pub(crate) prepaid_balance: Option<GrokCent>,
    pub(crate) billing_period_start: Option<String>,
    pub(crate) billing_period_end: Option<String>,
    /// Unified-billing monthly quota aliases (`periodStart`/`periodEnd`) from
    /// the plain `{proxy}/billing` response.
    pub(crate) period_start: Option<String>,
    pub(crate) period_end: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GrokCurrentPeriod {
    #[serde(rename = "type")]
    pub(crate) period_type: Option<String>,
    pub(crate) start: Option<String>,
    pub(crate) end: Option<String>,
}

/// Proto3 JSON cents: an omitted `{}` object means zero.
#[derive(Debug, Deserialize)]
pub(crate) struct GrokCent {
    #[serde(default)]
    pub(crate) val: i64,
}

/// Positive cent value as-is. Zero/negative magnitudes are invalid for
/// limits and caps, and a negative prepaid is a deficit, not a credit —
/// never mirrored into positive bounds. Every monetary quota field passes
/// through this before comparison/label/money.
pub(crate) fn positive_cent_value(val: i64) -> Option<i64> {
    (val > 0).then_some(val)
}

/// Period label: the server period type wins, then window duration, then Credits.
pub(crate) fn grok_period_label(period_type: Option<&str>, window_seconds: i64) -> &'static str {
    match period_type {
        Some(kind) if kind.contains("WEEKLY") => "Weekly",
        Some(kind) if kind.contains("MONTHLY") => "Monthly",
        _ => grok_cycle_label_from_minutes(window_seconds / 60),
    }
}

#[derive(Debug)]
pub(crate) enum GrokBillingSnapshot {
    // Boxed: the current `config`-bearing RPC response is far larger than Web.
    Rpc(Box<GrokBillingResponse>),
    Rest(Box<GrokBillingResponse>),
    Web(GrokWebBillingSnapshot),
}

#[derive(Debug)]
pub(crate) struct GrokWebBillingSnapshot {
    pub(crate) used_percent: f64,
    pub(crate) reset_at_epoch: Option<i64>,
}
