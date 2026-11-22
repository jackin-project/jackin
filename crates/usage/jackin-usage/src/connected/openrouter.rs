// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-FileCopyrightText: 2026 Robin Ebers
// SPDX-License-Identifier: Apache-2.0 AND MIT

use jackin_usage_provider_core::{Auth, Http, ProviderError, json_number};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use time::{Date, Duration, OffsetDateTime, PlainDateTime, macros::format_description};

const INVALID: ProviderError = ProviderError::Decode("invalid OpenRouter usage response");
pub(super) async fn read(
    http: &Http,
    key: &str,
    now: i64,
    previous: Option<&Value>,
    manual: bool,
) -> Result<Value, ProviderError> {
    let body = http
        .get("https://openrouter.ai/api/v1/key", Auth::Bearer(key), &[])
        .await?;
    let data = &body["data"];
    let management = data["is_management_key"]
        .as_bool()
        .ok_or(ProviderError::Decode("invalid OpenRouter key response"))?;
    let mut resources = serde_json::Map::new();
    for (id, unit, bound, used) in [
        (
            "keyLimit",
            "usd",
            json_number(&data["limit"]),
            json_number(&data["limit"])
                .zip(json_number(&data["limit_remaining"]))
                .map(|(l, r)| (l - r.max(0.0)).max(0.0)),
        ),
        (
            "budget",
            "usd",
            json_number(&data["effective_budget"]["limit_usd"]),
            json_number(&data["effective_budget"]["spend_usd"]),
        ),
        (
            "freeRequests",
            "requests",
            json_number(&data["free_model_daily_requests"]["limit"]),
            json_number(&data["free_model_daily_requests"]["used"]),
        ),
    ] {
        if let Some(limit) = bound.filter(|n| *n > 0.0) {
            let used = used
                .ok_or(ProviderError::Decode("missing OpenRouter consumption"))?
                .max(0.0);
            resources.insert(id.into(),json!({"kind":"consumption","unit":unit,"used":used,"limit":limit,"remaining":(limit-used).max(0.0)}));
        }
    }
    if management {
        match http
            .get(
                "https://openrouter.ai/api/v1/credits",
                Auth::Bearer(key),
                &[],
            )
            .await
        {
            Ok(balance) => {
                let (credits, usage) = json_number(&balance["data"]["total_credits"])
                    .zip(json_number(&balance["data"]["total_usage"]))
                    .ok_or(ProviderError::Decode("invalid OpenRouter credits response"))?;
                resources.insert("balance".into(),json!({"kind":"balance","unit":"usd","available":(credits.max(0.0)-usage.max(0.0)).max(0.0)}));
            }
            Err(ProviderError::Forbidden) => {}
            Err(err) => return Err(err),
        }
    }
    let today = OffsetDateTime::from_unix_timestamp(now)
        .map_err(|_| INVALID)?
        .date();
    let spend = if management {
        listed_spend(http, key).await?
    } else {
        periods(data)?
    };
    let history = if management {
        Some(activity(http, key, today, now, previous, manual).await)
    } else {
        None
    };
    for (name, interval) in [
        ("freeRequests", Some("daily")),
        ("keyLimit", data["limit_reset"].as_str()),
        (
            "budget",
            data["effective_budget"]["reset_interval"].as_str(),
        ),
    ] {
        if let Some(res) = resources.get_mut(name)
            && let Some((start, end)) = interval.and_then(|i| bounds(today, i))
        {
            res["resets_at"] = json!(end);
            res["window_seconds"] = json!(end - start);
        }
    }
    Ok(
        json!({"plan":data["is_free_tier"].as_bool().map(|free|if free{"Free"}else{"Pay-as-you-go"}),"key_type":if management{"management"}else{"standard"},"resources":resources,"period_spend":spend,"activity":history}),
    )
}

fn bounds(today: Date, interval: &str) -> Option<(i64, i64)> {
    let (start, end) = match interval {
        "daily" => (today, today.next_day()?),
        "weekly" => {
            let start = today.checked_sub(Duration::days(i64::from(
                today.weekday().number_days_from_monday(),
            )))?;
            (start, start.checked_add(Duration::days(7))?)
        }
        "monthly" => {
            let start = today.replace_day(1).ok()?;
            (
                start,
                start.checked_add(Duration::days(i64::from(
                    start.month().length(start.year()),
                )))?,
            )
        }
        _ => return None,
    };
    Some((
        start.midnight().assume_utc().unix_timestamp(),
        end.midnight().assume_utc().unix_timestamp(),
    ))
}

type Measures = BTreeMap<String, f64>;
type Periods = BTreeMap<String, Measures>;

fn add(into: &mut Measures, row: &Measures) {
    for (name, amount) in row {
        *into.entry(name.clone()).or_default() += amount;
    }
}

fn periods(key: &Value) -> Result<Periods, ProviderError> {
    let mut result = Periods::new();
    for (period, field) in [
        ("today", "daily"),
        ("thisWeek", "weekly"),
        ("thisMonth", "monthly"),
    ] {
        let spend = json_number(&key[format!("usage_{field}")])
            .ok_or(INVALID)?
            .max(0.0);
        let mut amounts = Measures::from([("spend".into(), spend)]);
        if let Some(byok) = json_number(&key[format!("byok_usage_{field}")]) {
            amounts.insert("byokSpend".into(), byok.max(0.0));
        }
        result.insert(period.into(), amounts);
    }
    Ok(result)
}

async fn listed_spend(http: &Http, key: &str) -> Result<Periods, ProviderError> {
    let mut totals = Periods::new();
    for page in 0..50 {
        let address = format!(
            "https://openrouter.ai/api/v1/keys?include_disabled=true&include_expired=true&offset={}",
            page * 100
        );
        let data = http.get(&address, Auth::Bearer(key), &[]).await?;
        let rows = data["data"].as_array().ok_or(INVALID)?;
        for row in rows {
            for (period, amounts) in periods(row)? {
                add(totals.entry(period).or_default(), &amounts);
            }
        }
        if rows.len() < 100 {
            return Ok(totals);
        }
    }
    Err(INVALID)
}

fn activity_day(date: &str) -> Option<Date> {
    Date::parse(date, format_description!("[year]-[month padding:none]-[day padding:none]"))
        .ok()
        .or_else(|| {
            PlainDateTime::parse(
                date,
                format_description!("[year]-[month padding:none]-[day padding:none] [hour padding:none]:[minute padding:none]:[second padding:none]"),
            )
            .ok()
            .map(PlainDateTime::date)
        })
}

fn amounts(row: &Value) -> Option<Measures> {
    let mut result = Measures::new();
    for (name, field) in [
        ("spend", "usage"),
        ("byokSpend", "byok_usage_inference"),
        ("inputTokens", "prompt_tokens"),
        ("cacheReadTokens", "cached_tokens"),
        ("outputTokens", "completion_tokens"),
        ("reasoningTokens", "reasoning_tokens"),
    ] {
        if !row[field].is_null() {
            result.insert(
                name.into(),
                json_number(&row[field]).filter(|n| n.is_finite() && *n >= 0.0)?,
            );
        }
    }
    result.get("spend")?;
    let input = *result.get("inputTokens")?;
    let output = *result.get("outputTokens")?;
    result.insert("tokens".into(), input + output);
    result.insert(
        "inputTokens".into(),
        input - result.get("cacheReadTokens").copied().unwrap_or_default(),
    );
    result.insert(
        "outputTokens".into(),
        output - result.get("reasoningTokens").copied().unwrap_or_default(),
    );
    Some(result)
}

type ActivityDays = BTreeMap<String, (Measures, BTreeMap<String, Measures>)>;

fn parse_activity(data: &Value) -> Option<ActivityDays> {
    let mut days = BTreeMap::<String, (Measures, BTreeMap<String, Measures>)>::new();
    for row in data["data"].as_array()? {
        let day = activity_day(row["date"].as_str()?)?.to_string();
        let model = row["model"].as_str()?;
        let measures = amounts(row)?;
        let (total, models) = days.entry(day).or_default();
        add(total, &measures);
        add(models.entry(model.into()).or_default(), &measures);
    }
    Some(days)
}

async fn activity(
    http: &Http,
    key: &str,
    today: Date,
    now: i64,
    previous: Option<&Value>,
    manual: bool,
) -> Value {
    let span: Vec<_> = (1..=30)
        .filter_map(|d| today.checked_sub(Duration::days(d)))
        .map(|d| d.to_string())
        .collect();
    let prior = previous
        .and_then(|v| v.get("activity"))
        .unwrap_or(&Value::Null);
    let mut days = prior["days"].as_object().cloned().unwrap_or_default();
    days.retain(|day, _| span.contains(day));
    if !manual
        && prior["as_of"] == today.to_string()
        && prior["read_at"]
            .as_i64()
            .is_some_and(|at| now < at.saturating_add(3600))
    {
        return prior.clone();
    }
    let due: Vec<_> = span
        .iter()
        .enumerate()
        .filter(|(i, d)| *i == 0 || !days.contains_key(*d))
        .map(|(_, d)| d)
        .collect();
    let mut url = "https://openrouter.ai/api/v1/activity".to_owned();
    if let [date] = due.as_slice() {
        url.push_str(&format!("?date={date}"));
    }
    let data = http
        .get(&url, Auth::Bearer(key), &[])
        .await
        .ok()
        .and_then(|v| parse_activity(&v));
    for date in &due {
        days.remove(*date);
        if let Some(data) = &data {
            let payload=data.get(*date).map_or_else(||json!({"totals":{"spend":0,"byokSpend":0,"tokens":0,"inputTokens":0,"cacheReadTokens":0,"outputTokens":0,"reasoningTokens":0},"by_model":{}}),
                |(totals,models)|json!({"totals":totals,"by_model":models}));
            days.insert((*date).clone(), payload);
        }
    }
    let mut totals = Measures::new();
    for day in days.values() {
        if let Some(row) = day["totals"].as_object() {
            for (name, n) in row {
                if let Some(n) = n.as_f64() {
                    *totals.entry(name.clone()).or_default() += n;
                }
            }
        }
    }
    let yesterday = span.first().and_then(|d| days.get(d));
    json!({"as_of":today.to_string(),"read_at":now,"expected_days":span.len(),"observed_days":days.len(),"prior30Days":if days.is_empty(){None}else{Some(totals)},"yesterday":yesterday,"days":days})
}
