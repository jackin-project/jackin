// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::control::{
    CanonicalRemainingPercent, Money, PercentObservationOrigin, QuotaBucketView, StatusSlot,
    UsageSnapshotStatus,
};
use jackin_usage_provider_core::{
    bucket, epoch_seconds_from_maybe_ms, expiry_label, format_cents, parse_iso_epoch, timed_bucket,
};

use super::{OpenRouterCreditsOutcome, OpenRouterCreditsResponse, OpenRouterKeyData};

fn openrouter_cents(dollars: f64) -> Option<i64> {
    if !dollars.is_finite() {
        return None;
    }
    let cents = (dollars * 100.0).round();
    if !cents.is_finite() || cents.abs() >= 9_007_199_254_740_992.0 {
        return None;
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "range-checked below 2^53, exactly representable"
    )]
    Some(cents as i64)
}

fn openrouter_byok_spend(data: &serde_json::Value) -> Option<i64> {
    let object = data.as_object()?;
    for key in [
        "byok_usage_monthly",
        "byok_usage_weekly",
        "byok_usage_daily",
        "byok_usage",
    ] {
        if let Some(cents) = object
            .get(key)
            .and_then(serde_json::Value::as_f64)
            .and_then(openrouter_cents)
            .filter(|cents| *cents >= 0)
        {
            return Some(cents);
        }
    }
    let mut total: i64 = 0;
    let mut found = false;
    for (key, value) in object {
        let lower = key.to_ascii_lowercase();
        if lower.contains("byok")
            && (lower.contains("usage") || lower.contains("spend") || lower.contains("cost"))
            && let Some(cents) = value.as_f64().and_then(openrouter_cents)
            && cents >= 0
            && let Some(sum) = total.checked_add(cents)
        {
            total = sum;
            found = true;
        }
    }
    found.then_some(total)
}

fn openrouter_key_expiry(data: &serde_json::Value, now: i64) -> Option<String> {
    let raw = data.get("expires_at")?;
    let epoch = raw
        .as_str()
        .and_then(parse_iso_epoch)
        .or_else(|| raw.as_i64().map(epoch_seconds_from_maybe_ms))
        .or_else(|| {
            raw.as_f64().and_then(|value| {
                value.is_finite().then(|| {
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "epoch millis fit i64 for any plausible date"
                    )]
                    {
                        epoch_seconds_from_maybe_ms(value.round() as i64)
                    }
                })
            })
        })?;
    Some(format!("expires {}", expiry_label(epoch, now)))
}

fn openrouter_key_reset(data: &serde_json::Value) -> Option<i64> {
    data.get("reset_at")
        .or_else(|| data.get("resets_at"))
        .and_then(serde_json::Value::as_str)
        .and_then(parse_iso_epoch)
}

#[derive(Debug)]
pub struct OpenRouterKeyQuota {
    pub(crate) buckets: Vec<QuotaBucketView>,
    pub(crate) plan_label: Option<String>,
}

pub fn parse_openrouter_key_usage(
    value: serde_json::Value,
    now: i64,
) -> Result<OpenRouterKeyQuota, String> {
    let data = value
        .get("data")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let key: OpenRouterKeyData = serde_json::from_value(data.clone())
        .map_err(|_| "OpenRouter key response is malformed".to_owned())?;
    let mut buckets = Vec::new();
    if let Some(cap) = key.limit.and_then(openrouter_cents).filter(|cap| *cap > 0) {
        let remaining = key
            .limit_remaining
            .and_then(openrouter_cents)
            .filter(|remaining| *remaining >= 0);
        let used = remaining.map(|remaining| cap.saturating_sub(remaining).max(0));
        let remaining_percent = remaining.and_then(|remaining| {
            #[expect(
                clippy::cast_precision_loss,
                reason = "continuous ratio is derived from exact cent amounts"
            )]
            let points = (i128::from(remaining) * 100) as f64 / i128::from(cap) as f64;
            CanonicalRemainingPercent::new(points)
                .ok()
                .map(|percent| (percent, PercentObservationOrigin::Derived))
        });
        let mut view = timed_bucket(
            "Key Limit",
            used.map(format_cents),
            Some(format_cents(cap)),
            remaining_percent,
            openrouter_key_reset(&data),
            now,
            openrouter_key_expiry(&data, now).as_deref(),
            UsageSnapshotStatus::Fresh,
        );
        view.used_money = used.map(|used| Money::new(used, "USD", 2));
        view.limit_money = Some(Money::new(cap, "USD", 2));
        view.status_slot = Some(StatusSlot::Spend);
        buckets.push(view);
    }
    for (label, spend) in [
        ("Spent today", key.usage_daily),
        ("Spent this week", key.usage_weekly),
        ("Spent this month", key.usage_monthly),
    ] {
        if let Some(cents) = spend.and_then(openrouter_cents).filter(|cents| *cents >= 0) {
            let mut view = bucket(
                label,
                Some(format_cents(cents)),
                None,
                None,
                None,
                None,
                UsageSnapshotStatus::Fresh,
            );
            view.used_money = Some(Money::new(cents, "USD", 2));
            buckets.push(view);
        }
    }
    if let Some(byok) = openrouter_byok_spend(&data) {
        let mut view = bucket(
            "BYOK spend",
            Some(format_cents(byok)),
            None,
            None,
            None,
            Some("billed to your own key"),
            UsageSnapshotStatus::Fresh,
        );
        view.used_money = Some(Money::new(byok, "USD", 2));
        buckets.push(view);
    }
    if buckets.iter().all(|bucket| bucket.status_slot.is_none())
        && let Some(monthly) = buckets
            .iter_mut()
            .find(|bucket| bucket.label == "Spent this month")
    {
        monthly.status_slot = Some(StatusSlot::Spend);
    }
    Ok(OpenRouterKeyQuota {
        buckets,
        plan_label: match key.is_free_tier {
            Some(true) => Some("Free tier".to_owned()),
            Some(false) => Some("Pay as you go".to_owned()),
            None => None,
        },
    })
}

pub fn parse_openrouter_credits(
    value: serde_json::Value,
) -> Result<OpenRouterCreditsOutcome, String> {
    let response: OpenRouterCreditsResponse = serde_json::from_value(value)
        .map_err(|_| "OpenRouter credits response is malformed".to_owned())?;
    let ceiling = openrouter_cents(response.data.total_credits)
        .filter(|ceiling| *ceiling >= 0)
        .ok_or_else(|| "OpenRouter total credits are invalid".to_owned())?;
    let spent = openrouter_cents(response.data.total_usage)
        .filter(|spent| *spent >= 0)
        .ok_or_else(|| "OpenRouter total usage is invalid".to_owned())?;
    Ok(OpenRouterCreditsOutcome::Available {
        spent_cents: spent,
        ceiling_cents: ceiling,
    })
}

pub fn openrouter_credits_bucket(spent_cents: i64, ceiling_cents: i64) -> QuotaBucketView {
    let remaining_percent = if ceiling_cents > 0 {
        let remaining_cents = i128::from(ceiling_cents) - i128::from(spent_cents);
        if remaining_cents >= 0 {
            #[expect(
                clippy::cast_precision_loss,
                reason = "continuous ratio is derived from exact cent amounts"
            )]
            let points = (remaining_cents * 100) as f64 / i128::from(ceiling_cents) as f64;
            CanonicalRemainingPercent::new(points)
                .ok()
                .map(|percent| (percent, PercentObservationOrigin::Derived))
        } else {
            None
        }
    } else {
        None
    };
    let mut view = bucket(
        "Account credits",
        Some(format_cents(spent_cents)),
        Some(format_cents(ceiling_cents)),
        remaining_percent,
        None,
        None,
        UsageSnapshotStatus::Fresh,
    );
    view.used_money = Some(Money::new(spent_cents, "USD", 2));
    view.limit_money = Some(Money::new(ceiling_cents, "USD", 2));
    view
}
