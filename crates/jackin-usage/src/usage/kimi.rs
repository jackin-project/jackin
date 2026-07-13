// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `Kimi` usage snapshot.
//!
//! Carved out of `usage.rs` during codebase-health-enforcement Workstream W5
//! (file-size ratchet). Items in this module are `pub(crate)` so the
//! coordinator (`usage.rs`) can re-export them.

#[allow(
    clippy::wildcard_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
use super::*;
use serde::Deserialize;

pub(crate) fn kimi_snapshot(agent: &str, token: Option<&str>, now: i64) -> FocusedUsageView {
    let has_local = home_path(".kimi-code").exists() || home_path(".kimi").exists();
    let has_token = token.is_some_and(|value| !value.is_empty());
    let (provider_usage, provider_error) = split_fetch(token.map(fetch_kimi_usage));
    let (status, source, confidence) = provider_outcome(ProviderPresence {
        has_data: provider_usage.is_some(),
        has_secret: has_token || has_local,
    });
    let buckets = provider_usage
        .as_ref()
        .map(|usage| usage.buckets(now))
        .filter(|buckets| !buckets.is_empty())
        .unwrap_or_else(|| {
            vec![
                bucket(
                    "Weekly",
                    None,
                    None,
                    None,
                    None,
                    provider_error
                        .as_deref()
                        .or(Some("Kimi billing endpoint unavailable")),
                    status,
                ),
                bucket(
                    "5-hour rate limit",
                    None,
                    None,
                    None,
                    None,
                    provider_error
                        .as_deref()
                        .or(Some("Kimi billing endpoint unavailable")),
                    status,
                ),
            ]
        });
    usage_view(UsageViewInput {
        agent,
        provider: None,
        surface: UsageSurface::Kimi,
        account_label: String::new(),
        username: None,
        plan_label: None,
        credential_origin: Some(
            if has_token {
                "API token · env KIMI_CODE_API_KEY"
            } else if has_local {
                "API key · ~/.kimi-code"
            } else {
                "needs Kimi auth"
            }
            .to_owned(),
        ),
        buckets,
        status,
        source,
        confidence,
        now,
        last_error: match status {
            UsageSnapshotStatus::NeedsSecret => {
                Some("Kimi auth not available to Capsule".to_owned())
            }
            UsageSnapshotStatus::Unsupported => Some(provider_error.unwrap_or_else(|| {
                "Kimi billing endpoint unavailable; local presence only".to_owned()
            })),
            _ => None,
        },
    })
}

#[derive(Debug, Deserialize)]
pub(crate) struct KimiUsageResponse {
    #[serde(default)]
    pub(crate) usages: Vec<KimiUsageItem>,
    pub(crate) usage: Option<KimiUsageDetail>,
    #[serde(default)]
    pub(crate) limits: Vec<KimiRateLimit>,
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
    pub(crate) limit: String,
    pub(crate) used: Option<String>,
    pub(crate) remaining: Option<String>,
    #[serde(rename = "resetTime")]
    pub(crate) reset_time: Option<String>,
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

impl KimiUsageResponse {
    pub(crate) fn buckets(&self, now: i64) -> Vec<QuotaBucketView> {
        let (detail, limits) = if let Some(detail) = &self.usage {
            (detail, self.limits.as_slice())
        } else if let Some(usage) = self
            .usages
            .iter()
            .find(|usage| usage.scope.as_deref() == Some("FEATURE_CODING"))
            .or_else(|| self.usages.first())
        {
            (&usage.detail, usage.limits.as_slice())
        } else {
            return Vec::new();
        };
        // rate (short/active) window on top, then Weekly — an operator
        // override of CodexBar's Weekly, Rate Limit order.
        let mut buckets = Vec::new();
        if let Some(rate_limit) = limits.first() {
            buckets.push(with_status_slot(
                kimi_bucket(
                    "Rate Limit",
                    &rate_limit.detail,
                    rate_limit.window.as_ref(),
                    now,
                ),
                Some(StatusSlot::Session),
            ));
        }
        buckets.push(with_status_slot(
            kimi_bucket("Weekly", detail, None, now),
            Some(StatusSlot::Weekly),
        ));
        buckets
    }
}

impl KimiUsageDetail {
    pub(crate) fn limit_value(&self) -> Option<i64> {
        self.limit.trim().parse().ok()
    }

    pub(crate) fn used_value(&self) -> Option<i64> {
        self.used
            .as_deref()
            .and_then(|value| value.trim().parse().ok())
    }

    pub(crate) fn remaining_value(&self) -> Option<i64> {
        self.remaining
            .as_deref()
            .and_then(|value| value.trim().parse().ok())
    }

    pub(crate) fn used_percent(&self) -> Option<u8> {
        let limit = self.limit_value()?.max(0);
        if limit == 0 {
            return None;
        }
        let used = self.used_value().or_else(|| {
            self.remaining_value()
                .map(|remaining| limit.saturating_sub(remaining))
        })?;
        #[expect(
            clippy::cast_sign_loss,
            reason = "used/limit clamped non-negative; percent is rounded f64→u8"
        )]
        {
            Some(((used.clamp(0, limit) as f64 / limit as f64) * 100.0).round() as u8)
        }
    }
}

impl KimiWindow {
    pub(crate) fn seconds(&self) -> Option<i64> {
        let duration = self.duration?;
        let unit = self
            .time_unit
            .as_deref()
            .unwrap_or("hour")
            .to_ascii_lowercase();
        if unit.contains("minute") {
            Some(duration * 60)
        } else if unit.contains("hour") {
            Some(duration * 60 * 60)
        } else if unit.contains("day") {
            Some(duration * 24 * 60 * 60)
        } else if unit.contains("week") {
            Some(duration * 7 * 24 * 60 * 60)
        } else {
            None
        }
    }
}

pub(crate) fn kimi_bucket(
    label: &str,
    detail: &KimiUsageDetail,
    window: Option<&KimiWindow>,
    now: i64,
) -> QuotaBucketView {
    let limit = detail.limit_value();
    let used = detail.used_value().or_else(|| {
        limit.and_then(|limit| {
            detail
                .remaining_value()
                .map(|remaining| limit.saturating_sub(remaining))
        })
    });
    let used_percent = detail.used_percent();
    let remaining = used_percent.map(|used| 100u8.saturating_sub(used));
    let reset_at = detail.reset_time.as_deref().and_then(parse_iso_epoch);
    let window_seconds = kimi_window_seconds(label, window);
    let pace = quota_pace_label(remaining, reset_at, window_seconds, now);
    timed_bucket(
        label,
        used.map(|value| compact_count(u64::try_from(value.max(0)).unwrap_or(0))),
        limit.map(|value| compact_count(u64::try_from(value.max(0)).unwrap_or(0))),
        remaining,
        reset_at,
        now,
        pace.as_deref(),
        UsageSnapshotStatus::Fresh,
    )
}

pub(crate) fn kimi_window_seconds(label: &str, window: Option<&KimiWindow>) -> Option<i64> {
    (label == "Rate Limit")
        .then(|| window.and_then(KimiWindow::seconds))
        .flatten()
}

pub(crate) fn fetch_kimi_usage(token: &str) -> Result<KimiUsageResponse, String> {
    get_json_bearer(
        "Kimi usage",
        "https://api.kimi.com/coding/v1/usages",
        token,
        &[(reqwest::header::USER_AGENT, "jackin-capsule/usage")],
    )
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
