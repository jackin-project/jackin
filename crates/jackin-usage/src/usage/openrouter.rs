// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `OpenRouter` key/account usage snapshot.
//!
//! An ordinary inference key reads `GET {base}/key` (key cap, remaining,
//! period spend, BYOK attribution, expiry). Account balance reads
//! `GET {base}/credits` and needs a Management key: its 403 is a typed scope
//! mismatch that never suppresses the `/key` rows. Model IDs validate against
//! the public `GET {base}/models` catalog; a stale omission is `Unverified`,
//! never a rejection. A null key cap means no configured cap, not infinite
//! credit — no percentage bar is drawn without a matching denominator.
//!
//! Completed activity history (`GET {base}/activity`) is Management-key-only.
//! The current account credential contract supplies an inference key, not a
//! separate Management key, so history remains explicitly unavailable rather
//! than being fetched with the wrong scope or inferred from live key usage.

#[cfg_attr(
    not(test),
    expect(clippy::wildcard_imports, reason = "target-dependent")
)]
use super::*;
use serde::Deserialize;

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
    usage: Option<f64>,
    #[serde(default)]
    usage_daily: Option<f64>,
    #[serde(default)]
    usage_weekly: Option<f64>,
    #[serde(default)]
    usage_monthly: Option<f64>,
    /// Null = no configured key cap (never infinite credit).
    #[serde(default)]
    limit: Option<f64>,
    #[serde(default)]
    limit_remaining: Option<f64>,
    #[serde(default)]
    is_free_tier: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterKeyResponse {
    data: OpenRouterKeyData,
}

#[derive(Debug, Deserialize)]
struct OpenRouterCreditsData {
    total_credits: f64,
    total_usage: f64,
}

#[derive(Debug, Deserialize)]
struct OpenRouterCreditsResponse {
    data: OpenRouterCreditsData,
}

/// Typed `/credits` outcome. A 403 means the ordinary key lacks the Management
/// scope — the `/key` rows still render.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum OpenRouterCreditsOutcome {
    Available {
        spent_cents: i64,
        ceiling_cents: i64,
    },
    ManagementScopeDenied,
    Unavailable(String),
}

impl OpenRouterCreditsOutcome {
    /// Classify a `/credits` HTTP failure without string sniffing at the call
    /// site: 403 is the management-scope mismatch, anything else stays an
    /// opaque unavailable state.
    pub(crate) fn from_http_status(status: u16) -> Self {
        if status == 403 {
            Self::ManagementScopeDenied
        } else {
            Self::Unavailable(format!("OpenRouter credits HTTP {status}"))
        }
    }
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

pub(crate) fn fetch_openrouter_key_usage(
    base_url: &str,
    key: &str,
) -> Result<serde_json::Value, String> {
    get_json_bearer::<serde_json::Value>(
        jackin_telemetry::schema::enums::ProviderName::Openrouter,
        "GET",
        "OpenRouter key",
        &format!("{base_url}/key"),
        key,
        &[],
    )
}

/// `/credits` never fails the snapshot: every outcome (including the typed 403
/// management-scope mismatch) is a value, so `/key` rows always survive it.
pub(crate) fn fetch_openrouter_credits(base_url: &str, key: &str) -> OpenRouterCreditsOutcome {
    provider_request(
        jackin_telemetry::schema::enums::ProviderName::Openrouter,
        "GET",
        "/credits",
        || {
            let client = provider_http_client()?;
            let response = client
                .get(format!("{base_url}/credits"))
                .bearer_auth(key)
                .header(reqwest::header::ACCEPT, "application/json")
                .send()
                .map_err(|error| format!("OpenRouter credits request failed: {error}"))?;
            let status = response.status();
            if !status.is_success() {
                return Ok(OpenRouterCreditsOutcome::from_http_status(status.as_u16()));
            }
            let value = response
                .json::<serde_json::Value>()
                .map_err(|error| format!("OpenRouter credits decode failed: {error}"))?;
            Ok(parse_openrouter_credits(value)
                .unwrap_or_else(OpenRouterCreditsOutcome::Unavailable))
        },
    )
    .unwrap_or_else(OpenRouterCreditsOutcome::Unavailable)
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
    openrouter_snapshot_with_base(agent, key, &openrouter_base_url(), now)
}

/// Key snapshot against an explicit base: production resolves the base from
/// env, hermetic tests point it at a dead port.
pub(crate) fn openrouter_snapshot_with_base(
    agent: &str,
    key: Option<&str>,
    base_url: &str,
    now: i64,
) -> FocusedUsageView {
    let Some(key) = key.filter(|key| !key.trim().is_empty()) else {
        return usage_view(UsageViewInput {
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
        });
    };
    let key_result = fetch_openrouter_key_usage(base_url, key)
        .map_err(|error| {
            if error.contains("401") {
                (UsageSnapshotStatus::NeedsLogin, error)
            } else {
                (UsageSnapshotStatus::Error, error)
            }
        })
        .and_then(|value| {
            parse_openrouter_key_usage(value, now)
                .map_err(|error| (UsageSnapshotStatus::Error, error))
        });
    let (quota, status, key_error) = match key_result {
        Ok(quota) => (Some(quota), UsageSnapshotStatus::Fresh, None),
        Err((status, error)) => (None, status, Some(error)),
    };
    let mut buckets = quota.as_ref().map_or_else(
        || {
            vec![bucket(
                "Usage",
                None,
                None,
                None,
                None,
                key_error.as_deref(),
                status,
            )]
        },
        |quota| quota.buckets.clone(),
    );
    // `/credits` enriches but never suppresses: a Management-scope 403 keeps
    // the `/key` rows and surfaces as a note.
    let credits_note =
        (status == UsageSnapshotStatus::Fresh).then(|| {
            match fetch_openrouter_credits(base_url, key) {
                OpenRouterCreditsOutcome::Available {
                    spent_cents,
                    ceiling_cents,
                } => {
                    buckets.push(openrouter_credits_bucket(spent_cents, ceiling_cents));
                    None
                }
                OpenRouterCreditsOutcome::ManagementScopeDenied => Some(
                    "OpenRouter account credits need a Management key; showing key usage only"
                        .to_owned(),
                ),
                OpenRouterCreditsOutcome::Unavailable(error) => Some(error),
            }
        });
    usage_view(UsageViewInput {
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
        last_error: key_error.or_else(|| credits_note.flatten()),
    })
}

#[cfg(test)]
mod tests;
