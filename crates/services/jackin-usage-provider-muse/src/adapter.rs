// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::control::{
    FocusedAccountHeader, FocusedUsageView, QuotaBucketView, StatusSlot, UsageConfidence,
    UsageSnapshotStatus, UsageSource,
};
use jackin_usage_provider_core::{
    status_bar_quota_labels, timed_bucket, window_minutes_label, with_status_slot,
};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
struct MuseWindowRaw {
    #[serde(rename = "usedPercent")]
    used_percent: f64,
    #[serde(rename = "resetsAtMs", default)]
    resets_at_ms: Option<i64>,
    #[serde(rename = "windowDurationMins", default)]
    window_duration_mins: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
struct MuseUsageRaw {
    #[serde(rename = "observedAtMs")]
    observed_at_ms: i64,
    #[serde(default)]
    tier: Option<String>,
    #[serde(default)]
    window: Option<MuseWindowRaw>,
    #[serde(default)]
    weekly: Option<MuseWindowRaw>,
}

#[derive(Debug, Clone, Deserialize)]
struct MuseUsageReadRaw {
    #[serde(default)]
    usage: Option<MuseUsageRaw>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MuseWindow {
    pub used_percent: f64,
    pub resets_at: Option<i64>,
    pub window_duration_mins: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MuseObservation {
    pub observed_at_ms: i64,
    pub tier: Option<String>,
    pub window: Option<MuseWindow>,
    pub weekly: Option<MuseWindow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MuseIdentity {
    pub email: Option<String>,
    pub full_name: Option<String>,
}

fn trimmed_string(value: Option<&serde_json::Value>) -> Option<String> {
    value
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

pub fn muse_identity_from_value(value: &serde_json::Value) -> Option<MuseIdentity> {
    let meta = value.get("providers")?.get("meta")?;
    let identity = MuseIdentity {
        email: trimmed_string(meta.get("user_email")),
        full_name: trimmed_string(meta.get("user_full_name")),
    };
    (identity.email.is_some() || identity.full_name.is_some()).then_some(identity)
}

pub fn parse_muse_usage_read(value: serde_json::Value) -> Result<Option<MuseObservation>, String> {
    let raw: MuseUsageReadRaw = serde_json::from_value(value)
        .map_err(|_| "Muse usage/read response is malformed".to_owned())?;
    raw.usage.map(parse_muse_usage).transpose()
}

fn parse_muse_usage(raw: MuseUsageRaw) -> Result<MuseObservation, String> {
    if raw.observed_at_ms < 0 {
        return Err("Muse observedAtMs is invalid".to_owned());
    }
    Ok(MuseObservation {
        observed_at_ms: raw.observed_at_ms,
        tier: raw
            .tier
            .map(|tier| tier.trim().to_owned())
            .filter(|tier| !tier.is_empty()),
        window: raw
            .window
            .map(|window| parse_muse_window(window, "window"))
            .transpose()?,
        weekly: raw
            .weekly
            .map(|window| parse_muse_window(window, "weekly"))
            .transpose()?,
    })
}

fn parse_muse_window(raw: MuseWindowRaw, label: &str) -> Result<MuseWindow, String> {
    if !raw.used_percent.is_finite() || raw.used_percent < 0.0 {
        return Err(format!("Muse {label} usedPercent is invalid"));
    }
    if raw.resets_at_ms.is_some_and(|ms| ms < 0) {
        return Err(format!("Muse {label} resetsAtMs is invalid"));
    }
    Ok(MuseWindow {
        used_percent: raw.used_percent,
        resets_at: raw.resets_at_ms.map(|ms| ms.div_euclid(1000)),
        window_duration_mins: raw.window_duration_mins.filter(|mins| *mins > 0),
    })
}

fn muse_used_label(used_percent: f64) -> String {
    if used_percent.fract() == 0.0 {
        format!("{used_percent:.0}% used")
    } else {
        format!("{used_percent:.1}% used")
    }
}

fn muse_remaining_percent(used_percent: f64) -> Option<u8> {
    if used_percent > 100.0 {
        return Some(0);
    }
    #[expect(clippy::cast_sign_loss, reason = "used is 0.0..=100.0; clamped below")]
    Some((100.0 - used_percent).round().clamp(0.0, 100.0) as u8)
}

fn muse_window_bucket(
    label: &str,
    slot: StatusSlot,
    window: &MuseWindow,
    now: i64,
) -> QuotaBucketView {
    with_status_slot(
        timed_bucket(
            label,
            Some(muse_used_label(window.used_percent)),
            None,
            muse_remaining_percent(window.used_percent),
            window.resets_at,
            now,
            None,
            UsageSnapshotStatus::Fresh,
        ),
        Some(slot),
    )
}

pub fn muse_buckets(observation: &MuseObservation, now: i64) -> Vec<QuotaBucketView> {
    let mut buckets = Vec::with_capacity(2);
    if let Some(window) = &observation.window {
        let label = window
            .window_duration_mins
            .and_then(window_minutes_label)
            .unwrap_or_else(|| "Window".to_owned());
        buckets.push(muse_window_bucket(&label, StatusSlot::Session, window, now));
    }
    if let Some(weekly) = &observation.weekly {
        buckets.push(muse_window_bucket(
            "Weekly",
            StatusSlot::Weekly,
            weekly,
            now,
        ));
    }
    buckets
}

pub fn muse_freshness_epoch(
    cached_fetched: i64,
    cached_observed_ms: i64,
    reread_observed_ms: i64,
    now: i64,
) -> i64 {
    if reread_observed_ms == cached_observed_ms {
        cached_fetched
    } else {
        now
    }
}

pub fn muse_view(
    agent: &str,
    account_label: &str,
    observation: Option<&MuseObservation>,
    fetched_at_epoch: i64,
) -> FocusedUsageView {
    let buckets = observation
        .map(|observation| muse_buckets(observation, fetched_at_epoch))
        .unwrap_or_default();
    let has_observation = observation.is_some();
    let status = if has_observation {
        UsageSnapshotStatus::Fresh
    } else {
        UsageSnapshotStatus::Unavailable
    };
    let labels = status_bar_quota_labels(&buckets);
    let status_bar_label = if labels.is_empty() {
        if has_observation {
            "Muse".to_owned()
        } else {
            "usage unavailable".to_owned()
        }
    } else {
        labels.join(" · ")
    };
    FocusedUsageView {
        focused_agent: Some(agent.to_owned()),
        focused_provider: Some("Muse".to_owned()),
        account: FocusedAccountHeader {
            provider_label: "Muse".to_owned(),
            account_label: account_label.to_owned(),
            username: None,
            plan_label: observation.and_then(|observation| observation.tier.clone()),
            credential_origin: Some("Muse login · auth.json identity".to_owned()),
        },
        buckets,
        status,
        source: if has_observation {
            UsageSource::Cache
        } else {
            UsageSource::None
        },
        confidence: if has_observation {
            UsageConfidence::Authoritative
        } else {
            UsageConfidence::None
        },
        fetched_at_epoch,
        updated_label: if status == UsageSnapshotStatus::Fresh {
            "Updated now"
        } else {
            "Unavailable"
        }
        .to_owned(),
        status_bar_label,
        tabs: Vec::new(),
        last_error: if has_observation {
            None
        } else {
            Some("Muse usage has no cached observation".to_owned())
        },
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MuseKeyExchangePolicy;

impl MuseKeyExchangePolicy {
    pub const URL: &str = "https://api.meta.ai/muse-code/key";

    pub fn polling_enabled() -> bool {
        false
    }

    pub const ALLOWED_FIELDS: &[&str] = &[
        "user_email",
        "user_id",
        "is_subs_active",
        "subs_tier_id",
        "subs_tier_name",
        "subs_usage",
        "require_payment",
    ];

    pub const SECRET_FIELDS: &[&str] = &[
        "api_key",
        "apiKey",
        "oauthAccessToken",
        "access_token",
        "refresh_token",
    ];
}
