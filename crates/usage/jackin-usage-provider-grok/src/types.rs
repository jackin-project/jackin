// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::grok_cycle_label_from_minutes;
use jackin_protocol::control::CanonicalUsedPercent;
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrokBillingAuth {
    Subscription,
    EnvKeyOnly,
    None,
}

pub fn resolve_grok_billing_auth(
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

#[derive(Debug, Deserialize)]
pub struct GrokBillingResponse {
    pub(crate) config: Option<GrokBillingConfig>,
    pub(crate) on_demand_enabled: Option<bool>,
    pub(crate) subscription_tier: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrokBillingConfig {
    #[serde(default, deserialize_with = "deserialize_present_json_value")]
    pub(crate) credit_usage_percent: Option<serde_json::Value>,
    pub(crate) current_period: Option<GrokCurrentPeriod>,
    pub(crate) monthly_limit: Option<GrokCent>,
    pub(crate) used: Option<GrokCent>,
    pub(crate) on_demand_cap: Option<GrokCent>,
    pub(crate) on_demand_used: Option<GrokCent>,
    pub(crate) prepaid_balance: Option<GrokCent>,
    pub(crate) billing_period_start: Option<String>,
    pub(crate) billing_period_end: Option<String>,
    pub(crate) period_start: Option<String>,
    pub(crate) period_end: Option<String>,
}

fn deserialize_present_json_value<'de, D>(
    deserializer: D,
) -> Result<Option<serde_json::Value>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    serde_json::Value::deserialize(deserializer).map(Some)
}

impl GrokBillingConfig {
    pub(crate) fn credit_usage_percent_value(&self) -> Result<Option<CanonicalUsedPercent>, ()> {
        let Some(value) = &self.credit_usage_percent else {
            return Ok(None);
        };
        let Some(percent) = value.as_f64() else {
            return Err(());
        };
        CanonicalUsedPercent::new(percent).map(Some).map_err(|_| ())
    }
}

#[derive(Debug, Deserialize)]
pub struct GrokCurrentPeriod {
    #[serde(rename = "type")]
    pub(crate) period_type: Option<String>,
    pub(crate) start: Option<String>,
    pub(crate) end: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GrokCent {
    #[serde(default)]
    pub(crate) val: i64,
}

pub fn positive_cent_value(val: i64) -> Option<i64> {
    (val > 0).then_some(val)
}

pub fn grok_period_label(period_type: Option<&str>, window_seconds: i64) -> &'static str {
    match period_type {
        Some(kind) if kind.contains("WEEKLY") => "Weekly",
        Some(kind) if kind.contains("MONTHLY") => "Monthly",
        _ => grok_cycle_label_from_minutes(window_seconds / 60),
    }
}

#[derive(Debug)]
pub enum GrokBillingSnapshot {
    Rpc(Box<GrokBillingResponse>),
    Rest(Box<GrokBillingResponse>),
    Web(GrokWebBillingSnapshot),
}

#[derive(Debug)]
pub struct GrokWebBillingSnapshot {
    pub used_percent: CanonicalUsedPercent,
    pub reset_at_epoch: Option<i64>,
}
