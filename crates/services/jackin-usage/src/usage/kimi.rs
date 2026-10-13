// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `Kimi` usage snapshot.
//!
//! Carved out of `usage.rs` for the file-size ratchet. Items in this module
//! are `pub(crate)` so the coordinator (`usage.rs`) can re-export them.
//!
//! Response families (see `ref-contracts-B.md` §1):
//!
//! * Code API `GET {base}/coding/v1/usages`: `usage` summary + `limits[]`
//!   rate windows + `usages` rolling/weekly/monthly pools object +
//!   `user.membership.level` + `version`.
//! * Web gateway `BillingService/GetUsages`: `usages` list with a
//!   `FEATURE_CODING` entry (same `detail`/`limits[]` shapes).
//! * Local server `GET /api/v1/oauth/usage`: `summary`/`limits` plus the
//!   Extra Usage wallet (`KimiLocalUsage`); `/api/v1/oauth/userinfo` carries
//!   the shared billing identity also present as Code API `user`.

#[cfg_attr(
    not(test),
    expect(clippy::wildcard_imports, reason = "target-dependent")
)]
use super::*;
use serde::Deserialize;

pub(crate) fn kimi_snapshot(agent: &str, token: Option<&str>, now: i64) -> FocusedUsageView {
    let has_local = home_path(".kimi-code").exists() || home_path(".kimi").exists();
    let has_token = token.is_some_and(|value| !value.is_empty());
    let (provider_usage, provider_error) = split_fetch(token.map(fetch_kimi_usage));
    let (status, source, confidence) = provider_outcome(ProviderPresence {
        has_data: provider_usage.is_some(),
        has_secret: has_token || has_local,
    });
    let buckets = provider_usage
        .as_ref()
        .map(|usage| usage.buckets(now))
        .filter(|buckets| !buckets.is_empty())
        .unwrap_or_else(|| {
            vec![
                bucket(
                    "Weekly",
                    None,
                    None,
                    None,
                    None,
                    provider_error
                        .as_deref()
                        .or(Some("Kimi billing endpoint unavailable")),
                    status,
                ),
                bucket(
                    "5-hour rate limit",
                    None,
                    None,
                    None,
                    None,
                    provider_error
                        .as_deref()
                        .or(Some("Kimi billing endpoint unavailable")),
                    status,
                ),
            ]
        });
    // One Kimi billing identity can fund the Kimi, Claude and Codex clients;
    // surface the stable subject (email/id) plus membership plan so the broker
    // can dedup observations by (service + billing subject), never by client.
    let (account_label, username, plan_label) = provider_usage
        .as_ref()
        .map(kimi_account_identity)
        .unwrap_or_default();
    usage_view(UsageViewInput {
        agent,
        provider: None,
        surface: UsageSurface::Kimi,
        account_label,
        username,
        plan_label,
        credential_origin: Some(
            if has_token {
                "API token · env KIMI_CODE_API_KEY"
            } else if has_local {
                "API key · ~/.kimi-code"
            } else {
                "needs Kimi auth"
            }
            .to_owned(),
        ),
        buckets,
        status,
        source,
        confidence,
        now,
        last_error: match status {
            UsageSnapshotStatus::NeedsSecret => {
                Some("Kimi auth not available to Capsule".to_owned())
            }
            UsageSnapshotStatus::Unsupported => Some(provider_error.unwrap_or_else(|| {
                "Kimi billing endpoint unavailable; local presence only".to_owned()
            })),
            _ => None,
        },
    })
}

/// Stable billing identity shared across every client one Kimi account funds:
/// `(account_label, username, plan_label)`.
pub(crate) fn kimi_account_identity(
    usage: &KimiUsageResponse,
) -> (String, Option<String>, Option<String>) {
    let user = usage.user.as_ref();
    let email = user
        .and_then(|user| user.email.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let name = user
        .and_then(|user| user.name.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let id = user
        .and_then(|user| user.id.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let account_label = email.or(name).or(id).unwrap_or_default().to_owned();
    let username = email.or(name).map(str::to_owned);
    let level = user
        .and_then(|user| user.membership.as_ref())
        .and_then(|membership| {
            membership
                .level
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
        });
    let plan_label = level.map(|level| kimi_membership_plan(level, usage.version.as_deref()));
    (account_label, username, plan_label)
}

/// Map a `user.membership.level` to its display plan. The `LEVEL_*` →
/// tempo-name mapping is only valid when `version` is absent or
/// `GOODS_VERSION_V1`; any other goods version passes the raw level through
/// humanized rather than risking a wrong plan name.
pub(crate) fn kimi_membership_plan(level: &str, version: Option<&str>) -> String {
    if version.is_some_and(|version| version != "GOODS_VERSION_V1") {
        return humanize_plan_label(&level.to_ascii_lowercase());
    }
    match level {
        "LEVEL_FREE" => "Adagio",
        "LEVEL_TRIAL" => "Andante",
        "LEVEL_BASIC" => "Moderato",
        "LEVEL_INTERMEDIATE" => "Allegretto",
        "LEVEL_ADVANCED" => "Allegro",
        other => {
            return humanize_plan_label(
                &other
                    .strip_prefix("LEVEL_")
                    .unwrap_or(other)
                    .to_ascii_lowercase(),
            );
        }
    }
    .to_owned()
}

#[derive(Debug, Deserialize)]
pub(crate) struct KimiUsageResponse {
    #[serde(default)]
    pub(crate) usages: Option<KimiUsages>,
    pub(crate) usage: Option<KimiUsageDetail>,
    #[serde(default)]
    pub(crate) limits: Vec<KimiRateLimit>,
    pub(crate) user: Option<KimiUser>,
    pub(crate) version: Option<String>,
}

/// The two `usages` shapes: the web gateway sends a list of scoped entries
/// while the Code API sends the rolling/weekly/monthly pools object.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum KimiUsages {
    Pools(Box<KimiPools>),
    List(Vec<KimiUsageItem>),
}

#[derive(Debug, Deserialize)]
pub(crate) struct KimiPools {
    pub(crate) limit_5h: Option<KimiPool>,
    pub(crate) limit_7d: Option<KimiPool>,
    pub(crate) limit_month_total: Option<KimiPool>,
}

impl KimiPools {
    pub(crate) fn any(&self) -> bool {
        self.limit_5h.is_some() || self.limit_7d.is_some() || self.limit_month_total.is_some()
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct KimiPool {
    pub(crate) limit: Option<KimiCount>,
    pub(crate) used: Option<KimiCount>,
    pub(crate) remaining: Option<KimiCount>,
    pub(crate) used_ratio: Option<f64>,
    #[serde(
        default,
        rename = "resetTime",
        alias = "reset_time",
        alias = "reset_at",
        alias = "resetAt"
    )]
    pub(crate) reset_time: Option<KimiReset>,
    pub(crate) name: Option<String>,
    pub(crate) title: Option<String>,
}

impl KimiPool {
    /// Raw used percent, unclamped: over-cap readings (>100%) survive so the
    /// bucket can carry the raw figure (T02); only the bar geometry clamps.
    pub(crate) fn used_percent_raw(&self) -> Option<f64> {
        if let Some(limit) = self
            .limit
            .as_ref()
            .and_then(KimiCount::value)
            .filter(|limit| *limit > 0)
        {
            let used = self.used.as_ref().and_then(KimiCount::value).or_else(|| {
                self.remaining
                    .as_ref()
                    .and_then(KimiCount::value)
                    .map(|remaining| limit.saturating_sub(remaining))
            })?;
            #[expect(clippy::cast_precision_loss, reason = "count magnitudes fit f64")]
            return Some(used.max(0) as f64 / limit as f64 * 100.0);
        }
        // `used_ratio` is a 0.0–1.0 fraction; values above 1.0 are an
        // already-scaled percent.
        self.used_ratio.and_then(|ratio| {
            if !ratio.is_finite() || ratio < 0.0 {
                return None;
            }
            Some(if ratio <= 1.0 { ratio * 100.0 } else { ratio })
        })
    }

    pub(crate) fn used_percent(&self) -> Option<u8> {
        self.used_percent_raw().map(|raw| {
            #[expect(
                clippy::cast_sign_loss,
                reason = "raw percent clamped non-negative; clamp bounds the f64→u8 cast"
            )]
            {
                raw.round().clamp(0.0, 100.0) as u8
            }
        })
    }

    pub(crate) fn label(&self, fallback: &str) -> String {
        [self.name.as_deref(), self.title.as_deref()]
            .into_iter()
            .flatten()
            .map(str::trim)
            .find(|value| !value.is_empty())
            .unwrap_or(fallback)
            .to_owned()
    }
}

/// A quota count sent either as a JSON number (Code API) or a string (web
/// gateway); both families are accepted so neither fails the whole parse.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub(crate) enum KimiCount {
    Num(i64),
    Text(String),
}

impl KimiCount {
    pub(crate) fn value(&self) -> Option<i64> {
        match self {
            Self::Num(value) => Some(*value),
            Self::Text(value) => value.trim().parse().ok(),
        }
    }
}

/// A reset timestamp sent either as RFC 3339 text or an epoch (seconds or
/// milliseconds); unparseable text yields no reset rather than failing.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub(crate) enum KimiReset {
    Text(String),
    Num(i64),
}

impl KimiReset {
    pub(crate) fn epoch(&self) -> Option<i64> {
        match self {
            Self::Num(value) => Some(epoch_seconds_from_maybe_ms(*value)),
            Self::Text(value) => parse_iso_epoch(value.trim()).or_else(|| {
                value
                    .trim()
                    .parse::<i64>()
                    .ok()
                    .map(epoch_seconds_from_maybe_ms)
            }),
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct KimiUsageItem {
    pub(crate) scope: Option<String>,
    pub(crate) detail: KimiUsageDetail,
    #[serde(default)]
    pub(crate) limits: Vec<KimiRateLimit>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct KimiUsageDetail {
    pub(crate) limit: Option<KimiCount>,
    pub(crate) used: Option<KimiCount>,
    pub(crate) remaining: Option<KimiCount>,
    #[serde(
        default,
        rename = "resetTime",
        alias = "reset_time",
        alias = "reset_at",
        alias = "resetAt"
    )]
    pub(crate) reset_time: Option<KimiReset>,
    pub(crate) name: Option<String>,
    pub(crate) title: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct KimiUser {
    pub(crate) id: Option<String>,
    pub(crate) email: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) membership: Option<KimiMembership>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct KimiMembership {
    pub(crate) level: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct KimiRateLimit {
    pub(crate) window: Option<KimiWindow>,
    pub(crate) detail: KimiUsageDetail,
}

#[derive(Debug, Deserialize)]
pub(crate) struct KimiWindow {
    pub(crate) duration: Option<i64>,
    #[serde(rename = "timeUnit")]
    pub(crate) time_unit: Option<String>,
}

impl KimiUsageResponse {
    pub(crate) fn buckets(&self, now: i64) -> Vec<QuotaBucketView> {
        // The pools object is the precise per-window meter set; when present
        // it supersedes the coarser `usage` summary and `limits[]` shapes so
        // the same window is never rendered twice.
        if let Some(KimiUsages::Pools(pools)) = &self.usages
            && pools.any()
        {
            return kimi_pool_buckets(pools, now);
        }
        let (detail, limits) = if let Some(detail) = &self.usage {
            (detail, self.limits.as_slice())
        } else if let Some(KimiUsages::List(usages)) = &self.usages
            && let Some(usage) = usages
                .iter()
                .find(|usage| usage.scope.as_deref() == Some("FEATURE_CODING"))
                .or_else(|| usages.first())
        {
            (&usage.detail, usage.limits.as_slice())
        } else {
            return Vec::new();
        };
        // rate (short/active) window on top, then Weekly — an operator
        // override of CodexBar's Weekly, Rate Limit order.
        let mut buckets = Vec::new();
        if let Some(rate_limit) = limits.first() {
            buckets.push(with_status_slot(
                kimi_bucket(
                    "Rate Limit",
                    &rate_limit.detail,
                    rate_limit.window.as_ref(),
                    now,
                ),
                Some(StatusSlot::Session),
            ));
        }
        buckets.push(with_status_slot(
            kimi_bucket("Weekly", detail, None, now),
            Some(StatusSlot::Weekly),
        ));
        buckets
    }
}

pub(crate) fn kimi_pool_buckets(pools: &KimiPools, now: i64) -> Vec<QuotaBucketView> {
    let mut buckets = Vec::new();
    if let Some(pool) = &pools.limit_5h {
        buckets.push(with_status_slot(
            kimi_pool_bucket(pool, "5-hour", 5 * 60 * 60, now),
            Some(StatusSlot::Session),
        ));
    }
    if let Some(pool) = &pools.limit_7d {
        buckets.push(with_status_slot(
            kimi_pool_bucket(pool, "Weekly", 7 * 24 * 60 * 60, now),
            Some(StatusSlot::Weekly),
        ));
    }
    if let Some(pool) = &pools.limit_month_total {
        buckets.push(kimi_pool_bucket(pool, "Monthly", 30 * 24 * 60 * 60, now));
    }
    buckets
}

pub(crate) fn kimi_pool_bucket(
    pool: &KimiPool,
    fallback_label: &str,
    window_seconds: i64,
    now: i64,
) -> QuotaBucketView {
    let limit = pool.limit.as_ref().and_then(KimiCount::value);
    let used = pool.used.as_ref().and_then(KimiCount::value).or_else(|| {
        limit.and_then(|limit| {
            pool.remaining
                .as_ref()
                .and_then(KimiCount::value)
                .map(|remaining| limit.saturating_sub(remaining))
        })
    });
    let used_percent = pool.used_percent();
    let remaining = used_percent.map(|used| 100u8.saturating_sub(used));
    let reset_at = pool.reset_time.as_ref().and_then(KimiReset::epoch);
    let pace = quota_pace_label(remaining, reset_at, Some(window_seconds), now);
    let pace = kimi_pace_with_over_cap(pool.used_percent_raw(), pace);
    timed_bucket(
        &pool.label(fallback_label),
        used.map(|value| compact_count(u64::try_from(value.max(0)).unwrap_or(0))),
        limit.map(|value| compact_count(u64::try_from(value.max(0)).unwrap_or(0))),
        remaining,
        reset_at,
        now,
        pace.as_deref(),
        UsageSnapshotStatus::Fresh,
    )
}

/// Prefix the raw over-cap figure ahead of the pace note (`142% used · …`),
/// or pass the pace through when at/below cap.
fn kimi_pace_with_over_cap(raw_percent: Option<f64>, pace: Option<String>) -> Option<String> {
    let over_cap = raw_percent.and_then(kimi_over_cap_label);
    match (over_cap, pace) {
        (Some(over_cap), Some(pace)) => Some(format!("{over_cap} · {pace}")),
        (Some(over_cap), None) => Some(over_cap),
        (None, pace) => pace,
    }
}

impl KimiUsageDetail {
    pub(crate) fn limit_value(&self) -> Option<i64> {
        self.limit.as_ref().and_then(KimiCount::value)
    }

    pub(crate) fn used_value(&self) -> Option<i64> {
        self.used.as_ref().and_then(KimiCount::value)
    }

    pub(crate) fn remaining_value(&self) -> Option<i64> {
        self.remaining.as_ref().and_then(KimiCount::value)
    }

    /// Raw used percent, unclamped (see [`KimiPool::used_percent_raw`]).
    pub(crate) fn used_percent_raw(&self) -> Option<f64> {
        let limit = self.limit_value()?.max(0);
        if limit == 0 {
            return None;
        }
        let used = self.used_value().or_else(|| {
            self.remaining_value()
                .map(|remaining| limit.saturating_sub(remaining))
        })?;
        #[expect(clippy::cast_precision_loss, reason = "count magnitudes fit f64")]
        Some(used.max(0) as f64 / limit as f64 * 100.0)
    }

    pub(crate) fn used_percent(&self) -> Option<u8> {
        self.used_percent_raw().map(|raw| {
            #[expect(
                clippy::cast_sign_loss,
                reason = "raw percent clamped non-negative; clamp bounds the f64→u8 cast"
            )]
            {
                raw.round().clamp(0.0, 100.0) as u8
            }
        })
    }
}

/// Over-cap label preserving the raw provider figure (`142% used`), or `None`
/// at/below cap. Only the bar geometry clamps; the raw overage stays visible
/// (T02; the Muse lane renders the same form).
pub(crate) fn kimi_over_cap_label(raw_percent: f64) -> Option<String> {
    if !raw_percent.is_finite() || raw_percent <= 100.0 {
        return None;
    }
    Some(if raw_percent.fract() == 0.0 {
        format!("{raw_percent:.0}% used")
    } else {
        format!("{raw_percent:.1}% used")
    })
}

impl KimiWindow {
    pub(crate) fn seconds(&self) -> Option<i64> {
        let duration = self.duration?;
        let unit = self
            .time_unit
            .as_deref()
            .unwrap_or("hour")
            .to_ascii_lowercase();
        if unit.contains("second") {
            Some(duration)
        } else if unit.contains("minute") {
            Some(duration * 60)
        } else if unit.contains("hour") {
            Some(duration * 60 * 60)
        } else if unit.contains("day") {
            Some(duration * 24 * 60 * 60)
        } else if unit.contains("week") {
            Some(duration * 7 * 24 * 60 * 60)
        } else {
            None
        }
    }
}

pub(crate) fn kimi_bucket(
    label: &str,
    detail: &KimiUsageDetail,
    window: Option<&KimiWindow>,
    now: i64,
) -> QuotaBucketView {
    let limit = detail.limit_value();
    let used = detail.used_value().or_else(|| {
        limit.and_then(|limit| {
            detail
                .remaining_value()
                .map(|remaining| limit.saturating_sub(remaining))
        })
    });
    let used_percent = detail.used_percent();
    let remaining = used_percent.map(|used| 100u8.saturating_sub(used));
    let reset_at = detail.reset_time.as_ref().and_then(KimiReset::epoch);
    let window_seconds = kimi_window_seconds(label, window);
    let pace = quota_pace_label(remaining, reset_at, window_seconds, now);
    let pace = kimi_pace_with_over_cap(detail.used_percent_raw(), pace);
    timed_bucket(
        label,
        used.map(|value| compact_count(u64::try_from(value.max(0)).unwrap_or(0))),
        limit.map(|value| compact_count(u64::try_from(value.max(0)).unwrap_or(0))),
        remaining,
        reset_at,
        now,
        pace.as_deref(),
        UsageSnapshotStatus::Fresh,
    )
}

pub(crate) fn kimi_window_seconds(label: &str, window: Option<&KimiWindow>) -> Option<i64> {
    (label == "Rate Limit")
        .then(|| window.and_then(KimiWindow::seconds))
        .flatten()
}

pub(crate) fn fetch_kimi_usage(token: &str) -> Result<KimiUsageResponse, String> {
    let url = resolve_kimi_usages_url();
    provider_request(
        jackin_telemetry::schema::enums::ProviderName::Kimi,
        "GET",
        "/coding/v1/usages",
        || {
            let client = provider_http_client()?;
            let response = client
                .get(&url)
                .bearer_auth(token)
                .header(reqwest::header::ACCEPT, "application/json")
                .header(reqwest::header::USER_AGENT, "jackin-capsule/usage")
                .send()
                .map_err(|err| format!("Kimi usage request failed: {err}"))?;
            let status = response.status();
            if !status.is_success() {
                return Err(match status.as_u16() {
                    401 => "Kimi usage rejected: invalid key (HTTP 401)".to_owned(),
                    403 => "Kimi usage denied (HTTP 403)".to_owned(),
                    404 => "Kimi usage endpoint unavailable (HTTP 404)".to_owned(),
                    _ => format!("Kimi usage HTTP {status}"),
                });
            }
            response
                .json::<KimiUsageResponse>()
                .map_err(|err| format!("Kimi usage decode failed: {err}"))
        },
    )
}

/// Code API usages URL honoring the `KIMI_CODE_BASE_URL` override. Any base
/// carrying a `.../coding[/v1]` path resolves to `.../coding/v1/usages`.
pub(crate) fn resolve_kimi_usages_url() -> String {
    kimi_usages_url_from_base(env_value("KIMI_CODE_BASE_URL").as_deref())
}

pub(crate) fn kimi_usages_url_from_base(base: Option<&str>) -> String {
    const DEFAULT: &str = "https://api.kimi.com/coding/v1/usages";
    let Some(base) = base.map(str::trim).filter(|value| !value.is_empty()) else {
        return DEFAULT.to_owned();
    };
    let normalized = normalize_url_or_host(base, "");
    let trimmed = normalized.trim_end_matches('/');
    if trimmed.ends_with("/coding/v1/usages") {
        return trimmed.to_owned();
    }
    if let Some(host) = trimmed.strip_suffix("/coding/v1") {
        return format!("{host}/coding/v1/usages");
    }
    if let Some(host) = trimmed.strip_suffix("/coding") {
        return format!("{host}/coding/v1/usages");
    }
    format!("{trimmed}/coding/v1/usages")
}

/// Experimental local-server usage shape (`GET /api/v1/oauth/usage`): the
/// Extra Usage wallet. `summary`/`limits` schemas are version-specific and
/// carried opaquely; only the wallet maps to a bucket today.
#[derive(Debug, Deserialize)]
pub(crate) struct KimiLocalUsage {
    pub(crate) summary: Option<serde_json::Value>,
    pub(crate) limits: Option<serde_json::Value>,
    pub(crate) extra_usage: Option<KimiExtraUsage>,
    pub(crate) error: Option<KimiLocalError>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct KimiLocalError {
    pub(crate) code: Option<String>,
    pub(crate) message: Option<String>,
}

/// Unitless wallet keys whose scale was never evidenced: a major-unit value
/// under one of these would render 100x off as [`Money`], so hits render as
/// unknown-scale labels, never money (F06 currency precision).
pub(crate) const KIMI_UNKNOWN_SCALE_KEYS: &[&str] = &[
    "remaining",
    "used",
    "monthly_cap",
    "monthly_used",
    "cap",
    "limit",
];

/// Extra Usage wallet: remaining balance, wallet size, and the monthly
/// cap/used pair, all in minor currency units. Only evidenced keys feed
/// [`Money`] — the `*_cents` spellings plus bare `balance`/`total`; the
/// unitless aliases land in `other` and surface as unknown-scale labels.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct KimiExtraUsage {
    #[serde(alias = "balance_cents", alias = "remaining_cents")]
    pub(crate) balance: Option<i64>,
    #[serde(alias = "total_cents")]
    pub(crate) total: Option<i64>,
    // `rename`, not `alias`: the bare unitless spellings (`monthly_cap`,
    // `monthly_used`) must NOT bind here — they fall through to `other` as
    // unknown-scale labels.
    #[serde(
        rename = "monthly_cap_cents",
        alias = "cap_cents",
        alias = "limit_cents"
    )]
    pub(crate) monthly_cap: Option<i64>,
    #[serde(rename = "monthly_used_cents", alias = "used_cents")]
    pub(crate) monthly_used: Option<i64>,
    pub(crate) currency: Option<String>,
    #[serde(default, flatten)]
    pub(crate) other: HashMap<String, serde_json::Value>,
}

impl KimiExtraUsage {
    /// First unitless-alias hit as an unknown-scale `(key, display)` pair, or
    /// `None` when no unitless wallet key carried a finite number.
    pub(crate) fn unknown_scale_hit(&self) -> Option<(&str, String)> {
        KIMI_UNKNOWN_SCALE_KEYS.iter().find_map(|key| {
            let value = self.other.get(*key).and_then(json_number)?;
            if !value.is_finite() {
                return None;
            }
            let display = if value.fract() == 0.0 {
                format!("{value:.0}")
            } else {
                format!("{value}")
            };
            Some((*key, display))
        })
    }
}

impl KimiLocalUsage {
    /// Reject an HTTP-200 body carrying an in-band error; otherwise return the
    /// wallet (if any).
    pub(crate) fn wallet(&self) -> Result<Option<&KimiExtraUsage>, String> {
        if let Some(error) = &self.error {
            let detail = error
                .message
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .or(error.code.as_deref())
                .unwrap_or("unknown error");
            return Err(format!("Kimi local usage error: {detail}"));
        }
        Ok(self.extra_usage.as_ref())
    }
}

pub(crate) fn fetch_kimi_local_usage(
    base_url: &str,
    credential: &str,
) -> Result<KimiLocalUsage, String> {
    let url = normalize_url_or_host(base_url, "api/v1/oauth/usage");
    provider_request(
        jackin_telemetry::schema::enums::ProviderName::Kimi,
        "GET",
        "/api/v1/oauth/usage",
        || {
            let client = provider_http_client()?;
            let response = client
                .get(&url)
                .bearer_auth(credential)
                .header(reqwest::header::ACCEPT, "application/json")
                .send()
                .map_err(|err| format!("Kimi local usage request failed: {err}"))?;
            let status = response.status();
            if !status.is_success() {
                return Err(format!("Kimi local usage HTTP {status}"));
            }
            let usage = response
                .json::<KimiLocalUsage>()
                .map_err(|err| format!("Kimi local usage decode failed: {err}"))?;
            usage.wallet()?;
            Ok(usage)
        },
    )
}

/// Map the Extra Usage wallet to a `Spend`-slot money bucket: monthly used of
/// the monthly cap when present, else wallet consumed (`total - balance`) of
/// the wallet total. Currency falls back to the generic `credits` label —
/// never an assumed fiat code. With no usable money pair, a unitless-alias
/// hit renders as a label-only unknown-scale row (never [`Money`]); with
/// neither, there is no bucket at all.
pub(crate) fn kimi_extra_usage_bucket(extra: &KimiExtraUsage) -> Option<QuotaBucketView> {
    let currency = extra
        .currency
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("credits");
    let (used_minor, limit_minor) =
        if let (Some(used), Some(limit)) = (extra.monthly_used, extra.monthly_cap) {
            (used, Some(limit))
        } else if let (Some(total), Some(balance)) = (extra.total, extra.balance) {
            (total.saturating_sub(balance), Some(total))
        } else {
            return kimi_unknown_scale_bucket(extra);
        };
    if used_minor < 0 || limit_minor.is_some_and(|limit| limit < 0) {
        return None;
    }
    let used = Money::new(used_minor, currency, 2);
    let limit = limit_minor.map(|limit| Money::new(limit, currency, 2));
    let used_percent = limit_minor.and_then(|limit| {
        if limit <= 0 {
            return None;
        }
        #[expect(
            clippy::cast_sign_loss,
            reason = "used/limit checked non-negative; percent is rounded f64→u8"
        )]
        {
            Some(((used_minor.clamp(0, limit) as f64 / limit as f64) * 100.0).round() as u8)
        }
    });
    let remaining_percent = used_percent.map(|used| 100u8.saturating_sub(used));
    let mut view = bucket(
        "Extra usage",
        Some(format!("{used} spent")),
        limit.as_ref().map(ToString::to_string),
        remaining_percent,
        None,
        used_percent.map(|used| format!("{used}% used")).as_deref(),
        UsageSnapshotStatus::Fresh,
    );
    view.status_slot = Some(StatusSlot::Spend);
    view.used_money = Some(used);
    view.limit_money = limit;
    Some(view)
}

/// Label-only wallet row for a unitless-alias hit: the raw figure with its
/// key, explicitly unknown-scale. No [`Money`], no percent, no headline slot —
/// a detail row the headline ignores.
pub(crate) fn kimi_unknown_scale_bucket(extra: &KimiExtraUsage) -> Option<QuotaBucketView> {
    let (key, display) = extra.unknown_scale_hit()?;
    Some(bucket(
        "Extra usage",
        Some(display),
        None,
        None,
        None,
        Some(&format!("{key} · unknown scale")),
        UsageSnapshotStatus::Fresh,
    ))
}

pub(crate) fn load_kimi_local_token(now: i64) -> Option<String> {
    load_kimi_local_token_from_home(&home_path(""), now)
}

pub(crate) fn load_kimi_local_token_from_home(home: &Path, now: i64) -> Option<String> {
    [
        home.join(".kimi-code/credentials/kimi-code.json"),
        home.join(".kimi/credentials/kimi-code.json"),
    ]
    .into_iter()
    .find_map(|path| {
        let value = read_json_file(&path)?;
        kimi_local_token_from_value(&value, now)
    })
}

pub(crate) fn kimi_local_token_from_value(value: &serde_json::Value, now: i64) -> Option<String> {
    if let Some(expires_at) = value.get("expires_at").and_then(json_epoch_seconds)
        && expires_at <= now
    {
        return None;
    }
    value
        .get("access_token")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests;
