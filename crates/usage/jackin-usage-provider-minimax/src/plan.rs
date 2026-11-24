// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_usage_provider_core::{Auth, Http, ProviderError, json_integer, json_number, json_text};
use serde_json::Value;

use crate::PAY_AS_YOU_GO_REASON;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Region {
    China,
    Global,
}

impl Region {
    pub const fn plan_url(self) -> &'static str {
        match self {
            Self::China => "https://api.minimax.cn/v1/token_plan/remains",
            Self::Global => "https://api.minimax.io/v1/token_plan/remains",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelQuota {
    pub model_name: String,
    pub interval: QuotaWindow,
    pub weekly: QuotaWindow,
    pub weekly_boost_permille: Option<i64>,
    pub excluded_from_plan: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QuotaWindow {
    pub status: Option<i64>,
    pub remaining_percent: Option<f64>,
    pub total: Option<i64>,
    pub remaining_count: Option<i64>,
    pub starts_at_ms: Option<i64>,
    pub ends_at_ms: Option<i64>,
    pub remaining_ms: Option<i64>,
}

pub async fn fetch_plan(
    http: &Http,
    region: Region,
    key: &str,
) -> Result<Vec<ModelQuota>, ProviderError> {
    if key.starts_with(super::PAY_AS_YOU_GO_PREFIX) {
        return Err(ProviderError::Unsupported(PAY_AS_YOU_GO_REASON));
    }
    fetch_oauth_plan(http, region, key).await
}

pub async fn fetch_oauth_plan(
    http: &Http,
    region: Region,
    token: &str,
) -> Result<Vec<ModelQuota>, ProviderError> {
    let body = http
        .get(region.plan_url(), Auth::Bearer(token), &[])
        .await?;
    if body
        .pointer("/base_resp/status_code")
        .and_then(json_integer)
        .is_some_and(|code| code != 0)
    {
        return Err(ProviderError::Decode(
            "MiniMax rejected the Token Plan request",
        ));
    }
    let models = body
        .get("model_remains")
        .and_then(Value::as_array)
        .ok_or(ProviderError::Decode(
            "MiniMax did not return Token Plan quotas",
        ))?
        .iter()
        .map(model_quota)
        .collect::<Result<Vec<_>, _>>()?;
    if models.is_empty() {
        return Err(ProviderError::Decode(
            "MiniMax did not return Token Plan quotas",
        ));
    }
    Ok(models)
}

fn model_quota(model: &Value) -> Result<ModelQuota, ProviderError> {
    let name = model
        .get("model_name")
        .and_then(json_text)
        .ok_or(ProviderError::Decode("MiniMax quota has no model name"))?;
    let interval = parse_window(model, "interval", "")?;
    let weekly = parse_window(model, "weekly", "weekly_")?;
    let excluded_from_plan = interval.status == Some(3)
        && weekly.status == Some(3)
        && interval.total == Some(0)
        && weekly.total == Some(0);
    Ok(ModelQuota {
        model_name: name.to_owned(),
        interval,
        weekly,
        excluded_from_plan,
        weekly_boost_permille: model.get("weekly_boost_permille").and_then(json_integer),
    })
}

fn parse_window(
    model: &Value,
    prefix: &str,
    time_prefix: &str,
) -> Result<QuotaWindow, ProviderError> {
    let field = |key: &str| model.get(format!("current_{prefix}_{key}"));
    let time = |key: &str| {
        model
            .get(format!("{time_prefix}{key}"))
            .and_then(json_integer)
    };
    let status = field("status").and_then(json_integer);
    let remaining_percent = field("remaining_percent").and_then(json_number);
    if remaining_percent.is_some_and(|p| !(0.0..=100.0).contains(&p)) {
        return Err(ProviderError::OUT_OF_RANGE);
    }
    let total = field("total_count").and_then(json_integer);
    let remaining_count = resolve_remaining(
        field("usage_count").and_then(json_integer),
        total,
        remaining_percent,
    );
    Ok(QuotaWindow {
        status,
        remaining_percent,
        total,
        remaining_count,
        starts_at_ms: time("start_time"),
        ends_at_ms: time("end_time"),
        remaining_ms: time("remains_time"),
    })
}

fn resolve_remaining(
    reported: Option<i64>,
    total: Option<i64>,
    percent: Option<f64>,
) -> Option<i64> {
    let (n, total) = (reported?, total?);
    if total <= 0 || !(0..=total).contains(&n) {
        return None;
    }
    let Some(percent) = percent else {
        return Some(n);
    };
    let observed = 100.0 * n as f64 / total as f64;
    let left_error = (observed - percent).abs();
    let used_error = (100.0 - observed - percent).abs();
    if left_error.min(used_error) > 1.0 {
        return None;
    }
    Some(if used_error < left_error {
        total - n
    } else {
        n
    })
}
