// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `OpenRouter` key usage and credit parsing plus credit buckets.

use super::super::{
    Money, QuotaBucketView, StatusSlot, UsageSnapshotStatus, bucket, epoch_seconds_from_maybe_ms,
    expiry_label, format_cents, parse_iso_epoch, timed_bucket,
};

use super::{OpenRouterCreditsOutcome, OpenRouterCreditsResponse, OpenRouterKeyData};

/// Exact dollars-to-cents conversion behind every `OpenRouter` money row.
/// Non-finite or out-of-range values are invalid (`None`), never clamped.
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

/// BYOK attribution stays a separate row: the first available BYOK spend field
/// (monthly, then weekly, daily, plain), or the sum of any other `byok*`
/// *spend* numerics. The fallback only counts keys that also name usage or
/// spend — a `byok_limit`/`byok_remaining` cap must never be read as spend.
/// Never merged into the key usage rows.
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

/// Optional key expiry (`expires_at` as ISO string or epoch number, seconds or
/// milliseconds). Unparseable values are ignored, never fatal.
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

/// Optional key-cap reset timestamp (`reset_at`/`resets_at` ISO). Absent on
/// most keys — the Key Limit row then carries no reset.
fn openrouter_key_reset(data: &serde_json::Value) -> Option<i64> {
    data.get("reset_at")
        .or_else(|| data.get("resets_at"))
        .and_then(serde_json::Value::as_str)
        .and_then(parse_iso_epoch)
}

#[derive(Debug)]
pub(crate) struct OpenRouterKeyQuota {
    pub(crate) buckets: Vec<QuotaBucketView>,
    pub(crate) plan_label: Option<String>,
}

pub(crate) fn parse_openrouter_key_usage(
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
    // Key Limit meter: coherent cap/remaining only. A null (or non-positive)
    // cap means no configured cap — no row at all, never an infinite bar.
    if let Some(cap) = key.limit.and_then(openrouter_cents).filter(|cap| *cap > 0) {
        let remaining = key
            .limit_remaining
            .and_then(openrouter_cents)
            .filter(|remaining| *remaining >= 0);
        let used = remaining.map(|remaining| cap.saturating_sub(remaining).max(0));
        let remaining_percent = remaining.map(|remaining| {
            #[expect(clippy::cast_precision_loss, reason = "cents magnitudes fit f64")]
            let fraction = (remaining.min(cap) as f64) / (cap as f64);
            #[expect(clippy::cast_sign_loss, reason = "clamped 0.0..=100.0 before cast")]
            {
                (fraction * 100.0).round().clamp(0.0, 100.0) as u8
            }
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
    // Period spend rows carry no denominator, so they never draw percentages.
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
    // Without a cap the monthly spend row feeds the status-bar money headline.
    if buckets.iter().all(|bucket| bucket.status_slot.is_none())
        && let Some(monthly) = buckets
            .iter_mut()
            .find(|bucket| bucket.label == "Spent this month")
    {
        monthly.status_slot = Some(StatusSlot::Spend);
    }
    Ok(OpenRouterKeyQuota {
        buckets,
        // An omitted tier flag is unknown, never evidence of PAYG: only an
        // explicit `false` earns the paid label.
        plan_label: match key.is_free_tier {
            Some(true) => Some("Free tier".to_owned()),
            Some(false) => Some("Pay as you go".to_owned()),
            None => None,
        },
    })
}

pub(crate) fn parse_openrouter_credits(
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

/// Account-credits row: the meter shows remaining balance, while the spent
/// fields retain the provider's `total_usage`. A real zero balance renders as
/// `$0` remaining (never "No data"). The percentage meter exists only when the
/// ceiling is positive.
pub(crate) fn openrouter_credits_bucket(spent_cents: i64, ceiling_cents: i64) -> QuotaBucketView {
    let balance = ceiling_cents.saturating_sub(spent_cents);
    let overage = ceiling_cents > 0 && spent_cents > ceiling_cents;
    let remaining_percent = (ceiling_cents > 0 && !overage).then(|| {
        #[expect(clippy::cast_precision_loss, reason = "cents magnitudes fit f64")]
        let fraction = (balance.max(0).min(ceiling_cents) as f64) / (ceiling_cents as f64);
        #[expect(clippy::cast_sign_loss, reason = "clamped 0.0..=100.0 before cast")]
        {
            (fraction * 100.0).round().clamp(0.0, 100.0) as u8
        }
    });
    let mut view = bucket(
        "Account credits",
        Some(format_cents(spent_cents)),
        Some(format_cents(ceiling_cents)),
        remaining_percent,
        None,
        None,
        UsageSnapshotStatus::Fresh,
    );
    if overage {
        let raw_used = spent_cents
            .saturating_mul(100)
            .checked_div(ceiling_cents)
            .unwrap_or(i64::MAX);
        view.used_label = Some(format!("{raw_used}% used"));
    }
    view.used_money = Some(Money::new(spent_cents, "USD", 2));
    view.limit_money = Some(Money::new(ceiling_cents, "USD", 2));
    view
}
