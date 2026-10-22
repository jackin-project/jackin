// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::control::CanonicalUsedPercent;
use jackin_usage_provider_core::{epoch_seconds_from_maybe_ms, parse_iso_epoch};
use serde::{Deserialize, de::Error as _};

#[derive(Debug, Deserialize)]
pub struct KimiUsageResponse {
    #[serde(default)]
    pub(crate) usages: Option<KimiUsages>,
    pub(crate) usage: Option<KimiUsageDetail>,
    #[serde(default)]
    pub(crate) limits: Vec<KimiRateLimit>,
    pub(crate) user: Option<KimiUser>,
    pub(crate) version: Option<String>,
}

#[derive(Debug)]
pub enum KimiUsages {
    Pools(Box<KimiPools>),
    List(Vec<KimiUsageItem>),
}

impl<'de> Deserialize<'de> for KimiUsages {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        match value {
            serde_json::Value::Array(_) => serde_json::from_value::<Vec<KimiUsageItem>>(value)
                .map(Self::List)
                .map_err(D::Error::custom),
            serde_json::Value::Object(_) => serde_json::from_value::<KimiPools>(value)
                .map(|pools| Self::Pools(Box::new(pools)))
                .map_err(D::Error::custom),
            _ => Err(D::Error::custom("Kimi usages must be an array or object")),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct KimiPools {
    #[serde(default, deserialize_with = "deserialize_present_json_value")]
    pub(crate) limit_5h: Option<serde_json::Value>,
    #[serde(default, deserialize_with = "deserialize_present_json_value")]
    pub(crate) limit_7d: Option<serde_json::Value>,
    #[serde(default, deserialize_with = "deserialize_present_json_value")]
    pub(crate) limit_month_total: Option<serde_json::Value>,
}

impl KimiPools {
    pub(crate) fn any(&self) -> bool {
        self.limit_5h.is_some() || self.limit_7d.is_some() || self.limit_month_total.is_some()
    }
}

#[derive(Debug, Deserialize)]
pub struct KimiPool {
    #[serde(default, deserialize_with = "deserialize_present_kimi_count")]
    pub(crate) limit: Option<KimiCount>,
    #[serde(default, deserialize_with = "deserialize_present_kimi_count")]
    pub(crate) used: Option<KimiCount>,
    #[serde(default, deserialize_with = "deserialize_present_kimi_count")]
    pub(crate) remaining: Option<KimiCount>,
    #[serde(default, deserialize_with = "deserialize_present_json_value")]
    pub(crate) used_ratio: Option<serde_json::Value>,
    #[serde(
        default,
        rename = "resetTime",
        alias = "reset_time",
        alias = "reset_at",
        alias = "resetAt"
    )]
    pub(crate) reset_time: Option<KimiReset>,
    pub(crate) name: Option<String>,
    pub(crate) title: Option<String>,
}

impl KimiPool {
    pub(crate) fn used_percent(&self) -> Result<Option<CanonicalUsedPercent>, ()> {
        kimi_reported_used_percent(self.used_ratio.as_ref())
    }

    pub(crate) fn label(&self, fallback: &str) -> String {
        [self.name.as_deref(), self.title.as_deref()]
            .into_iter()
            .flatten()
            .map(str::trim)
            .find(|value| !value.is_empty())
            .unwrap_or(fallback)
            .to_owned()
    }
}

#[derive(Debug, Clone)]
pub enum KimiCount {
    Num(serde_json::Number),
    Text(String),
    Invalid(serde_json::Value),
}

impl KimiCount {
    pub(crate) fn value(&self) -> Option<u64> {
        match self {
            Self::Num(value) => value.as_u64(),
            Self::Text(value) => value.trim().parse().ok(),
            Self::Invalid(_) => None,
        }
    }

    pub(crate) fn observation(&self) -> Result<u64, &'static str> {
        match self {
            Self::Num(value) => value.as_u64().ok_or_else(|| {
                if value.to_string().starts_with('-') {
                    "negative count has an unverified unsigned domain"
                } else {
                    "fractional or out-of-range count is invalid"
                }
            }),
            Self::Text(value) => {
                let value = value.trim();
                value.parse::<u64>().map_err(|_| {
                    if value.starts_with('-') {
                        "negative count has an unverified unsigned domain"
                    } else {
                        "nonnumeric, fractional or out-of-range count is invalid"
                    }
                })
            }
            Self::Invalid(_) => Err("count has an unsupported JSON type"),
        }
    }
}

fn deserialize_present_kimi_count<'de, D>(deserializer: D) -> Result<Option<KimiCount>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(Some(match value {
        serde_json::Value::Number(value) => KimiCount::Num(value),
        serde_json::Value::String(value) => KimiCount::Text(value),
        value => KimiCount::Invalid(value),
    }))
}

fn deserialize_present_json_value<'de, D>(
    deserializer: D,
) -> Result<Option<serde_json::Value>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    serde_json::Value::deserialize(deserializer).map(Some)
}

pub(crate) fn kimi_reported_used_percent(
    ratio: Option<&serde_json::Value>,
) -> Result<Option<CanonicalUsedPercent>, ()> {
    let Some(ratio) = ratio else {
        return Ok(None);
    };
    let reported_ratio = match ratio {
        serde_json::Value::Number(value) => value.as_f64(),
        serde_json::Value::String(value) => value.trim().parse::<f64>().ok(),
        _ => None,
    };
    let Some(reported_ratio) = reported_ratio else {
        return Err(());
    };
    if !reported_ratio.is_finite() || reported_ratio < 0.0 {
        return Err(());
    }
    // Kimi Code's managed `/coding/v1/usages` contract reports `used_ratio` as
    // a fraction. Keep this source-path conversion uniform, including values
    // above one that represent overage; magnitude never changes the unit.
    let points = reported_ratio * 100.0;
    CanonicalUsedPercent::new(points).map(Some).map_err(|_| ())
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum KimiReset {
    Text(String),
    Num(i64),
}

impl KimiReset {
    pub(crate) fn epoch(&self) -> Option<i64> {
        match self {
            Self::Num(value) => Some(epoch_seconds_from_maybe_ms(*value)),
            Self::Text(value) => parse_iso_epoch(value.trim()).or_else(|| {
                value
                    .trim()
                    .parse::<i64>()
                    .ok()
                    .map(epoch_seconds_from_maybe_ms)
            }),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct KimiUsageItem {
    pub(crate) scope: Option<String>,
    pub(crate) detail: KimiUsageDetail,
    #[serde(default)]
    pub(crate) limits: Vec<KimiRateLimit>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct KimiUsageDetail {
    #[serde(default, deserialize_with = "deserialize_present_kimi_count")]
    pub(crate) limit: Option<KimiCount>,
    #[serde(default, deserialize_with = "deserialize_present_kimi_count")]
    pub(crate) used: Option<KimiCount>,
    #[serde(default, deserialize_with = "deserialize_present_kimi_count")]
    pub(crate) remaining: Option<KimiCount>,
    #[serde(
        default,
        rename = "resetTime",
        alias = "reset_time",
        alias = "reset_at",
        alias = "resetAt"
    )]
    pub(crate) reset_time: Option<KimiReset>,
    pub(crate) name: Option<String>,
    pub(crate) title: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct KimiUser {
    pub(crate) id: Option<String>,
    pub(crate) email: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) membership: Option<KimiMembership>,
}

#[derive(Debug, Deserialize)]
pub struct KimiMembership {
    pub(crate) level: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct KimiRateLimit {
    pub(crate) window: Option<KimiWindow>,
    pub(crate) detail: KimiUsageDetail,
}

#[derive(Debug, Deserialize)]
pub struct KimiWindow {
    pub(crate) duration: Option<i64>,
    #[serde(rename = "timeUnit")]
    pub(crate) time_unit: Option<String>,
}
