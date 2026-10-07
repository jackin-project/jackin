// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Kimi` usage response and pool types.

use jackin_usage_provider_core::{epoch_seconds_from_maybe_ms, parse_iso_epoch};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct KimiUsageResponse {
    #[serde(default)]
    pub(crate) usages: Option<KimiUsages>,
    pub(crate) usage: Option<KimiUsageDetail>,
    #[serde(default)]
    pub(crate) limits: Vec<KimiRateLimit>,
    pub(crate) user: Option<KimiUser>,
    pub(crate) version: Option<String>,
}

/// The two `usages` shapes: the web gateway sends a list of scoped entries
/// while the Code API sends the rolling/weekly/monthly pools object.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum KimiUsages {
    Pools(Box<KimiPools>),
    List(Vec<KimiUsageItem>),
}

#[derive(Debug, Deserialize)]
pub(crate) struct KimiPools {
    pub(crate) limit_5h: Option<KimiPool>,
    pub(crate) limit_7d: Option<KimiPool>,
    pub(crate) limit_month_total: Option<KimiPool>,
}

impl KimiPools {
    pub(crate) fn any(&self) -> bool {
        self.limit_5h.is_some() || self.limit_7d.is_some() || self.limit_month_total.is_some()
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct KimiPool {
    pub(crate) limit: Option<KimiCount>,
    pub(crate) used: Option<KimiCount>,
    pub(crate) remaining: Option<KimiCount>,
    pub(crate) used_ratio: Option<f64>,
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
    /// Raw used percent, unclamped: over-cap readings (>100%) survive so the
    /// bucket can carry the raw figure (T02); only the bar geometry clamps.
    pub(crate) fn used_percent_raw(&self) -> Option<f64> {
        if let Some(limit) = self
            .limit
            .as_ref()
            .and_then(KimiCount::value)
            .filter(|limit| *limit > 0)
        {
            let used = self.used.as_ref().and_then(KimiCount::value).or_else(|| {
                self.remaining
                    .as_ref()
                    .and_then(KimiCount::value)
                    .map(|remaining| limit.saturating_sub(remaining))
            })?;
            #[expect(clippy::cast_precision_loss, reason = "count magnitudes fit f64")]
            return Some(used.max(0) as f64 / limit as f64 * 100.0);
        }
        // `used_ratio` is a 0.0–1.0 fraction; values above 1.0 are an
        // already-scaled percent.
        self.used_ratio.and_then(|ratio| {
            if !ratio.is_finite() || ratio < 0.0 {
                return None;
            }
            Some(if ratio <= 1.0 { ratio * 100.0 } else { ratio })
        })
    }

    pub(crate) fn used_percent(&self) -> Option<u8> {
        self.used_percent_raw().map(|raw| {
            #[expect(
                clippy::cast_sign_loss,
                reason = "raw percent clamped non-negative; clamp bounds the f64→u8 cast"
            )]
            {
                raw.round().clamp(0.0, 100.0) as u8
            }
        })
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

/// A quota count sent either as a JSON number (Code API) or a string (web
/// gateway); both families are accepted so neither fails the whole parse.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub(crate) enum KimiCount {
    Num(i64),
    Text(String),
}

impl KimiCount {
    pub(crate) fn value(&self) -> Option<i64> {
        match self {
            Self::Num(value) => Some(*value),
            Self::Text(value) => value.trim().parse().ok(),
        }
    }
}

/// A reset timestamp sent either as RFC 3339 text or an epoch (seconds or
/// milliseconds); unparseable text yields no reset rather than failing.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub(crate) enum KimiReset {
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
pub(crate) struct KimiUsageItem {
    pub(crate) scope: Option<String>,
    pub(crate) detail: KimiUsageDetail,
    #[serde(default)]
    pub(crate) limits: Vec<KimiRateLimit>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct KimiUsageDetail {
    pub(crate) limit: Option<KimiCount>,
    pub(crate) used: Option<KimiCount>,
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
pub(crate) struct KimiUser {
    pub(crate) id: Option<String>,
    pub(crate) email: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) membership: Option<KimiMembership>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct KimiMembership {
    pub(crate) level: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct KimiRateLimit {
    pub(crate) window: Option<KimiWindow>,
    pub(crate) detail: KimiUsageDetail,
}

#[derive(Debug, Deserialize)]
pub(crate) struct KimiWindow {
    pub(crate) duration: Option<i64>,
    #[serde(rename = "timeUnit")]
    pub(crate) time_unit: Option<String>,
}
