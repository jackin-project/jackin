// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::control::{
    CanonicalUsedPercent, FocusedAccountHeader, FocusedUsageView, PercentObservationOrigin,
    QuotaBucketView, StatusSlot, UsageConfidence, UsageSnapshotStatus, UsageSource,
};
use jackin_usage_provider_core::{
    status_bar_quota_labels, timed_bucket, window_minutes_label, with_status_slot,
    with_used_percent,
};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
struct MuseUsageRaw {
    #[serde(rename = "observedAtMs")]
    observed_at_ms: i64,
    #[serde(default)]
    tier: Option<String>,
    #[serde(default)]
    window: Option<serde_json::Value>,
    #[serde(default)]
    weekly: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
struct MuseUsageReadRaw {
    #[serde(default)]
    usage: Option<MuseUsageRaw>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MuseWindow {
    pub used_percent: CanonicalUsedPercent,
    pub resets_at: Option<i64>,
    pub window_duration_mins: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MuseObservation {
    pub observed_at_ms: i64,
    pub tier: Option<String>,
    pub window: Option<MuseWindow>,
    pub invalid_window: bool,
    pub invalid_window_resets_at: Option<i64>,
    pub invalid_window_duration_mins: Option<i64>,
    pub weekly: Option<MuseWindow>,
    pub invalid_weekly: bool,
    pub invalid_weekly_resets_at: Option<i64>,
    pub invalid_weekly_duration_mins: Option<i64>,
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
    let (window, invalid_window, invalid_window_resets_at, invalid_window_duration_mins) =
        parse_optional_muse_window(raw.window);
    let (weekly, invalid_weekly, invalid_weekly_resets_at, invalid_weekly_duration_mins) =
        parse_optional_muse_window(raw.weekly);
    Ok(MuseObservation {
        observed_at_ms: raw.observed_at_ms,
        tier: raw
            .tier
            .map(|tier| tier.trim().to_owned())
            .filter(|tier| !tier.is_empty()),
        window,
        invalid_window,
        invalid_window_resets_at,
        invalid_window_duration_mins,
        weekly,
        invalid_weekly,
        invalid_weekly_resets_at,
        invalid_weekly_duration_mins,
    })
}

fn parse_optional_muse_window(
    raw: Option<serde_json::Value>,
) -> (Option<MuseWindow>, bool, Option<i64>, Option<i64>) {
    let Some(raw) = raw else {
        return (None, false, None, None);
    };
    let resets_at = muse_window_resets_at(&raw);
    let duration_mins = muse_window_duration_mins(&raw);
    match parse_muse_window(raw) {
        Ok(window) => (Some(window), false, None, None),
        Err(()) => (None, true, resets_at, duration_mins),
    }
}

fn parse_muse_window(raw: serde_json::Value) -> Result<MuseWindow, ()> {
    let used_percent = raw
        .get("usedPercent")
        .and_then(serde_json::Value::as_f64)
        .and_then(|percent| CanonicalUsedPercent::new(percent).ok())
        .ok_or(())?;
    Ok(MuseWindow {
        used_percent,
        resets_at: muse_window_resets_at(&raw),
        window_duration_mins: muse_window_duration_mins(&raw),
    })
}

fn muse_window_resets_at(raw: &serde_json::Value) -> Option<i64> {
    raw.get("resetsAtMs")
        .and_then(serde_json::Value::as_i64)
        .filter(|ms| *ms >= 0)
        .map(|ms| ms.div_euclid(1000))
}

fn muse_window_duration_mins(raw: &serde_json::Value) -> Option<i64> {
    raw.get("windowDurationMins")
        .and_then(serde_json::Value::as_i64)
        .filter(|mins| *mins > 0)
}

fn muse_window_bucket(
    label: &str,
    slot: StatusSlot,
    window: &MuseWindow,
    now: i64,
) -> QuotaBucketView {
    let bucket = with_status_slot(
        timed_bucket(
            label,
            Some(window.used_percent.label()),
            Some("100%".to_owned()),
            None,
            window.resets_at,
            now,
            None,
            UsageSnapshotStatus::Fresh,
        ),
        Some(slot),
    );
    with_used_percent(
        bucket,
        Some((
            window.used_percent,
            PercentObservationOrigin::ProviderReported,
        )),
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
    if observation.invalid_window {
        let label = observation
            .invalid_window_duration_mins
            .and_then(window_minutes_label)
            .unwrap_or_else(|| "Window".to_owned());
        buckets.push(with_status_slot(
            timed_bucket(
                &label,
                None,
                Some("100%".to_owned()),
                None,
                observation.invalid_window_resets_at,
                now,
                Some("Muse window usedPercent is invalid"),
                UsageSnapshotStatus::Unavailable,
            ),
            Some(StatusSlot::Session),
        ));
    }
    if observation.invalid_weekly {
        buckets.push(with_status_slot(
            timed_bucket(
                "Weekly",
                None,
                Some("100%".to_owned()),
                None,
                observation.invalid_weekly_resets_at,
                now,
                Some("Muse weekly usedPercent is invalid"),
                UsageSnapshotStatus::Unavailable,
            ),
            Some(StatusSlot::Weekly),
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
    let has_observation = buckets.iter().any(muse_bucket_has_quantitative_observation);
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
    let last_error = muse_view_diagnostics(observation, &buckets, has_observation);
    FocusedUsageView {
        focused_agent: Some(agent.to_owned()),
        focused_provider: Some("Muse".to_owned()),
        account: FocusedAccountHeader {
            provider_label: "Muse".to_owned(),
            account_label: account_label.to_owned(),
            username: None,
            plan_label: observation.and_then(|observation| observation.tier.clone()),
            credential_origin: Some("Muse login · auth.json identity".to_owned()),
            account_capability: None,
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
        last_error,
    }
}

fn muse_bucket_has_quantitative_observation(bucket: &QuotaBucketView) -> bool {
    bucket.status == UsageSnapshotStatus::Fresh
        && (bucket.used_percent.is_some()
            || bucket.remaining_percent.is_some()
            || bucket.used_money.is_some()
            || bucket.limit_money.is_some()
            || bucket.count_quota.is_some_and(|quota| {
                quota.used.is_some() || quota.limit.is_some() || quota.remaining.is_some()
            }))
}

fn muse_view_diagnostics(
    observation: Option<&MuseObservation>,
    buckets: &[QuotaBucketView],
    has_observation: bool,
) -> Option<String> {
    let mut diagnostics = buckets
        .iter()
        .filter_map(|bucket| bucket.pace_label.as_deref())
        .filter(|message| !message.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if !has_observation && diagnostics.is_empty() {
        diagnostics.push(if observation.is_some() {
            "Muse usage response contained no quantitative observation".to_owned()
        } else {
            "Muse usage has no cached observation".to_owned()
        });
    }
    (!diagnostics.is_empty()).then(|| diagnostics.join("; "))
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
