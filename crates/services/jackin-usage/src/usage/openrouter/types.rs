// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `OpenRouter` key/credit response types and fetch outcomes.

use serde::Deserialize;

#[derive(Debug, Deserialize, Default)]
pub(crate) struct OpenRouterKeyData {
    #[serde(default)]
    pub(crate) usage: Option<f64>,
    #[serde(default)]
    pub(crate) usage_daily: Option<f64>,
    #[serde(default)]
    pub(crate) usage_weekly: Option<f64>,
    #[serde(default)]
    pub(crate) usage_monthly: Option<f64>,
    /// Null = no configured key cap (never infinite credit).
    #[serde(default)]
    pub(crate) limit: Option<f64>,
    #[serde(default)]
    pub(crate) limit_remaining: Option<f64>,
    #[serde(default)]
    pub(crate) is_free_tier: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterKeyResponse {
    data: OpenRouterKeyData,
}

#[derive(Debug, Deserialize)]
pub(crate) struct OpenRouterCreditsData {
    pub(crate) total_credits: f64,
    pub(crate) total_usage: f64,
}

#[derive(Debug, Deserialize)]
pub(crate) struct OpenRouterCreditsResponse {
    pub(crate) data: OpenRouterCreditsData,
}

/// Typed `/credits` outcome. A 403 means the ordinary key lacks the Management
/// scope — the `/key` rows still render.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum OpenRouterCreditsOutcome {
    Available {
        spent_cents: i64,
        ceiling_cents: i64,
    },
    ManagementScopeDenied,
    Unavailable(String),
}

impl OpenRouterCreditsOutcome {
    /// Classify a `/credits` HTTP failure without string sniffing at the call
    /// site: 403 is the management-scope mismatch, anything else stays an
    /// opaque unavailable state.
    pub(crate) fn from_http_status(status: u16) -> Self {
        if status == 403 {
            Self::ManagementScopeDenied
        } else {
            Self::Unavailable(format!("OpenRouter credits HTTP {status}"))
        }
    }
}

/// Model-ID check against a catalog snapshot. Stale omission is `Unverified`,
/// never a rejection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OpenRouterModelCheck {
    Verified { model_id: String },
    Unverified { model_id: String, reason: String },
}
