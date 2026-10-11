// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Claude` OAuth usage response types.

use jackin_protocol::control::Money;
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Deserialize)]
pub struct ClaudeOAuthUsageResponse {
    #[serde(rename = "five_hour")]
    pub five_hour: Option<ClaudeOAuthUsageWindow>,
    // `seven_day` is the Weekly window. `seven_day_oauth_apps` is a SEPARATE
    // window the API also returns — it must NOT be aliased here (the API sends
    // both keys, so aliasing collides into a serde "duplicate field" and fails
    // the whole decode). It is not a CodexBar quota window, so it is ignored.
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
    // Authoritative shape for Session / "All models" Weekly / per-model Weekly
    // (Fable, and future model-scoped limits). The API migrated model-specific
    // windows here: the legacy `seven_day_sonnet`/`seven_day_opus` keys are
    // still returned but `null` on current accounts — the data lives only in
    // `limits` as `weekly_scoped` entries. Surfaced generically so a new model
    // codename (Fable today, others tomorrow) appears without per-model code.
    #[serde(default)]
    pub limits: Vec<ClaudeOAuthLimit>,
    #[serde(rename = "extra_usage")]
    pub extra_usage: Option<ClaudeOAuthExtraUsage>,
    // The newer, self-describing money object. Preferred over `extra_usage`
    // because it states the unit scale (`exponent`) and currency explicitly, so
    // a minor-unit amount can never be mis-scaled. `extra_usage` is kept as a
    // fallback for responses that predate `spend`.
    #[serde(rename = "spend")]
    pub spend: Option<ClaudeOAuthSpend>,
    // Catch-all for the remaining keys — chiefly the rotating-codename dollar
    // budget windows (`amber_ladder`, `omelette_promotional`, …). Capturing
    // them generically, rather than enumerating each ephemeral name, is what
    // lets enterprise dollar budgets surface instead of being silently dropped
    // by a fixed-field struct.
    #[serde(flatten)]
    pub other_windows: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct ClaudeOAuthSpend {
    pub used: Option<ClaudeOAuthMoney>,
    pub limit: Option<ClaudeOAuthMoney>,
    pub percent: Option<u8>,
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
    // Dollar-denominated budget windows (enterprise contractual allocations,
    // carried under rotating codename keys like `amber_ladder`). Named in
    // major-unit dollars by the API, so no `exponent` is supplied.
    #[serde(rename = "limit_dollars")]
    pub limit_dollars: Option<f64>,
    #[serde(rename = "used_dollars")]
    pub used_dollars: Option<f64>,
}

/// One entry in the `limits` array — the authoritative shape for Session,
/// "All models" Weekly, and per-model Weekly (Fable, and future model-scoped
/// limits). `percent` is already-scaled (0..=100); `kind` selects the bucket
/// (`session` | `weekly_all` | `weekly_scoped`); `scope.model.display_name`
/// labels a `weekly_scoped` window; `severity` mirrors the web console's meter
/// color and maps to [`UsageSeverity`]. The API also sends `group`, `is_active`,
/// `scope.surface`, and `model.id`, but those carry no rendering meaning today,
/// so they are intentionally not modeled — serde ignores unknown fields, and a
/// field is added back here only when something reads it (no dead fields).
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
    // Unit scale for `used_credits`/`monthly_limit`: they are MINOR units
    // (e.g. cents), so the major value is `value / 10^decimal_places`. Ignoring
    // this is what produced the 100×-too-large spend display.
    #[serde(rename = "decimal_places")]
    pub decimal_places: Option<u8>,
    #[serde(rename = "disabled_reason")]
    pub disabled_reason: Option<String>,
}
