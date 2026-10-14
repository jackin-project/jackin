// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use serde::Deserialize;

#[derive(Debug, Deserialize, Default)]
pub struct OpenRouterKeyData {
    #[serde(default)]
    pub(crate) usage: Option<f64>,
    #[serde(default)]
    pub(crate) usage_daily: Option<f64>,
    #[serde(default)]
    pub(crate) usage_weekly: Option<f64>,
    #[serde(default)]
    pub(crate) usage_monthly: Option<f64>,

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
pub struct OpenRouterCreditsResponse {
    pub(crate) data: OpenRouterCreditsData,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OpenRouterCreditsOutcome {
    Available {
        spent_cents: i64,
        ceiling_cents: i64,
    },
    ManagementScopeDenied,
    Unavailable(String),
}

impl OpenRouterCreditsOutcome {
    pub(crate) fn from_http_status(status: u16) -> Self {
        if status == 403 {
            Self::ManagementScopeDenied
        } else {
            Self::Unavailable(format!("OpenRouter credits HTTP {status}"))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenRouterModelCheck {
    Verified { model_id: String },
    Unverified { model_id: String, reason: String },
}
