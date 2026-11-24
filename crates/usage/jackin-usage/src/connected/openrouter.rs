// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-FileCopyrightText: 2026 Robin Ebers
// SPDX-License-Identifier: Apache-2.0 AND MIT

use jackin_usage_provider_core::{Auth, Decimal, Exact, Http, ProviderError};
use serde_json::{Map, Value, json};
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
        .get_exact("https://openrouter.ai/api/v1/key", Auth::Bearer(key), &[])
        .await?;
    let data = &body["data"];
    let management = data["is_management_key"]
        .as_bool()
        .ok_or(ProviderError::Decode("invalid OpenRouter key response"))?;
    let mut resources = Map::new();
    let limit = data["limit"].decimal()?;
    let key_used = match (limit, data["limit_remaining"].decimal()?) {
        (Some(limit), Some(left)) => Some(
            limit
                .checked_sub(&left.clamped_at_zero())
                .ok_or(ProviderError::OUT_OF_RANGE)?
                .clamped_at_zero(),
        ),
        _ => None,
    };
    let budget = &data["effective_budget"];
    let free = &data["free_model_daily_requests"];
    for (id, unit, bound, used) in [
        ("keyLimit", "usd", limit, key_used),
        (
            "budget",
            "usd",
            budget["limit_usd"].decimal()?,
            budget["spend_usd"].decimal()?,
        ),
        (
            "freeRequests",
            "requests",
            free["limit"].decimal()?,
            free["used"].decimal()?,
        ),
    ] {
        if let Some(limit) = bound.filter(Decimal::is_positive) {
            let used = used
                .ok_or(ProviderError::Decode("missing OpenRouter consumption"))?
                .clamped_at_zero();
            let remaining = limit
                .checked_sub(&used)
                .ok_or(ProviderError::OUT_OF_RANGE)?
                .clamped_at_zero();
            resources.insert(id.into(),json!({"kind":"consumption","unit":unit,"used":quantity(unit,&used),"limit":quantity(unit,&limit),"remaining":quantity(unit,&remaining)}));
        }
    }
    if management {
        match http
            .get_exact(
                "https://openrouter.ai/api/v1/credits",
                Auth::Bearer(key),
                &[],
            )
            .await
        {
            Ok(balance) => {
                let (credits, usage) = balance["data"]["total_credits"]
                    .decimal()?
                    .zip(balance["data"]["total_usage"].decimal()?)
                    .ok_or(ProviderError::Decode("invalid OpenRouter credits response"))?;
                let available = credits
                    .clamped_at_zero()
                    .checked_sub(&usage.clamped_at_zero())
                    .ok_or(ProviderError::OUT_OF_RANGE)?
                    .clamped_at_zero();
                resources.insert(
                    "balance".into(),
                    json!({"kind":"balance","unit":"usd","available":quantity("usd",&available)}),
                );
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
        json!({"plan":data["is_free_tier"].as_bool().map(|free|if free{"Free"}else{"Pay-as-you-go"}),"key_type":if management{"management"}else{"standard"},"resources":resources,"period_spend":periods_json(&spend),"activity":history}),
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

type Measures = BTreeMap<String, Decimal>;
type Periods = BTreeMap<String, Measures>;

const MONEY: [&str; 2] = ["spend", "byokSpend"];

fn quantity(unit: &str, amount: &Decimal) -> Value {
    if unit == "usd" {
        Value::String(amount.to_string())
    } else {
        json!(amount.to_f64())
    }
}

fn measures_json(measures: &Measures) -> Value {
    Value::Object(
        measures
            .iter()
            .map(|(name, amount)| {
                let unit = if MONEY.contains(&name.as_str()) {
                    "usd"
                } else {
                    "count"
                };
                (name.clone(), quantity(unit, amount))
            })
            .collect(),
    )
}

fn periods_json(periods: &Periods) -> Value {
    Value::Object(
        periods
            .iter()
            .map(|(period, measures)| (period.clone(), measures_json(measures)))
            .collect(),
    )
}

fn add(into: &mut Measures, row: &Measures) -> Result<(), ProviderError> {
    for (name, amount) in row {
        let total = match into.get(name) {
            Some(total) => total
                .checked_add(amount)
                .ok_or(ProviderError::OUT_OF_RANGE)?,
            None => *amount,
        };
        into.insert(name.clone(), total);
    }
    Ok(())
}

fn periods(key: &Exact) -> Result<Periods, ProviderError> {
    let mut result = Periods::new();
    for (period, field) in [
        ("today", "daily"),
        ("thisWeek", "weekly"),
        ("thisMonth", "monthly"),
    ] {
        let spend = key[format!("usage_{field}").as_str()]
            .decimal()?
            .ok_or(INVALID)?
            .clamped_at_zero();
        let mut amounts = Measures::from([("spend".into(), spend)]);
        if let Some(byok) = key[format!("byok_usage_{field}").as_str()].decimal()? {
            amounts.insert("byokSpend".into(), byok.clamped_at_zero());
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
        let data = http.get_exact(&address, Auth::Bearer(key), &[]).await?;
        let rows = data["data"].as_list().ok_or(INVALID)?;
        for row in rows {
            for (period, amounts) in periods(row)? {
                add(totals.entry(period).or_default(), &amounts)?;
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

fn amounts(row: &Exact) -> Option<Measures> {
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
            let amount = row[field].decimal().ok()??;
            if amount.is_negative() {
                return None;
            }
            result.insert(name.into(), amount);
        }
    }
    result.get("spend")?;
    let input = *result.get("inputTokens")?;
    let output = *result.get("outputTokens")?;
    let cached = result.get("cacheReadTokens").copied().unwrap_or_default();
    let reasoning = result.get("reasoningTokens").copied().unwrap_or_default();
    result.insert("tokens".into(), input.checked_add(&output)?);
    result.insert("inputTokens".into(), input.checked_sub(&cached)?);
    result.insert("outputTokens".into(), output.checked_sub(&reasoning)?);
    Some(result)
}

type ActivityDays = BTreeMap<String, (Measures, BTreeMap<String, Measures>)>;

fn parse_activity(data: &Exact) -> Option<ActivityDays> {
    let mut days = BTreeMap::<String, (Measures, BTreeMap<String, Measures>)>::new();
    for row in data["data"].as_list()? {
        let day = activity_day(row["date"].as_str()?)?.to_string();
        let model = row["model"].as_str()?;
        let measures = amounts(row)?;
        let (total, models) = days.entry(day).or_default();
        add(total, &measures).ok()?;
        add(models.entry(model.into()).or_default(), &measures).ok()?;
    }
    Some(days)
}

fn exact_amounts(measures: &Value) -> bool {
    let readable = |field: &Value| {
        field
            .as_str()
            .is_some_and(|text| Decimal::parse_text(text).is_ok())
    };
    measures.as_object().is_some_and(|fields| {
        fields.get("spend").is_some_and(readable) && fields.get("byokSpend").is_none_or(readable)
    })
}

fn exact_day(day: &Value) -> bool {
    exact_amounts(&day["totals"])
        && day["by_model"]
            .as_object()
            .is_some_and(|models| models.values().all(exact_amounts))
}

fn total_of(days: &Map<String, Value>) -> Option<Measures> {
    let mut totals = Measures::new();
    for day in days.values() {
        for (name, value) in day["totals"].as_object()? {
            let amount = Decimal::from_json(value).ok()?;
            let sum = match totals.get(name) {
                Some(total) => total.checked_add(&amount)?,
                None => amount,
            };
            totals.insert(name.clone(), sum);
        }
    }
    Some(totals)
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
    days.retain(|day, stored| span.contains(day) && exact_day(stored));
    let stored_exactly = prior["days"]
        .as_object()
        .is_none_or(|stored| stored.values().all(exact_day))
        && (prior["yesterday"].is_null() || exact_day(&prior["yesterday"]))
        && (prior["prior30Days"].is_null() || exact_amounts(&prior["prior30Days"]));
    if !manual
        && stored_exactly
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
        .get_exact(&url, Auth::Bearer(key), &[])
        .await
        .ok()
        .and_then(|v| parse_activity(&v));
    for date in &due {
        days.remove(*date);
        if let Some(data) = &data {
            let payload=data.get(*date).map_or_else(||json!({"totals":{"spend":"0","byokSpend":"0","tokens":0,"inputTokens":0,"cacheReadTokens":0,"outputTokens":0,"reasoningTokens":0},"by_model":{}}),
                |(totals,models)|json!({"totals":measures_json(totals),"by_model":models.iter().map(|(name,measures)|(name.clone(),measures_json(measures))).collect::<Map<_,_>>()}));
            days.insert((*date).clone(), payload);
        }
    }
    let totals = if days.is_empty() {
        None
    } else {
        total_of(&days).map(|totals| measures_json(&totals))
    };
    let yesterday = span.first().and_then(|d| days.get(d));
    json!({"as_of":today.to_string(),"read_at":now,"expected_days":span.len(),"observed_days":days.len(),"prior30Days":totals,"yesterday":yesterday,"days":days})
}
