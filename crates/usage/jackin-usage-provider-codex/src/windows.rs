// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_usage_provider_core::{
    Bucket, BucketKind, Money, Remaining, json_integer, json_money, json_number, json_text,
    title_words,
};
use serde_json::Value;

pub(crate) fn buckets(body: &Value, now: i64) -> Vec<Bucket> {
    let mut out = Vec::new();
    if let Some(limit) = body.get("rate_limit") {
        out.extend(pair(limit, None, true, now));
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
            out.extend(pair(limit, Some(&name), false, now));
        }
    }
    out.extend(credits(body.get("credits")));
    out.extend(individual(
        body.pointer("/spend_control/individual_limit"),
        now,
    ));
    out
}

fn pair(limit: &Value, prefix: Option<&str>, by_position: bool, now: i64) -> Vec<Bucket> {
    let slots = [BucketKind::FiveHour, BucketKind::Weekly];
    ["primary_window", "secondary_window"]
        .into_iter()
        .zip(slots)
        .filter_map(|(key, slot)| {
            let window = limit.get(key).filter(|window| window.is_object())?;
            Some(window_bucket(
                window,
                prefix,
                by_position.then_some(slot),
                now,
            ))
        })
        .collect()
}

fn window_bucket(
    window: &Value,
    prefix: Option<&str>,
    slot: Option<BucketKind>,
    now: i64,
) -> Bucket {
    let seconds = window.get("limit_window_seconds").and_then(json_integer);
    let slot = if seconds.is_some() { None } else { slot };
    Bucket::timed(prefix, seconds, slot, prefix.unwrap_or("Codex"))
        .used(window.get("used_percent").and_then(json_number))
        .resets(reset_at(window, now))
}

fn reset_at(window: &Value, now: i64) -> Option<i64> {
    window.get("reset_at").and_then(json_integer).or_else(|| {
        let after = window.get("reset_after_seconds").and_then(json_integer)?;
        (after >= 0).then(|| now.saturating_add(after))
    })
}

fn credits(credits: Option<&Value>) -> Option<Bucket> {
    let credits = credits?;
    if credits.get("has_credits").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let balance = credits
        .get("balance")
        .and_then(|value| json_money(value, 2, "credits"))?;
    Some(Bucket::named(BucketKind::Credits, "credits").left(Some(Remaining::Amount(balance))))
}

fn individual(limit: Option<&Value>, now: i64) -> Option<Bucket> {
    let limit = limit?;
    let money = |key: &str| limit.get(key).and_then(|value| json_money(value, 2, "USD"));
    let remaining = match (money("remaining"), money("limit"), money("used")) {
        (Some(left), _, _) => Some(Remaining::Amount(left.magnitude())),
        (None, Some(cap), Some(used)) => Some(Remaining::Amount(Money::minus(&cap, &used))),
        _ => limit
            .get("remaining_percent")
            .and_then(json_number)
            .map(Remaining::Percent),
    };
    Some(
        Bucket::named(BucketKind::Spend, "Individual limit")
            .used(limit.get("used_percent").and_then(json_number))
            .left(remaining)
            .resets(reset_at(limit, now)),
    )
}
