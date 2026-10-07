// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Kimi` local-server usage and token loading.

use super::super::json_epoch_seconds;
use jackin_protocol::control::{Money, QuotaBucketView, StatusSlot, UsageSnapshotStatus};
use jackin_usage_provider_core::{
    bucket, home_path, json_number, normalize_url_or_host, provider_http_client, provider_request,
    read_json_file,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

/// Experimental local-server usage shape (`GET /api/v1/oauth/usage`): the
/// Extra Usage wallet. `summary`/`limits` schemas are version-specific and
/// carried opaquely; only the wallet maps to a bucket today.
#[derive(Debug, Deserialize)]
pub(crate) struct KimiLocalUsage {
    pub(crate) summary: Option<serde_json::Value>,
    pub(crate) limits: Option<serde_json::Value>,
    pub(crate) extra_usage: Option<KimiExtraUsage>,
    pub(crate) error: Option<KimiLocalError>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct KimiLocalError {
    pub(crate) code: Option<String>,
    pub(crate) message: Option<String>,
}

/// Unitless wallet keys whose scale was never evidenced: a major-unit value
/// under one of these would render 100x off as [`Money`], so hits render as
/// unknown-scale labels, never money (F06 currency precision).
pub(crate) const KIMI_UNKNOWN_SCALE_KEYS: &[&str] = &[
    "remaining",
    "used",
    "monthly_cap",
    "monthly_used",
    "cap",
    "limit",
];

/// Extra Usage wallet: remaining balance, wallet size, and the monthly
/// cap/used pair, all in minor currency units. Only evidenced keys feed
/// [`Money`] — the `*_cents` spellings plus bare `balance`/`total`; the
/// unitless aliases land in `other` and surface as unknown-scale labels.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct KimiExtraUsage {
    #[serde(alias = "balance_cents", alias = "remaining_cents")]
    pub(crate) balance: Option<i64>,
    #[serde(alias = "total_cents")]
    pub(crate) total: Option<i64>,
    // `rename`, not `alias`: the bare unitless spellings (`monthly_cap`,
    // `monthly_used`) must NOT bind here — they fall through to `other` as
    // unknown-scale labels.
    #[serde(
        rename = "monthly_cap_cents",
        alias = "cap_cents",
        alias = "limit_cents"
    )]
    pub(crate) monthly_cap: Option<i64>,
    #[serde(rename = "monthly_used_cents", alias = "used_cents")]
    pub(crate) monthly_used: Option<i64>,
    pub(crate) currency: Option<String>,
    #[serde(default, flatten)]
    pub(crate) other: HashMap<String, serde_json::Value>,
}

impl KimiExtraUsage {
    /// First unitless-alias hit as an unknown-scale `(key, display)` pair, or
    /// `None` when no unitless wallet key carried a finite number.
    pub(crate) fn unknown_scale_hit(&self) -> Option<(&str, String)> {
        KIMI_UNKNOWN_SCALE_KEYS.iter().find_map(|key| {
            let value = self.other.get(*key).and_then(json_number)?;
            if !value.is_finite() {
                return None;
            }
            let display = if value.fract() == 0.0 {
                format!("{value:.0}")
            } else {
                format!("{value}")
            };
            Some((*key, display))
        })
    }
}

impl KimiLocalUsage {
    /// Reject an HTTP-200 body carrying an in-band error; otherwise return the
    /// wallet (if any).
    pub(crate) fn wallet(&self) -> Result<Option<&KimiExtraUsage>, String> {
        if let Some(error) = &self.error {
            let detail = error
                .message
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .or(error.code.as_deref())
                .unwrap_or("unknown error");
            return Err(format!("Kimi local usage error: {detail}"));
        }
        Ok(self.extra_usage.as_ref())
    }
}

pub(crate) fn fetch_kimi_local_usage(
    base_url: &str,
    credential: &str,
) -> Result<KimiLocalUsage, String> {
    let url = normalize_url_or_host(base_url, "api/v1/oauth/usage");
    provider_request(
        jackin_telemetry::schema::enums::ProviderName::Kimi,
        "GET",
        "/api/v1/oauth/usage",
        || {
            let client = provider_http_client()?;
            let response = client
                .get(&url)
                .bearer_auth(credential)
                .header(reqwest::header::ACCEPT, "application/json")
                .send()
                .map_err(|err| format!("Kimi local usage request failed: {err}"))?;
            let status = response.status();
            if !status.is_success() {
                return Err(format!("Kimi local usage HTTP {status}"));
            }
            let usage = response
                .json::<KimiLocalUsage>()
                .map_err(|err| format!("Kimi local usage decode failed: {err}"))?;
            usage.wallet()?;
            Ok(usage)
        },
    )
}

/// Map the Extra Usage wallet to a `Spend`-slot money bucket: monthly used of
/// the monthly cap when present, else wallet consumed (`total - balance`) of
/// the wallet total. Currency falls back to the generic `credits` label —
/// never an assumed fiat code. With no usable money pair, a unitless-alias
/// hit renders as a label-only unknown-scale row (never [`Money`]); with
/// neither, there is no bucket at all.
pub(crate) fn kimi_extra_usage_bucket(extra: &KimiExtraUsage) -> Option<QuotaBucketView> {
    let currency = extra
        .currency
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("credits");
    let (used_minor, limit_minor) =
        if let (Some(used), Some(limit)) = (extra.monthly_used, extra.monthly_cap) {
            (used, Some(limit))
        } else if let (Some(total), Some(balance)) = (extra.total, extra.balance) {
            (total.saturating_sub(balance), Some(total))
        } else {
            return kimi_unknown_scale_bucket(extra);
        };
    if used_minor < 0 || limit_minor.is_some_and(|limit| limit < 0) {
        return None;
    }
    let used = Money::new(used_minor, currency, 2);
    let limit = limit_minor.map(|limit| Money::new(limit, currency, 2));
    let used_percent = limit_minor.and_then(|limit| {
        if limit <= 0 {
            return None;
        }
        #[expect(
            clippy::cast_sign_loss,
            reason = "used/limit checked non-negative; percent is rounded f64→u8"
        )]
        {
            Some(((used_minor.clamp(0, limit) as f64 / limit as f64) * 100.0).round() as u8)
        }
    });
    let remaining_percent = used_percent.map(|used| 100u8.saturating_sub(used));
    let mut view = bucket(
        "Extra usage",
        Some(format!("{used} spent")),
        limit.as_ref().map(ToString::to_string),
        remaining_percent,
        None,
        used_percent.map(|used| format!("{used}% used")).as_deref(),
        UsageSnapshotStatus::Fresh,
    );
    view.status_slot = Some(StatusSlot::Spend);
    view.used_money = Some(used);
    view.limit_money = limit;
    Some(view)
}

/// Label-only wallet row for a unitless-alias hit: the raw figure with its
/// key, explicitly unknown-scale. No [`Money`], no percent, no headline slot —
/// a detail row the headline ignores.
pub(crate) fn kimi_unknown_scale_bucket(extra: &KimiExtraUsage) -> Option<QuotaBucketView> {
    let (key, display) = extra.unknown_scale_hit()?;
    Some(bucket(
        "Extra usage",
        Some(display),
        None,
        None,
        None,
        Some(&format!("{key} · unknown scale")),
        UsageSnapshotStatus::Fresh,
    ))
}

pub(crate) fn load_kimi_local_token(now: i64) -> Option<String> {
    load_kimi_local_token_from_home(&home_path(""), now)
}

pub(crate) fn load_kimi_local_token_from_home(home: &Path, now: i64) -> Option<String> {
    [
        home.join(".kimi-code/credentials/kimi-code.json"),
        home.join(".kimi/credentials/kimi-code.json"),
    ]
    .into_iter()
    .find_map(|path| {
        let value = read_json_file(&path)?;
        kimi_local_token_from_value(&value, now)
    })
}

pub(crate) fn kimi_local_token_from_value(value: &serde_json::Value, now: i64) -> Option<String> {
    if let Some(expires_at) = value.get("expires_at").and_then(json_epoch_seconds)
        && expires_at <= now
    {
        return None;
    }
    value
        .get("access_token")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}
