// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Codex` usage response and window types.

use super::{
    CodexAdditionalRateLimit, CodexResetCredits, CodexRpcCredits, CodexRpcRateLimitWindow,
};
use jackin_protocol::control::StatusSlot;
use jackin_usage_provider_core::{json_number, window_minutes_label};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct CodexUsageResponse {
    #[serde(rename = "plan_type")]
    pub(crate) plan_type: Option<String>,
    #[serde(rename = "rate_limit")]
    pub(crate) rate_limit: Option<CodexRateLimitDetails>,
    pub(crate) credits: Option<CodexCreditDetails>,
    #[serde(rename = "additional_rate_limits")]
    pub(crate) additional_rate_limits: Option<Vec<CodexAdditionalRateLimit>>,
    #[serde(skip)]
    pub(crate) reset_credits: Option<CodexResetCredits>,
    #[serde(default)]
    pub(crate) individual_limit: Option<CodexIndividualLimit>,
    #[serde(default)]
    pub(crate) spend_control: Option<CodexSpendControl>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct CodexSpendControl {
    #[serde(default)]
    pub(crate) individual_limit: Option<CodexIndividualLimit>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct CodexIndividualLimit {
    pub(crate) limit: Option<serde_json::Value>,
    pub(crate) used: Option<serde_json::Value>,
    #[serde(rename = "remaining_percent")]
    pub(crate) remaining_percent: Option<u8>,
    #[serde(rename = "resets_at")]
    pub(crate) resets_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CodexRateLimitDetails {
    #[serde(rename = "primary_window")]
    pub(crate) primary_window: Option<CodexWindowSnapshot>,
    #[serde(rename = "secondary_window")]
    pub(crate) secondary_window: Option<CodexWindowSnapshot>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CodexWindowSnapshot {
    // Untyped so a float/string `used_percent` degrades to a used-less window
    // instead of failing the whole response decode (wham shape drift).
    #[serde(rename = "used_percent")]
    pub(crate) used_percent: Option<serde_json::Value>,
    #[serde(rename = "reset_at")]
    pub(crate) reset_at: Option<i64>,
    // Relative reset form the wham API sends instead of `reset_at` on some
    // windows (`reset_at (epoch s) | reset_after_seconds`); resolved against
    // the fetch time in `resets_at`.
    #[serde(rename = "reset_after_seconds")]
    pub(crate) reset_after_seconds: Option<i64>,
    #[serde(rename = "limit_window_seconds")]
    pub(crate) limit_window_seconds: Option<i64>,
    #[serde(skip)]
    pub(crate) window_duration_mins: Option<i64>,
}

impl CodexWindowSnapshot {
    pub(crate) fn from_rpc(window: CodexRpcRateLimitWindow) -> Self {
        Self {
            used_percent: window.used_percent.map(|used| {
                let bounded = used.round().clamp(0.0, 100.0);
                serde_json::Value::from(bounded.to_string().parse::<u8>().unwrap_or(0))
            }),
            reset_at: window.resets_at,
            reset_after_seconds: None,
            limit_window_seconds: None,
            window_duration_mins: window.window_duration_mins,
        }
    }

    /// Raw used percent, rounded but unclamped: over-cap readings (>100%)
    /// survive so the bucket can carry the raw figure (T02); only the bar
    /// geometry clamps. `None` when the server sent no usable number.
    pub(crate) fn used_percent_raw(&self) -> Option<f64> {
        let used = json_number(self.used_percent.as_ref()?)?.round();
        used.is_finite().then_some(used)
    }

    /// Used percent rounded and clamped to `0..=100`; `None` when the server
    /// sent no usable number (missing, float drift handled, strings parsed).
    pub(crate) fn used_percent_clamped(&self) -> Option<u8> {
        let used = self.used_percent_raw()?;
        #[expect(clippy::cast_sign_loss, reason = "clamped to 0.0..=100.0")]
        Some(used.clamp(0.0, 100.0) as u8)
    }

    /// Effective reset epoch: absolute `reset_at` wins, otherwise `now` plus
    /// the relative `reset_after_seconds` offset (negative offsets ignored).
    pub(crate) fn resets_at(&self, now: i64) -> Option<i64> {
        self.reset_at.or_else(|| {
            self.reset_after_seconds
                .filter(|offset| *offset >= 0)
                .map(|offset| now.saturating_add(offset))
        })
    }

    pub(crate) fn window_label(&self) -> Option<String> {
        let minutes = self
            .window_duration_mins
            .or_else(|| self.limit_window_seconds.map(|seconds| seconds / 60))?;
        window_minutes_label(minutes)
    }

    pub(crate) fn window_seconds(&self) -> Option<i64> {
        self.limit_window_seconds
            .or_else(|| self.window_duration_mins.map(|minutes| minutes * 60))
    }

    pub(crate) fn exact_status_slot(&self) -> Option<StatusSlot> {
        match self.window_seconds()? {
            300 => Some(StatusSlot::Session),
            seconds if seconds == 7 * 24 * 60 * 60 => Some(StatusSlot::Weekly),
            _ => None,
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct CodexCreditDetails {
    #[serde(rename = "has_credits")]
    pub(crate) has_credits: Option<bool>,
    pub(crate) unlimited: Option<bool>,
    pub(crate) balance: Option<serde_json::Value>,
}

impl CodexCreditDetails {
    pub(crate) fn from_rpc(credits: CodexRpcCredits) -> Self {
        Self {
            has_credits: Some(credits.has_credits),
            unlimited: Some(credits.unlimited),
            balance: credits.balance.map(serde_json::Value::String),
        }
    }
}
