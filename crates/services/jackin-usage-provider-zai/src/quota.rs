// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::control::{QuotaBucketView, StatusSlot};
use jackin_usage_provider_core::with_status_slot;

use super::zai_bucket;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ZaiQuotaResponse {
    pub code: Option<i64>,
    pub msg: Option<String>,
    pub success: Option<bool>,
    pub data: Option<ZaiQuotaData>,
}

#[derive(Debug, Deserialize)]
pub struct ZaiQuotaData {
    #[serde(default)]
    pub limits: Vec<ZaiLimitRaw>,
    #[serde(
        rename = "planName",
        alias = "plan",
        alias = "plan_type",
        alias = "packageName"
    )]
    pub plan_name: Option<String>,
    pub level: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ZaiLimitRaw {
    #[serde(rename = "type")]
    pub limit_type: String,
    pub unit: Option<i64>,
    pub number: Option<i64>,
    pub usage: Option<serde_json::Value>,
    #[serde(rename = "currentValue")]
    pub current_value: Option<serde_json::Value>,
    pub remaining: Option<serde_json::Value>,
    pub percentage: Option<f64>,
    #[serde(rename = "nextResetTime")]
    pub next_reset_time: Option<i64>,
    #[serde(rename = "usageDetails", default)]
    pub usage_details: Vec<ZaiUsageDetail>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ZaiUsageDetail {
    #[serde(rename = "modelCode", alias = "model_code", alias = "model")]
    pub model_code: Option<String>,
    pub usage: Option<serde_json::Value>,
}

impl ZaiQuotaResponse {
    pub fn buckets(&self, now: i64) -> Vec<QuotaBucketView> {
        let limits = self
            .data
            .as_ref()
            .map(|data| data.limits.clone())
            .unwrap_or_default();
        let mut buckets = Vec::new();
        for limit in &limits {
            let Some(slot) = limit.semantic_slot() else {
                if limit.limit_type == "TIME_LIMIT" {
                    buckets.push(zai_bucket(limit.time_label(), limit, now));
                }
                continue;
            };
            let label = match slot {
                StatusSlot::Session => "Session",
                StatusSlot::Weekly => "Weekly",
                _ => continue,
            };
            buckets.push(with_status_slot(zai_bucket(label, limit, now), Some(slot)));
        }
        buckets
    }

    pub fn plan_name(&self) -> Option<String> {
        let data = self.data.as_ref()?;
        data.plan_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or_else(|| {
                data.level
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
            })
            .map(str::to_owned)
    }
}

impl ZaiLimitRaw {
    pub fn used_percent(&self) -> Option<f64> {
        self.percentage
            .filter(|percent| percent.is_finite() && *percent >= 0.0)
    }

    pub fn window_minutes(&self) -> Option<i64> {
        let number = self.number?;
        if number <= 0 {
            return None;
        }
        match self.unit {
            Some(5) => Some(number),
            Some(3) => Some(number * 60),
            Some(1) => Some(number * 24 * 60),
            Some(6) => Some(number * 7 * 24 * 60),
            _ => None,
        }
    }

    pub fn time_label(&self) -> &'static str {
        if self
            .window_minutes()
            .is_some_and(|minutes| minutes >= 28 * 24 * 60)
        {
            "Web search"
        } else {
            "MCP"
        }
    }

    fn semantic_slot(&self) -> Option<StatusSlot> {
        if !matches!(self.limit_type.as_str(), "TOKENS_LIMIT" | "CREDIT_LIMIT") {
            return None;
        }
        match self.window_minutes()? {
            minutes if minutes < 24 * 60 => Some(StatusSlot::Session),
            minutes if minutes >= 3 * 24 * 60 => Some(StatusSlot::Weekly),
            _ => None,
        }
    }
}
