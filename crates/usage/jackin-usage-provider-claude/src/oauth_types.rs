// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::control::Money;
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Deserialize)]
pub struct ClaudeOAuthUsageResponse {
    #[serde(rename = "five_hour")]
    pub five_hour: Option<ClaudeOAuthUsageWindow>,
    #[serde(rename = "seven_day")]
    pub seven_day: Option<ClaudeOAuthUsageWindow>,
    #[serde(rename = "seven_day_sonnet")]
    pub seven_day_sonnet: Option<ClaudeOAuthUsageWindow>,
    #[serde(rename = "seven_day_opus")]
    pub seven_day_opus: Option<ClaudeOAuthUsageWindow>,
    #[serde(alias = "seven_day_claude_routines")]
    #[serde(alias = "claude_routines")]
    #[serde(alias = "routines")]
    #[serde(alias = "seven_day_cowork")]
    #[serde(rename = "seven_day_routines")]
    pub seven_day_routines: Option<ClaudeOAuthUsageWindow>,
    #[serde(default)]
    pub limits: Vec<ClaudeOAuthLimit>,
    #[serde(rename = "extra_usage")]
    pub extra_usage: Option<ClaudeOAuthExtraUsage>,
    #[serde(rename = "spend")]
    pub spend: Option<ClaudeOAuthSpend>,
    #[serde(flatten)]
    pub other_windows: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct ClaudeOAuthSpend {
    pub used: Option<ClaudeOAuthMoney>,
    pub limit: Option<ClaudeOAuthMoney>,
    pub percent: Option<f64>,
    pub severity: Option<String>,
    pub enabled: Option<bool>,
    #[serde(rename = "disabled_reason")]
    pub disabled_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ClaudeOAuthMoney {
    #[serde(rename = "amount_minor")]
    pub amount_minor: Option<i64>,
    pub currency: Option<String>,
    pub exponent: Option<u8>,
}

impl ClaudeOAuthMoney {
    pub fn into_money(self) -> Option<Money> {
        Some(Money::new(
            self.amount_minor?,
            self.currency.unwrap_or_else(|| "credits".to_owned()),
            self.exponent.unwrap_or(2),
        ))
    }
}

#[derive(Debug, Deserialize)]
pub struct ClaudeOAuthUsageWindow {
    pub utilization: Option<f64>,
    #[serde(rename = "resets_at")]
    pub resets_at: Option<String>,
    #[serde(rename = "limit_dollars")]
    pub limit_dollars: Option<f64>,
    #[serde(rename = "used_dollars")]
    pub used_dollars: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct ClaudeOAuthLimit {
    pub kind: Option<String>,
    pub percent: Option<serde_json::Value>,
    pub severity: Option<String>,
    #[serde(rename = "resets_at")]
    pub resets_at: Option<String>,
    pub scope: Option<ClaudeOAuthLimitScope>,
}

#[derive(Debug, Deserialize)]
pub struct ClaudeOAuthLimitScope {
    pub model: Option<ClaudeOAuthLimitModel>,
}

#[derive(Debug, Deserialize)]
pub struct ClaudeOAuthLimitModel {
    #[serde(rename = "display_name")]
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ClaudeOAuthExtraUsage {
    #[serde(rename = "is_enabled")]
    pub is_enabled: Option<bool>,
    #[serde(rename = "monthly_limit")]
    pub monthly_limit: Option<f64>,
    #[serde(rename = "used_credits")]
    pub used_credits: Option<f64>,

    pub utilization: Option<f64>,
    pub currency: Option<String>,
    #[serde(rename = "decimal_places")]
    pub decimal_places: Option<u8>,
    #[serde(rename = "disabled_reason")]
    pub disabled_reason: Option<String>,
}
