// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_usage_provider_core::{
    Bucket, BucketKind, ProviderError, Remaining, json_integer, json_money, json_number, json_text,
    title_words,
};
use serde_json::Value;

pub(crate) fn buckets(body: &Value, now: i64) -> Result<Vec<Bucket>, ProviderError> {
    let mut out = Vec::new();
    if let Some(limit) = body.get("rate_limit") {
        out.extend(pair(limit, None, true, now)?);
    }
    let extras = body
        .get("additional_rate_limits")
        .and_then(Value::as_array)
        .into_iter()
        .flatten();
    for extra in extras {
        let name = extra
            .get("limit_name")
            .and_then(json_text)
            .or_else(|| extra.get("metered_feature").and_then(json_text))
            .map_or_else(|| "Codex extra".to_owned(), title_words);
        if let Some(limit) = extra.get("rate_limit") {
            out.extend(pair(limit, Some(&name), false, now)?);
        }
    }
    out.extend(credits(body.get("credits"))?);
    out.extend(individual(
        body.pointer("/spend_control/individual_limit"),
        now,
    )?);
    Ok(out)
}

fn pair(
    limit: &Value,
    prefix: Option<&str>,
    by_position: bool,
    now: i64,
) -> Result<Vec<Bucket>, ProviderError> {
    let slots = [BucketKind::FiveHour, BucketKind::Weekly];
    let mut out = Vec::new();
    for (key, slot) in ["primary_window", "secondary_window"]
        .into_iter()
        .zip(slots)
    {
        if let Some(window) = limit.get(key).filter(|window| window.is_object()) {
            out.push(window_bucket(
                window,
                prefix,
                by_position.then_some(slot),
                now,
            )?);
        }
    }
    Ok(out)
}

fn window_bucket(
    window: &Value,
    prefix: Option<&str>,
    slot: Option<BucketKind>,
    now: i64,
) -> Result<Bucket, ProviderError> {
    let seconds = window.get("limit_window_seconds").and_then(json_integer);
    let slot = if seconds.is_some() { None } else { slot };
    Ok(
        Bucket::timed(prefix, seconds, slot, prefix.unwrap_or("Codex"))
            .used(window.get("used_percent").and_then(json_number))
            .resets(reset_at(window, now)?),
    )
}

fn reset_at(window: &Value, now: i64) -> Result<Option<i64>, ProviderError> {
    if let Some(at) = window.get("reset_at").and_then(json_integer) {
        return Ok(Some(at));
    }
    let Some(after) = window
        .get("reset_after_seconds")
        .and_then(json_integer)
        .filter(|after| *after >= 0)
    else {
        return Ok(None);
    };
    now.checked_add(after)
        .map(Some)
        .ok_or(ProviderError::OUT_OF_RANGE)
}

fn credits(credits: Option<&Value>) -> Result<Option<Bucket>, ProviderError> {
    let Some(credits) = credits else {
        return Ok(None);
    };
    if credits.get("has_credits").and_then(Value::as_bool) != Some(true) {
        return Ok(None);
    }
    let Some(balance) = credits.get("balance") else {
        return Ok(None);
    };
    Ok(json_money(balance, 2, "credits")?.map(|balance| {
        Bucket::named(BucketKind::Credits, "credits").left(Some(Remaining::Amount(balance)))
    }))
}

fn individual(limit: Option<&Value>, now: i64) -> Result<Option<Bucket>, ProviderError> {
    let Some(limit) = limit else {
        return Ok(None);
    };
    let money = |key: &str| match limit.get(key) {
        Some(value) => json_money(value, 2, "USD"),
        None => Ok(None),
    };
    let remaining = match (money("remaining")?, money("limit")?, money("used")?) {
        (Some(left), _, _) => Some(Remaining::Amount(left.clamped_at_zero())),
        (None, Some(cap), Some(used)) => Some(Remaining::Amount(
            cap.minus(&used)
                .ok_or(ProviderError::OUT_OF_RANGE)?
                .clamped_at_zero(),
        )),
        _ => limit
            .get("remaining_percent")
            .and_then(json_number)
            .map(Remaining::Percent),
    };
    Ok(Some(
        Bucket::named(BucketKind::Spend, "Individual limit")
            .used(limit.get("used_percent").and_then(json_number))
            .left(remaining)
            .resets(reset_at(limit, now)?),
    ))
}
