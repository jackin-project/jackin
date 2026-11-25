// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use chrono::NaiveDateTime;
use jackin_usage_provider_core::{Auth, Http, ProviderError};
use serde::Deserialize;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Region {
    China,
    Singapore,
    Europe,
}

impl Region {
    pub const fn token_plan_url(self) -> &'static str {
        match self {
            Self::China => "https://token-plan-cn.xiaomimimo.com/v1/platform/token_plan",
            Self::Singapore => "https://token-plan-sgp.xiaomimimo.com/v1/platform/token_plan",
            Self::Europe => "https://token-plan-ams.xiaomimimo.com/v1/platform/token_plan",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenPlan {
    pub plan_code: String,
    pub starts_at: i64,
    pub ends_at: i64,
    pub quota: CreditQuota,
    pub auto_renew: Option<bool>,
    pub claw_enabled: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub struct CreditQuota {
    pub total_credits: u64,
    pub used_credits: u64,
    pub remaining_credits: u64,
}

#[derive(Deserialize)]
struct Response {
    object: String,
    plan_code: String,
    period: Period,
    quota: CreditQuota,
    auto_renew: Option<bool>,
    claw_enabled: Option<bool>,
}

#[derive(Deserialize)]
struct Period {
    start_at: String,
    end_at: String,
}

fn utc_epoch(value: &str) -> Result<i64, ProviderError> {
    NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S")
        .map(|time| time.and_utc().timestamp())
        .map_err(|_| ProviderError::Decode("invalid MiMo UTC billing period"))
}

pub async fn fetch_token_plan(
    http: &Http,
    region: Region,
    key: &str,
) -> Result<TokenPlan, ProviderError> {
    if !key.starts_with("tp-") || key.len() == 3 {
        return Err(ProviderError::Unsupported(
            "MiMo Token Plan requires a tp- key",
        ));
    }
    let body = http
        .get(region.token_plan_url(), Auth::Bearer(key), &[])
        .await?;
    let response: Response = serde_json::from_value(body)
        .map_err(|_| ProviderError::Decode("invalid MiMo Token Plan response"))?;
    if response.object != "token_plan" || response.plan_code.trim().is_empty() {
        return Err(ProviderError::Decode("MiMo did not return a Token Plan"));
    }
    let starts_at = utc_epoch(&response.period.start_at)?;
    let ends_at = utc_epoch(&response.period.end_at)?;
    if starts_at >= ends_at {
        return Err(ProviderError::Decode("invalid MiMo billing period bounds"));
    }
    Ok(TokenPlan {
        plan_code: response.plan_code,
        starts_at,
        ends_at,
        quota: response.quota,
        auto_renew: response.auto_renew,
        claw_enabled: response.claw_enabled,
    })
}

mod balance;
mod history;
pub use balance::*;
pub use history::{DailySource, fetch_daily};
