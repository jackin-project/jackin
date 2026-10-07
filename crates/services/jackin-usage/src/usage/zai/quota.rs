// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Z.AI` quota response types and plan windows.

use jackin_protocol::control::{QuotaBucketView, StatusSlot};
use jackin_usage_provider_core::with_status_slot;

use super::zai_bucket;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct ZaiQuotaResponse {
    pub(crate) code: Option<i64>,
    pub(crate) msg: Option<String>,
    pub(crate) success: Option<bool>,
    pub(crate) data: Option<ZaiQuotaData>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ZaiQuotaData {
    #[serde(default)]
    pub(crate) limits: Vec<ZaiLimitRaw>,
    #[serde(
        rename = "planName",
        alias = "plan",
        alias = "plan_type",
        alias = "packageName"
    )]
    pub(crate) plan_name: Option<String>,
    // Separate field, not another `plan_name` alias: serde raises a
    // duplicate-field error if two aliased keys co-occur, which would fail
    // the whole parse when a response carries both `planName` and `level`.
    pub(crate) level: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ZaiLimitRaw {
    #[serde(rename = "type")]
    pub(crate) limit_type: String,
    pub(crate) unit: Option<i64>,
    pub(crate) number: Option<i64>,
    pub(crate) usage: Option<i64>,
    #[serde(rename = "currentValue")]
    pub(crate) current_value: Option<i64>,
    pub(crate) remaining: Option<i64>,
    pub(crate) percentage: Option<f64>,
    #[serde(rename = "nextResetTime")]
    pub(crate) next_reset_time: Option<i64>,
    #[serde(rename = "usageDetails", default)]
    pub(crate) usage_details: Vec<ZaiUsageDetail>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ZaiUsageDetail {
    #[serde(rename = "modelCode", alias = "model_code", alias = "model")]
    pub(crate) model_code: Option<String>,
    pub(crate) usage: Option<i64>,
}

impl ZaiQuotaResponse {
    pub(crate) fn buckets(&self, now: i64) -> Vec<QuotaBucketView> {
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

    pub(crate) fn plan_name(&self) -> Option<String> {
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
    pub(crate) fn used_percent(&self) -> Option<u8> {
        if let Some(limit) = self.usage.filter(|limit| *limit > 0) {
            let used = if let Some(remaining) = self.remaining {
                let from_remaining = limit.saturating_sub(remaining);
                self.current_value
                    .map_or(from_remaining, |current| from_remaining.max(current))
            } else {
                self.current_value?
            };
            #[expect(clippy::cast_sign_loss, reason = "clamped to 0.0..=100.0 above")]
            let percent = ((used.clamp(0, limit) as f64 / limit as f64) * 100.0)
                .round()
                .clamp(0.0, 100.0) as u8;
            return Some(percent);
        }
        self.percentage.map(|percent| {
            #[expect(clippy::cast_sign_loss, reason = "clamped to 0.0..=100.0 above")]
            {
                percent.round().clamp(0.0, 100.0) as u8
            }
        })
    }

    /// Window length in minutes from the `(unit, number)` period pair:
    /// `1` = day, `3` = hour, `5` = minute, `6` = week. Unknown unit codes
    /// yield no window (never a guessed duration).
    pub(crate) fn window_minutes(&self) -> Option<i64> {
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

    /// `TIME_LIMIT` covers both short tool/MCP quotas and the monthly
    /// web-search count; the window tells them apart. The ≥28d `Web search`
    /// split is a window-size heuristic with a known residual risk: a 28d+
    /// MCP window would mislabel. No sharper signal exists in the quota
    /// payload, so the guess stays, documented.
    pub(crate) fn time_label(&self) -> &'static str {
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
