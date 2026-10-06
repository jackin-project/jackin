// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `Muse` (Meta) usage observation via MSP `usage/read` + `usage/changed`.
//!
//! Preferred official observation: the subscription payload carries
//! `observedAtMs`, a tier label, a rolling window
//! (`usedPercent`/`resetsAtMs`/`windowDurationMins`) and a weekly window. The
//! response may omit `usage` when no observation exists, and percentages above
//! 100 are valid over-cap readings — preserved raw, never clamped. Re-reading
//! cached data must not reset its freshness timestamp.
//!
//! omp's `POST https://api.meta.ai/muse-code/key` is a *documented
//! conditional* only: the response can mint/return an API key, so it must never
//! be enabled as a read-only polling fallback. See [`MuseKeyExchangePolicy`].

use super::{
    FocusedAccountHeader, FocusedUsageView, QuotaBucketView, StatusSlot, UsageConfidence,
    UsageSnapshotStatus, UsageSource, status_bar_quota_labels, timed_bucket, window_minutes_label,
    with_status_slot,
};
use serde::Deserialize;

/// Raw MSP window: rolling window or weekly. Field names follow the pinned
/// MSP schema (`usedPercent`, `resetsAtMs`, `windowDurationMins`).
#[derive(Debug, Clone, Deserialize)]
struct MuseWindowRaw {
    #[serde(rename = "usedPercent")]
    used_percent: f64,
    #[serde(rename = "resetsAtMs", default)]
    resets_at_ms: Option<i64>,
    #[serde(rename = "windowDurationMins", default)]
    window_duration_mins: Option<i64>,
}

/// Raw MSP subscription usage: `observedAtMs` freshness marker, tier label,
/// rolling window and weekly window.
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

/// Raw MSP `usage/read` response. `usage` is absent when no observation exists.
#[derive(Debug, Clone, Deserialize)]
struct MuseUsageReadRaw {
    #[serde(default)]
    usage: Option<MuseUsageRaw>,
}

/// Parsed Muse window with the reset normalized to epoch seconds.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MuseWindow {
    pub(crate) used_percent: f64,
    pub(crate) resets_at: Option<i64>,
    pub(crate) window_duration_mins: Option<i64>,
}

/// Parsed Muse cached observation.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MuseObservation {
    pub(crate) observed_at_ms: i64,
    pub(crate) tier: Option<String>,
    pub(crate) window: Option<MuseWindow>,
    pub(crate) weekly: Option<MuseWindow>,
}

/// Muse login identity from `~/.config/muse/auth.json`
/// (`providers.meta.user_email` / `user_full_name`). The secret itself lives in
/// the platform credential store, never in this file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MuseIdentity {
    pub(crate) email: Option<String>,
    pub(crate) full_name: Option<String>,
}

fn trimmed_string(value: Option<&serde_json::Value>) -> Option<String> {
    value
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

pub(crate) fn muse_identity_from_value(value: &serde_json::Value) -> Option<MuseIdentity> {
    let meta = value.get("providers")?.get("meta")?;
    let identity = MuseIdentity {
        email: trimmed_string(meta.get("user_email")),
        full_name: trimmed_string(meta.get("user_full_name")),
    };
    (identity.email.is_some() || identity.full_name.is_some()).then_some(identity)
}

/// Parse one MSP `usage/read` response. `Ok(None)` is the honest
/// no-observation state (usage omitted), never an error.
pub(crate) fn parse_muse_usage_read(
    value: serde_json::Value,
) -> Result<Option<MuseObservation>, String> {
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

/// Used-side label preserving the raw provider figure, including over-cap
/// readings (`142.5% used`). MSP `usedPercent` is already a percent — never
/// run through the fraction/percent heuristic.
fn muse_used_label(used_percent: f64) -> String {
    if used_percent.fract() == 0.0 {
        format!("{used_percent:.0}% used")
    } else {
        format!("{used_percent:.1}% used")
    }
}

/// Remaining side for the meter: over-cap windows clamp remaining to 0
/// (nothing left / bar full) while the used label carries the raw overage.
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

pub(crate) fn muse_buckets(observation: &MuseObservation, now: i64) -> Vec<QuotaBucketView> {
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

/// Freshness for a re-read of the cached observation: when the re-read carries
/// the same `observedAtMs` as the cached one, the cached `fetched_at_epoch` is
/// kept — a re-read must not advance freshness. Only a new observation stamps
/// `now`.
pub(crate) fn muse_freshness_epoch(
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

pub(crate) fn muse_view(
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

/// omp's `POST https://api.meta.ai/muse-code/key`: documented conditional,
/// never a poller.
///
/// The exchange returns identity + quota in one call, but the response can
/// also mint/return an API key — until proven safe for passive monitoring it
/// must never back a refresh loop. No fetch function exists on this type by
/// design; a future conditional one-shot must whitelist only non-secret usage
/// fields, never send onboarding flags, and respect 429/backoff.
#[derive(Debug, Clone, Copy)]
pub(crate) struct MuseKeyExchangePolicy;

impl MuseKeyExchangePolicy {
    /// Key-exchange endpoint. Request shape: `POST` with
    /// `Authorization: Bearer <Meta OAuth access>`, `x-api-version: 1.0.0`,
    /// body `{}`.
    pub(crate) const URL: &str = "https://api.meta.ai/muse-code/key";

    /// Always `false`: the key exchange must never be enabled as a poller.
    pub(crate) fn polling_enabled() -> bool {
        false
    }

    /// Non-secret fields a conditional one-shot read may retain (whitelist).
    pub(crate) const ALLOWED_FIELDS: &[&str] = &[
        "user_email",
        "user_id",
        "is_subs_active",
        "subs_tier_id",
        "subs_tier_name",
        "subs_usage",
        "require_payment",
    ];

    /// Fields that must never be persisted, logged, or rendered. Payment
    /// action URLs are likewise dropped (absent from the whitelist) — a
    /// monitor never needs them.
    pub(crate) const SECRET_FIELDS: &[&str] = &[
        "api_key",
        "apiKey",
        "oauthAccessToken",
        "access_token",
        "refresh_token",
    ];
}

#[cfg(test)]
mod tests;
