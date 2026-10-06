// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Codex` quota windows and reset credits.

use super::super::{
    Money, QuotaBucketView, StatusSlot, UsageSnapshotStatus, bucket, codex_limit_label,
    expiry_label, format_amount_with_unit, json_number, parse_iso_epoch, quota_pace_label,
    timed_bucket, with_status_slot,
};
use super::{CodexIndividualLimit, CodexUsageResponse, CodexWindowSnapshot};
use serde::Deserialize;

impl CodexUsageResponse {
    pub(crate) fn individual_limit(&self) -> Option<&CodexIndividualLimit> {
        self.individual_limit
            .as_ref()
            .or_else(|| self.spend_control.as_ref()?.individual_limit.as_ref())
    }

    pub(crate) fn buckets(&self, now: i64) -> Vec<QuotaBucketView> {
        let mut buckets = Vec::new();
        if let Some(rate_limit) = &self.rate_limit {
            let primary = rate_limit.primary_window.as_ref();
            let secondary = rate_limit.secondary_window.as_ref();
            let primary_slot = primary.and_then(CodexWindowSnapshot::exact_status_slot);
            let secondary_slot = secondary
                .and_then(CodexWindowSnapshot::exact_status_slot)
                .filter(|slot| Some(*slot) != primary_slot);
            let primary_slot = primary_slot
                .or_else(|| preferred_slot(StatusSlot::Session, primary_slot, secondary_slot));
            let secondary_slot = secondary_slot
                .or_else(|| preferred_slot(StatusSlot::Weekly, primary_slot, secondary_slot));
            if let Some(window) = primary {
                let label = codex_window_display_label(window, primary_slot);
                push_codex_window(&mut buckets, &label, primary_slot, Some(window), now);
            }
            if let Some(window) = secondary {
                let label = codex_window_display_label(window, secondary_slot);
                push_codex_window(&mut buckets, &label, secondary_slot, Some(window), now);
            }
        }
        for limit in self.additional_rate_limits.iter().flatten() {
            let label = limit
                .limit_name
                .as_deref()
                .or(limit.metered_feature.as_deref())
                .map_or_else(|| "Codex extra limit".to_owned(), codex_limit_label);
            if let Some(rate_limit) = &limit.rate_limit {
                // Extra per-feature limits are detail rows, never the headline.
                push_codex_window(
                    &mut buckets,
                    &format!("{label} 5-hour"),
                    None,
                    rate_limit.primary_window.as_ref(),
                    now,
                );
                push_codex_window(
                    &mut buckets,
                    &format!("{label} Weekly"),
                    None,
                    rate_limit.secondary_window.as_ref(),
                    now,
                );
            }
        }
        if let Some(reset_credits) = &self.reset_credits
            && reset_credits.available_count > 0
        {
            let detail = reset_credits.detail_label(now);
            buckets.push(bucket(
                "Limit Reset Credits",
                None,
                None,
                None,
                None,
                Some(detail.as_str()),
                UsageSnapshotStatus::Fresh,
            ));
        }
        if let Some(credits) = &self.credits
            && credits.has_credits.unwrap_or(false)
        {
            let balance = credits.balance.as_ref().and_then(json_number);
            buckets.push(bucket(
                "Credits",
                None,
                balance.map(|value| format_amount_with_unit(value, "credits")),
                credits.unlimited.unwrap_or(false).then_some(100),
                None,
                credits.unlimited.unwrap_or(false).then_some("unlimited"),
                UsageSnapshotStatus::Fresh,
            ));
        }
        if let Some(limit) = self.individual_limit() {
            let limit_money = limit.limit.as_ref().and_then(codex_money_value);
            let used_money = limit.used.as_ref().and_then(codex_money_value);
            let remaining = limit.remaining_percent.or_else(|| {
                let used = used_money.as_ref()?.amount_minor;
                let cap = limit_money.as_ref()?.amount_minor;
                remaining_from_money(used, cap)
            });
            if used_money.is_some() || limit_money.is_some() || remaining.is_some() {
                let mut view = timed_bucket(
                    "Individual limit",
                    used_money.as_ref().map(ToString::to_string),
                    limit_money.as_ref().map(ToString::to_string),
                    remaining,
                    limit.resets_at,
                    now,
                    None,
                    UsageSnapshotStatus::Fresh,
                );
                view.status_slot = Some(StatusSlot::Spend);
                view.used_money = used_money;
                view.limit_money = limit_money;
                buckets.push(view);
            }
        }
        buckets
    }
}

pub(crate) fn remaining_from_money(used: i64, cap: i64) -> Option<u8> {
    if cap <= 0 {
        return None;
    }
    let used = u128::try_from(used).ok()?;
    let cap = u128::try_from(cap).ok()?;
    let remaining = cap
        .saturating_sub(used)
        .saturating_mul(100)
        .checked_div(cap)?
        .min(100);
    u8::try_from(remaining).ok()
}

pub(crate) fn preferred_slot(
    preferred: StatusSlot,
    primary: Option<StatusSlot>,
    secondary: Option<StatusSlot>,
) -> Option<StatusSlot> {
    if primary != Some(preferred) && secondary != Some(preferred) {
        return Some(preferred);
    }
    let alternate = match preferred {
        StatusSlot::Session => StatusSlot::Weekly,
        StatusSlot::Weekly => StatusSlot::Session,
        _ => preferred,
    };
    (primary != Some(alternate) && secondary != Some(alternate)).then_some(alternate)
}

pub(crate) fn codex_window_display_label(
    window: &CodexWindowSnapshot,
    slot: Option<StatusSlot>,
) -> String {
    match slot {
        Some(StatusSlot::Session) => "Session".to_owned(),
        Some(StatusSlot::Weekly) => "Weekly".to_owned(),
        _ => window.window_label().unwrap_or_else(|| "Window".to_owned()),
    }
}

pub(crate) fn codex_money_value(value: &serde_json::Value) -> Option<Money> {
    let amount = match value {
        serde_json::Value::Number(number) => number.as_f64()?,
        serde_json::Value::String(string) => string.trim().parse::<f64>().ok()?,
        _ => return None,
    };
    if !amount.is_finite() || amount < 0.0 {
        return None;
    }
    let cents = (amount * 100.0).round() as i64;
    Some(Money::new(cents, "USD", 2))
}

#[derive(Debug, Deserialize)]
pub(crate) struct CodexResetCredits {
    pub(crate) credits: Vec<CodexResetCredit>,
    #[serde(rename = "available_count")]
    pub(crate) available_count: i64,
}

impl CodexResetCredits {
    pub(crate) fn detail_label(&self, now: i64) -> String {
        let count = if self.available_count == 1 {
            "1 manual reset available".to_owned()
        } else {
            format!("{} manual resets available", self.available_count)
        };
        let Some(expires_at) = self.next_expiring_available_epoch(now) else {
            return count;
        };
        format!("{count} · Next expires {}", expiry_label(expires_at, now))
    }

    pub(crate) fn next_expiring_available_epoch(&self, now: i64) -> Option<i64> {
        self.credits
            .iter()
            .filter(|credit| credit.status.as_deref() == Some("available"))
            .filter_map(|credit| {
                credit
                    .expires_at
                    .as_deref()
                    .and_then(parse_iso_epoch)
                    .filter(|epoch| *epoch > now)
            })
            .min()
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct CodexResetCredit {
    pub(crate) status: Option<String>,
    #[serde(rename = "expires_at")]
    pub(crate) expires_at: Option<String>,
}

pub(crate) fn push_codex_window(
    buckets: &mut Vec<QuotaBucketView>,
    label: &str,
    slot: Option<StatusSlot>,
    window: Option<&CodexWindowSnapshot>,
    now: i64,
) {
    let Some(window) = window else {
        return;
    };
    let used = window.used_percent_clamped();
    let remaining = used.map(|value| 100u8.saturating_sub(value));
    // The label carries the raw figure (`142% used` over cap); only the
    // remaining bar clamps. Negative garbage floors at 0, never "-3% used".
    let used_label = window
        .used_percent_raw()
        .map(|raw| codex_used_label(raw.max(0.0)));
    let window_seconds = window.window_seconds();
    let reset_at = window.resets_at(now);
    let pace = quota_pace_label(remaining, reset_at, window_seconds, now)
        .or_else(|| window.window_label());
    buckets.push(with_status_slot(
        timed_bucket(
            label,
            used_label,
            Some("100%".to_owned()),
            remaining,
            reset_at,
            now,
            pace.as_deref(),
            UsageSnapshotStatus::Fresh,
        ),
        slot,
    ));
}

/// Used-side label preserving the raw provider figure, including over-cap
/// readings (`142% used`) — the Muse lane renders the same form.
pub(crate) fn codex_used_label(used_percent: f64) -> String {
    if used_percent.fract() == 0.0 {
        format!("{used_percent:.0}% used")
    } else {
        format!("{used_percent:.1}% used")
    }
}
