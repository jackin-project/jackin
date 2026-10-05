// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `OpenRouter` key/account usage snapshot.
//!
//! An ordinary inference credential reads only `GET {base}/key`: key cap,
//! remaining allowance and the account's UTC daily free-model request quota.
//! Account funds (`/credits`) and completed activity (`/activity`) require a
//! separately configured Management credential. The current credential
//! contract supplies only an inference key, so those routes are absent from
//! this collector and their data remains explicitly unavailable.
//! A null key cap means no configured cap, never unlimited account funds.
//! Public model catalog omissions remain unverified rather than rejected.

use super::refresh::{ProviderError, ProviderRateLimit};
#[cfg_attr(
    not(test),
    expect(clippy::wildcard_imports, reason = "target-dependent")
)]
use super::*;
use serde::Deserialize;

#[path = "openrouter_money.rs"]
mod openrouter_money;
use openrouter_money::{MoneyField, MoneySum, SignedPolicy, parse_money_field};

pub(crate) const OPENROUTER_DEFAULT_BASE_URL: &str = "https://openrouter.ai/api/v1";

pub(crate) fn openrouter_base_url() -> String {
    openrouter_base_url_from(
        env_value("OPENROUTER_API_URL").as_deref(),
        env_value("OPENROUTER_BASE_URL").as_deref(),
    )
}

/// Pure base-URL resolution: `OPENROUTER_API_URL` wins over
/// `OPENROUTER_BASE_URL`, blank inputs fall through to the next candidate.
/// The hermetic seam tests use so live env can never vacate assertions.
pub(crate) fn openrouter_base_url_from(api_url: Option<&str>, base_url: Option<&str>) -> String {
    [api_url, base_url]
        .into_iter()
        .flatten()
        .map(|value| value.trim().trim_end_matches('/'))
        .find(|value| !value.is_empty())
        .map_or_else(|| OPENROUTER_DEFAULT_BASE_URL.to_owned(), str::to_owned)
}

#[derive(Debug, Deserialize, Default)]
struct OpenRouterKeyData {
    #[serde(default)]
    is_free_tier: Option<bool>,
    #[serde(default)]
    free_model_daily_requests: Option<OpenRouterFreeModelQuota>,
}

/// The account's free-model daily policy; exempt accounts, endpoints and
/// BYOK requests are not constrained by this ceiling.
#[derive(Debug, Deserialize)]
struct OpenRouterFreeModelQuota {
    used: Option<u64>,
    limit: Option<u64>,
    remaining: Option<u64>,
}

/// Model-ID check against a catalog snapshot. Stale omission is `Unverified`,
/// never a rejection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OpenRouterModelCheck {
    Verified { model_id: String },
    Unverified { model_id: String, reason: String },
}

pub(crate) fn check_openrouter_model_in_catalog(
    catalog: &serde_json::Value,
    model_id: &str,
) -> OpenRouterModelCheck {
    let found = catalog
        .get("data")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|models| {
            models.iter().any(|model| {
                model
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|id| id == model_id)
            })
        });
    if found {
        OpenRouterModelCheck::Verified {
            model_id: model_id.to_owned(),
        }
    } else {
        OpenRouterModelCheck::Unverified {
            model_id: model_id.to_owned(),
            reason: "not in catalog snapshot; unverified".to_owned(),
        }
    }
}

/// Preserve absent/null while rejecting any present invalid monetary field.
fn openrouter_money_field(
    data: &serde_json::Value,
    field: &str,
    policy: SignedPolicy,
) -> Result<MoneyField, String> {
    let parsed = parse_money_field(data.get(field), policy);
    if matches!(parsed, MoneyField::Invalid(_)) {
        // Only locally selected field names appear in this error. Never echo
        // provider-controlled field names or decimal spellings.
        return Err("OpenRouter key response contains an invalid monetary field".to_owned());
    }
    Ok(parsed)
}

fn known_money(field: MoneyField) -> Option<Money> {
    match field {
        MoneyField::Known(money) => Some(money),
        MoneyField::Unknown | MoneyField::Null | MoneyField::Invalid(_) => None,
    }
}

/// BYOK attribution stays separate. The first non-null primary spend field
/// owns attribution; an invalid selected primary never falls through.
/// Fallback sums are exact and complete, or remain unknown when a component
/// is null. Cap/remaining fields cannot become spend.
fn openrouter_byok_spend(data: &serde_json::Value) -> Result<Option<Money>, String> {
    const PRIMARY: [&str; 4] = [
        "byok_usage_monthly",
        "byok_usage_weekly",
        "byok_usage_daily",
        "byok_usage",
    ];
    let object = data
        .as_object()
        .ok_or_else(|| "OpenRouter key response is malformed".to_owned())?;
    for field in PRIMARY {
        let money = openrouter_money_field(data, field, SignedPolicy::NonNegative)?;
        if let Some(money) = known_money(money) {
            return Ok(Some(money));
        }
    }
    let mut total = MoneySum::new();
    let mut incomplete = false;
    for (field, value) in object {
        let lower = field.to_ascii_lowercase();
        if PRIMARY.contains(&field.as_str())
            || !lower.contains("byok")
            || !(lower.contains("usage") || lower.contains("spend") || lower.contains("cost"))
            || lower.contains("limit")
            || lower.contains("remaining")
        {
            continue;
        }
        match parse_money_field(Some(value), SignedPolicy::NonNegative) {
            MoneyField::Known(money) => {
                total.add(&money).map_err(|_| {
                    "OpenRouter BYOK spend total exceeds accumulator bounds".to_owned()
                })?;
            }
            MoneyField::Unknown | MoneyField::Null => incomplete = true,
            MoneyField::Invalid(_) => {
                return Err("OpenRouter BYOK spend contains an invalid monetary field".to_owned());
            }
        }
    }
    if incomplete {
        return Ok(None);
    }
    total
        .finish()
        .map_err(|_| "OpenRouter BYOK spend total is not exactly representable".to_owned())
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

/// Official recurring key caps reset at UTC midnight; weeks begin Monday.
/// Null or an unknown reset policy cannot supply a reset timestamp.
fn openrouter_key_reset(data: &serde_json::Value, now: i64) -> Option<i64> {
    use chrono::Datelike as _;
    let moment = chrono::DateTime::from_timestamp(now, 0)?;
    let date = moment.date_naive();
    let next = match data
        .get("limit_reset")
        .and_then(serde_json::Value::as_str)?
    {
        "daily" => date.checked_add_days(chrono::Days::new(1))?,
        "weekly" => date.checked_add_days(chrono::Days::new(u64::from(
            7 - date.weekday().num_days_from_monday(),
        )))?,
        "monthly" => {
            let (year, month) = if date.month() == 12 {
                (date.year().checked_add(1)?, 1)
            } else {
                (date.year(), date.month() + 1)
            };
            chrono::NaiveDate::from_ymd_opt(year, month, 1)?
        }
        _ => return None,
    };
    Some(next.and_hms_opt(0, 0, 0)?.and_utc().timestamp())
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
    // Validate every official monetary field, including usage when period
    // rows are unavailable. Malformed money must not become a fresh omission.
    let cap = openrouter_money_field(&data, "limit", SignedPolicy::NonNegative)?;
    let remaining = openrouter_money_field(&data, "limit_remaining", SignedPolicy::Remaining)?;
    let _usage = openrouter_money_field(&data, "usage", SignedPolicy::NonNegative)?;
    let daily_spend = openrouter_money_field(&data, "usage_daily", SignedPolicy::NonNegative)?;
    let weekly_spend = openrouter_money_field(&data, "usage_weekly", SignedPolicy::NonNegative)?;
    let monthly_spend = openrouter_money_field(&data, "usage_monthly", SignedPolicy::NonNegative)?;
    let byok_spend = openrouter_byok_spend(&data)?;
    // A known remaining allowance survives independently of cap availability.
    // Zero is a configured cap; unknown denominators supply no percentage.
    let cap = known_money(cap);
    let remaining = known_money(remaining);
    if cap.is_some() || remaining.is_some() {
        let used = match (&cap, &remaining) {
            (Some(cap), Some(remaining)) => {
                let used = cap.checked_sub(remaining).ok_or_else(|| {
                    "OpenRouter key usage difference is not exactly representable".to_owned()
                })?;
                if used.amount_minor < 0 {
                    return Err("OpenRouter key remaining allowance exceeds its cap".to_owned());
                }
                Some(used)
            }
            _ => None,
        };
        let remaining_percent = cap.as_ref().and_then(|cap| {
            remaining
                .as_ref()
                .and_then(|remaining| remaining.remaining_percent_of(cap))
        });
        let mut view = timed_bucket(
            "Key Limit",
            used.as_ref().map(ToString::to_string),
            Some(
                cap.as_ref()
                    .map_or_else(|| "Unknown".to_owned(), ToString::to_string),
            ),
            remaining_percent,
            openrouter_key_reset(&data, now),
            now,
            openrouter_key_expiry(&data, now).as_deref(),
            UsageSnapshotStatus::Fresh,
        );
        view.used_money = used;
        view.limit_money = cap;
        view.remaining_money = remaining;
        view.status_slot = Some(StatusSlot::Spend);
        buckets.push(view);
    }
    if let Some(daily) = key.free_model_daily_requests {
        let counts = jackin_protocol::control::CountQuota {
            used: daily.used,
            limit: daily.limit,
            remaining: daily.remaining,
            unit: jackin_protocol::control::CountQuotaUnit::Requests,
            period: jackin_protocol::control::CountQuotaPeriod::UtcDaily,
            provenance: jackin_protocol::control::CountQuotaProvenance::ProviderReported,
        };
        let remaining_percent = counts.remaining_percent();
        // Official contract: this account counter resets at UTC midnight.
        let reset_at = now
            .div_euclid(86_400)
            .checked_add(1)
            .and_then(|day| day.checked_mul(86_400));
        let mut view = timed_bucket(
            "Free model daily requests",
            daily.used.map(|used| format!("{used} requests")),
            daily.limit.map(|limit| format!("{limit} requests")),
            remaining_percent,
            reset_at,
            now,
            Some("account tier policy; exempt accounts, endpoints and BYOK are not gated"),
            UsageSnapshotStatus::Fresh,
        );
        view.count_quota = Some(counts);
        buckets.push(view);
    }
    // Period spend rows carry no denominator, so they never draw percentages.
    for (label, spend) in [
        ("Spent today", daily_spend),
        ("Spent this week", weekly_spend),
        ("Spent this month", monthly_spend),
    ] {
        if let Some(money) = known_money(spend) {
            let mut view = bucket(
                label,
                Some(money.to_string()),
                None,
                None,
                None,
                None,
                UsageSnapshotStatus::Fresh,
            );
            view.used_money = Some(money);
            buckets.push(view);
        }
    }
    if let Some(byok) = byok_spend {
        let mut view = bucket(
            "BYOK spend",
            Some(byok.to_string()),
            None,
            None,
            None,
            Some("billed to your own key"),
            UsageSnapshotStatus::Fresh,
        );
        view.used_money = Some(byok);
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

pub(crate) fn fetch_openrouter_key_usage(
    base_url: &str,
    key: &str,
) -> Result<serde_json::Value, ProviderHttpError> {
    get_json_bearer::<serde_json::Value>(
        jackin_telemetry::schema::enums::ProviderName::Openrouter,
        "GET",
        "OpenRouter key",
        &format!("{base_url}/key"),
        key,
        &[],
    )
}

fn openrouter_key_error_status(error: &ProviderError) -> UsageSnapshotStatus {
    match error.status() {
        Some(401) => UsageSnapshotStatus::NeedsLogin,
        _ => UsageSnapshotStatus::Error,
    }
}

/// Catalog validation never errors: any fetch failure degrades to `Unverified`
/// (a stale catalog must not reject a configured model).
pub(crate) fn fetch_openrouter_model_check(base_url: &str, model_id: &str) -> OpenRouterModelCheck {
    let unverified = |reason: String| OpenRouterModelCheck::Unverified {
        model_id: model_id.to_owned(),
        reason,
    };
    let catalog = provider_request(
        jackin_telemetry::schema::enums::ProviderName::Openrouter,
        "GET",
        "/models",
        || {
            let client = provider_http_client()?;
            let response = client
                .get(format!("{base_url}/models"))
                .header(reqwest::header::ACCEPT, "application/json")
                .send()
                .map_err(|error| format!("OpenRouter models request failed: {error}"))?;
            if !response.status().is_success() {
                return Err(format!("OpenRouter models HTTP {}", response.status()));
            }
            response
                .json::<serde_json::Value>()
                .map_err(|error| format!("OpenRouter models decode failed: {error}"))
        },
    );
    match catalog {
        Ok(value) => check_openrouter_model_in_catalog(&value, model_id),
        Err(error) => unverified(error),
    }
}

pub(crate) fn openrouter_snapshot(agent: &str, key: Option<&str>, now: i64) -> FocusedUsageView {
    openrouter_snapshot_with_rate_limit(agent, key, now).0
}

/// Key snapshot against an explicit base.
pub(crate) fn openrouter_snapshot_with_base(
    agent: &str,
    key: Option<&str>,
    base_url: &str,
    now: i64,
) -> FocusedUsageView {
    openrouter_snapshot_with_key_fetch(agent, key, base_url, now, fetch_openrouter_key_usage).0
}

pub(crate) fn openrouter_snapshot_with_rate_limit(
    agent: &str,
    key: Option<&str>,
    now: i64,
) -> (FocusedUsageView, Option<ProviderRateLimit>) {
    openrouter_snapshot_with_key_fetch(
        agent,
        key,
        &openrouter_base_url(),
        now,
        fetch_openrouter_key_usage,
    )
}

/// Snapshot boundary with an injectable key fetch. Production supplies the
/// shared HTTP fetcher; tests can drive transport/decode failures without
/// relying on an unreserved local port.
fn openrouter_snapshot_with_key_fetch<F>(
    agent: &str,
    key: Option<&str>,
    base_url: &str,
    now: i64,
    fetch_key: F,
) -> (FocusedUsageView, Option<ProviderRateLimit>)
where
    F: FnOnce(&str, &str) -> Result<serde_json::Value, ProviderHttpError>,
{
    let Some(key) = key.filter(|key| !key.trim().is_empty()) else {
        return (
            usage_view(UsageViewInput {
                agent,
                provider: Some("OpenRouter"),
                surface: UsageSurface::OpenRouter,
                account_label: "OpenRouter key missing".to_owned(),
                username: None,
                plan_label: None,
                credential_origin: None,
                buckets: vec![bucket(
                    "Usage",
                    None,
                    None,
                    None,
                    None,
                    Some("OpenRouter API key missing"),
                    UsageSnapshotStatus::NeedsLogin,
                )],
                status: UsageSnapshotStatus::NeedsLogin,
                source: UsageSource::None,
                confidence: UsageConfidence::None,
                now,
                last_error: Some("OpenRouter API key missing".to_owned()),
            }),
            None,
        );
    };
    let key_result = fetch_key(base_url, key)
        .map_err(ProviderError::from)
        .and_then(|value| parse_openrouter_key_usage(value, now).map_err(ProviderError::from));
    let (quota, key_error) = match key_result {
        Ok(quota) => (Some(quota), None),
        Err(error) => (None, Some(error)),
    };
    let status = key_error
        .as_ref()
        .map_or(UsageSnapshotStatus::Fresh, |error| {
            openrouter_key_error_status(error)
        });
    let rate_limit = key_error.as_ref().and_then(ProviderError::rate_limit);
    let key_error_message = key_error.as_ref().map(|error| error.message().to_owned());
    let buckets = quota.as_ref().map_or_else(
        || {
            vec![bucket(
                "Usage",
                None,
                None,
                None,
                None,
                key_error_message.as_deref(),
                status,
            )]
        },
        |quota| quota.buckets.clone(),
    );
    // No management authority exists in this API. Removing the management
    // fetch path makes credential substitution impossible, including retries.
    let enrichment_note = (status == UsageSnapshotStatus::Fresh).then(|| {
        "OpenRouter account funds and activity unavailable: a separately configured Management credential is required".to_owned()
    });
    let view = usage_view(UsageViewInput {
        agent,
        provider: Some("OpenRouter"),
        surface: UsageSurface::OpenRouter,
        account_label: "OpenRouter key".to_owned(),
        username: None,
        plan_label: quota.as_ref().and_then(|quota| quota.plan_label.clone()),
        credential_origin: Some("API token · OpenRouter key".to_owned()),
        buckets,
        status,
        source: if status == UsageSnapshotStatus::Fresh {
            UsageSource::ProviderApi
        } else {
            UsageSource::None
        },
        confidence: if status == UsageSnapshotStatus::Fresh {
            UsageConfidence::Authoritative
        } else {
            UsageConfidence::None
        },
        now,
        last_error: key_error_message.or_else(|| enrichment_note),
    });
    (view, rate_limit)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "openrouter_money_tests.rs"]
mod money_tests;
