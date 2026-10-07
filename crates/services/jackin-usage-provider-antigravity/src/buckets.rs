// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Antigravity` identity, plan, and quota buckets.

use jackin_protocol::control::{QuotaBucketView, StatusSlot, UsageSnapshotStatus};
use jackin_usage_provider_core::{bucket, humanize_plan_label, quota_pace_label, timed_bucket};

use super::{AntigravityFamily, AntigravityPool, AntigravityUsage, AntigravityWindow};

pub(crate) const ANTIGRAVITY_SESSION_WINDOW_SECONDS: i64 = 5 * 60 * 60;
pub(crate) const ANTIGRAVITY_WEEKLY_WINDOW_SECONDS: i64 = 7 * 24 * 60 * 60;

/// Identity from a `/usage` response, if present. The command may omit it; the
/// caller must then bind the observation to its runtime, not to an account.
pub fn antigravity_identity_from_value(value: &serde_json::Value) -> Option<String> {
    for key in ["email", "emailAddress", "accountEmail", "userEmail", "user"] {
        if let Some(node) = value.get(key) {
            if let Some(text) = node.as_str()
                && !text.trim().is_empty()
            {
                return Some(text.trim().to_owned());
            }
            if let Some(text) = node
                .get("email")
                .or_else(|| node.get("emailAddress"))
                .and_then(serde_json::Value::as_str)
                && !text.trim().is_empty()
            {
                return Some(text.trim().to_owned());
            }
        }
    }
    None
}

/// Plan label: prefer the Google tier name over the Windsurf-inherited plan
/// name (always "Pro" when paid, so it carries no tier signal).
pub fn antigravity_plan_from_value(value: &serde_json::Value) -> Option<String> {
    for key in ["userTier", "currentTier", "paidTier", "tier"] {
        if let Some(name) = value
            .get(key)
            .and_then(|tier| tier.get("name").or_else(|| tier.get("id")))
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
        {
            return Some(humanize_plan_label(name));
        }
    }
    value
        .get("planInfo")
        .and_then(|info| info.get("planName"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(humanize_plan_label)
}

pub fn antigravity_buckets(usage: &AntigravityUsage, now: i64) -> Vec<QuotaBucketView> {
    let mut buckets = Vec::new();
    for pool in &usage.pools {
        buckets.push(antigravity_pool_bucket(pool, now));
    }
    // Legacy per-model rows are 5h-only: weekly reads "No data" for families
    // with a session pool but no weekly pool.
    if usage.legacy_fallback {
        for family in [AntigravityFamily::Gemini, AntigravityFamily::Other] {
            let has_session = usage.pools.iter().any(|pool| {
                pool.family == family && pool.window == Some(AntigravityWindow::Session)
            });
            let has_weekly = usage.pools.iter().any(|pool| {
                pool.family == family && pool.window == Some(AntigravityWindow::Weekly)
            });
            if has_session && !has_weekly {
                buckets.push(bucket(
                    antigravity_family_label(family, Some(AntigravityWindow::Weekly)).as_str(),
                    None,
                    None,
                    None,
                    None,
                    Some("No data"),
                    UsageSnapshotStatus::Fresh,
                ));
            }
        }
    }
    buckets
}

fn antigravity_pool_bucket(pool: &AntigravityPool, now: i64) -> QuotaBucketView {
    let label = match (&pool.window, &pool.source_label) {
        (Some(window), _) => antigravity_family_label(pool.family, Some(*window)),
        (None, Some(source)) => format!("Quota · {source}"),
        (None, None) => "Quota".to_owned(),
    };
    let remaining = pool.remaining_percent;
    let used_label = remaining.map(|left| format!("{}% used", 100u8.saturating_sub(left)));
    let window_seconds = match pool.window {
        Some(AntigravityWindow::Session) => Some(ANTIGRAVITY_SESSION_WINDOW_SECONDS),
        Some(AntigravityWindow::Weekly) => Some(ANTIGRAVITY_WEEKLY_WINDOW_SECONDS),
        None => None,
    };
    let pace = quota_pace_label(remaining, pool.reset_at, window_seconds, now);
    // No quota signal: an honest "No data" detail row (the legacy path's
    // convention), never a fabricated 0% or 100%.
    let pace = pace.or_else(|| remaining.is_none().then(|| "No data".to_owned()));
    let mut view = timed_bucket(
        &label,
        used_label,
        Some("100%".to_owned()),
        remaining,
        pool.reset_at,
        now,
        pace.as_deref(),
        UsageSnapshotStatus::Fresh,
    );
    // The Gemini family fills the headline slots; the non-Gemini pools are
    // detail rows the headline ignores.
    view.status_slot = match (pool.family, pool.window) {
        (AntigravityFamily::Gemini, Some(AntigravityWindow::Session)) => Some(StatusSlot::Session),
        (AntigravityFamily::Gemini, Some(AntigravityWindow::Weekly)) => Some(StatusSlot::Weekly),
        _ => None,
    };
    view
}

pub(crate) fn antigravity_family_label(
    family: AntigravityFamily,
    window: Option<AntigravityWindow>,
) -> String {
    let family_label = match family {
        AntigravityFamily::Gemini => "Gemini",
        AntigravityFamily::Other => "Other models",
    };
    match window {
        Some(AntigravityWindow::Session) => format!("{family_label} · 5h"),
        Some(AntigravityWindow::Weekly) => format!("{family_label} · Weekly"),
        None => family_label.to_owned(),
    }
}
