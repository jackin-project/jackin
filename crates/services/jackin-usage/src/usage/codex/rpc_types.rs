// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Codex` RPC response types.

use super::{
    CodexCreditDetails, CodexRateLimitDetails, CodexResetCredits, CodexUsageResponse,
    CodexWindowSnapshot,
};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Deserialize)]
pub(crate) struct CodexAdditionalRateLimit {
    #[serde(rename = "limit_name")]
    pub(crate) limit_name: Option<String>,
    #[serde(rename = "metered_feature")]
    pub(crate) metered_feature: Option<String>,
    #[serde(rename = "rate_limit")]
    pub(crate) rate_limit: Option<CodexRateLimitDetails>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CodexRpcAccountResponse {
    pub(crate) account: Option<CodexRpcAccountDetails>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub(crate) enum CodexRpcAccountDetails {
    #[serde(rename = "apiKey")]
    ApiKey,
    #[serde(rename = "chatgpt")]
    Chatgpt {
        email: Option<String>,
        #[serde(rename = "planType")]
        plan_type: Option<String>,
    },
    #[serde(rename = "amazonBedrock")]
    AmazonBedrock,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CodexRpcRateLimitsResponse {
    // Defaulted: a server that omits the whole object (permission/capability
    // drift) still decodes, yielding no windows instead of no snapshot.
    #[serde(rename = "rateLimits", default)]
    pub(crate) rate_limits: CodexRpcRateLimits,
    // Per-limit-id windows. Every entry other than the main "codex" limit
    // (already surfaced as Session/Weekly) is an extra limit — the
    // "…Codex-Spark" entry carries the Codex Spark 5-hour/Weekly windows.
    #[serde(rename = "rateLimitsByLimitId", default)]
    pub(crate) rate_limits_by_limit_id: BTreeMap<String, CodexRpcLimitEntry>,
    #[serde(rename = "rateLimitResetCredits")]
    pub(crate) reset_credits: Option<CodexRpcResetCredits>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CodexRpcLimitEntry {
    #[serde(rename = "limitId")]
    pub(crate) limit_id: Option<String>,
    #[serde(rename = "limitName")]
    pub(crate) limit_name: Option<String>,
    pub(crate) primary: Option<CodexRpcRateLimitWindow>,
    pub(crate) secondary: Option<CodexRpcRateLimitWindow>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CodexRpcResetCredits {
    // Defaulted: a missing count reads as zero (no bucket) rather than failing
    // the whole rate-limit decode.
    #[serde(rename = "availableCount", default)]
    pub(crate) available_count: i64,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct CodexRpcRateLimits {
    pub(crate) primary: Option<CodexRpcRateLimitWindow>,
    pub(crate) secondary: Option<CodexRpcRateLimitWindow>,
    pub(crate) credits: Option<CodexRpcCredits>,
    #[serde(rename = "planType")]
    pub(crate) plan_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CodexRpcRateLimitWindow {
    // Optional: a window without a used figure still decodes (reset/duration
    // rows stay); the bucket just carries no used/remaining percent.
    #[serde(rename = "usedPercent")]
    pub(crate) used_percent: Option<f64>,
    #[serde(rename = "windowDurationMins")]
    pub(crate) window_duration_mins: Option<i64>,
    #[serde(rename = "resetsAt")]
    pub(crate) resets_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CodexRpcCredits {
    // Defaulted: a credits object with drifted/missing flags reads as
    // no-credits (no bucket) rather than failing the whole decode.
    #[serde(rename = "hasCredits", default)]
    pub(crate) has_credits: bool,
    #[serde(default)]
    pub(crate) unlimited: bool,
    pub(crate) balance: Option<String>,
}

pub(crate) struct CodexRpcUsage {
    pub(crate) response: CodexUsageResponse,
    pub(crate) account_label: Option<String>,
}

impl CodexRpcUsage {
    pub(crate) fn from_rpc(
        limits: CodexRpcRateLimitsResponse,
        account: Option<CodexRpcAccountResponse>,
    ) -> Self {
        let account_details = account.and_then(|response| response.account);
        let account_label = match &account_details {
            Some(CodexRpcAccountDetails::Chatgpt { email, .. }) => email.clone(),
            Some(CodexRpcAccountDetails::ApiKey) => Some("Codex API key".to_owned()),
            Some(CodexRpcAccountDetails::AmazonBedrock) | None => None,
        };
        let account_plan = match account_details {
            Some(CodexRpcAccountDetails::Chatgpt { plan_type, .. }) => plan_type,
            _ => None,
        };
        let CodexRpcRateLimitsResponse {
            rate_limits,
            rate_limits_by_limit_id,
            reset_credits: rpc_reset_credits,
        } = limits;
        // Every per-limit-id entry except the main "codex" limit is an extra
        // rate limit (e.g. Codex Spark); its primary/secondary become the
        // "<label> 5-hour"/"Weekly" buckets in `buckets()`.
        let additional_rate_limits: Vec<CodexAdditionalRateLimit> = rate_limits_by_limit_id
            .into_values()
            .filter(|entry| entry.limit_id.as_deref() != Some("codex"))
            .filter_map(|entry| {
                let primary = entry.primary.map(CodexWindowSnapshot::from_rpc);
                let secondary = entry.secondary.map(CodexWindowSnapshot::from_rpc);
                if primary.is_none() && secondary.is_none() {
                    return None;
                }
                Some(CodexAdditionalRateLimit {
                    limit_name: entry.limit_name,
                    metered_feature: None,
                    rate_limit: Some(CodexRateLimitDetails {
                        primary_window: primary,
                        secondary_window: secondary,
                    }),
                })
            })
            .collect();
        let reset_credits = rpc_reset_credits
            .filter(|reset| reset.available_count > 0)
            .map(|reset| CodexResetCredits {
                credits: Vec::new(),
                available_count: reset.available_count,
            });
        let response = CodexUsageResponse {
            plan_type: account_plan.or(rate_limits.plan_type),
            rate_limit: Some(CodexRateLimitDetails {
                primary_window: rate_limits.primary.map(CodexWindowSnapshot::from_rpc),
                secondary_window: rate_limits.secondary.map(CodexWindowSnapshot::from_rpc),
            }),
            credits: rate_limits.credits.map(CodexCreditDetails::from_rpc),
            additional_rate_limits: (!additional_rate_limits.is_empty())
                .then_some(additional_rate_limits),
            reset_credits,
            individual_limit: None,
            spend_control: None,
        };
        Self {
            response,
            account_label,
        }
    }
}
