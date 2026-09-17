// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `Z.AI` / `GLM` usage snapshot.
//!
//! Carved out of `usage.rs` for the file-size ratchet. Items in this module
//! are `pub(crate)` so the coordinator (`usage.rs`) can re-export them.
//!
//! Quota contract (see `ref-contracts-B.md` §2): `GET
//! {api.z.ai,open.bigmodel.cn}/api/monitor/usage/quota/limit` with
//! `data.limits[]` carrying new `CREDIT_LIMIT` and older `TOKENS_LIMIT`
//! windows plus separate `TIME_LIMIT` tool/MCP quotas. A 2xx
//! `success: false` envelope means a valid key with no GLM Coding Plan — a
//! distinct state, not a transport error. CN team scope appends `?type=2`
//! with `Bigmodel-Organization` / `Bigmodel-Project` headers; missing
//! selectors can return HTTP success with empty data.

#[cfg_attr(
    not(test),
    expect(clippy::wildcard_imports, reason = "target-dependent")
)]
use super::*;
use chrono::{Datelike, Timelike};
use serde::Deserialize;

pub(crate) fn provider_key_snapshot(
    agent: &str,
    surface: UsageSurface,
    key_name: &str,
    key: Option<&str>,
    now: i64,
) -> FocusedUsageView {
    let has_key = key.is_some_and(|value| !value.is_empty());
    let (provider_quota, provider_error) = split_fetch(
        key.filter(|_| matches!(surface, UsageSurface::Zai))
            .map(fetch_zai_usage),
    );
    let (status, source, confidence) = provider_outcome(ProviderPresence {
        has_data: provider_quota.is_some(),
        has_secret: has_key,
    });
    let buckets = provider_quota
        .as_ref()
        .map(|quota| quota.buckets(now))
        .filter(|buckets| !buckets.is_empty())
        .unwrap_or_else(|| {
            vec![bucket(
                "Quota",
                None,
                None,
                None,
                None,
                provider_error
                    .as_deref()
                    .or(Some("provider quota API pending")),
                status,
            )]
        });
    let team_active = resolve_zai_team_scope().active();
    let plan_label = provider_quota
        .as_ref()
        .and_then(ZaiQuotaResponse::plan_name)
        .map(|plan| {
            if team_active {
                format!("{plan} · Team")
            } else {
                plan
            }
        });
    usage_view(UsageViewInput {
        agent,
        provider: Some(surface.label()),
        surface,
        account_label: String::new(),
        username: None,
        plan_label,
        credential_origin: Some(if has_key {
            format!("API token · env {key_name}")
        } else {
            format!("needs env {key_name}")
        }),
        buckets,
        status,
        source,
        confidence,
        now,
        last_error: match status {
            UsageSnapshotStatus::NeedsSecret => {
                Some(format!("{key_name} is not available to Capsule"))
            }
            UsageSnapshotStatus::Unsupported => Some(provider_error.unwrap_or_else(|| {
                format!(
                    "{} quota API unavailable; key presence only",
                    surface.label()
                )
            })),
            _ => None,
        },
    })
}

#[derive(Debug, Deserialize)]
pub(crate) struct ZaiQuotaResponse {
    pub(crate) code: Option<i64>,
    pub(crate) msg: Option<String>,
    pub(crate) success: Option<bool>,
    pub(crate) data: Option<ZaiQuotaData>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ZaiQuotaData {
    #[serde(default)]
    pub(crate) limits: Vec<ZaiLimitRaw>,
    #[serde(
        rename = "planName",
        alias = "plan",
        alias = "plan_type",
        alias = "packageName"
    )]
    pub(crate) plan_name: Option<String>,
    // Separate field, not another `plan_name` alias: serde raises a
    // duplicate-field error if two aliased keys co-occur, which would fail
    // the whole parse when a response carries both `planName` and `level`.
    pub(crate) level: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ZaiLimitRaw {
    #[serde(rename = "type")]
    pub(crate) limit_type: String,
    pub(crate) unit: Option<i64>,
    pub(crate) number: Option<i64>,
    pub(crate) usage: Option<i64>,
    #[serde(rename = "currentValue")]
    pub(crate) current_value: Option<i64>,
    pub(crate) remaining: Option<i64>,
    pub(crate) percentage: Option<f64>,
    #[serde(rename = "nextResetTime")]
    pub(crate) next_reset_time: Option<i64>,
    #[serde(rename = "usageDetails", default)]
    pub(crate) usage_details: Vec<ZaiUsageDetail>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ZaiUsageDetail {
    #[serde(rename = "modelCode", alias = "model_code", alias = "model")]
    pub(crate) model_code: Option<String>,
    pub(crate) usage: Option<i64>,
}

impl ZaiQuotaResponse {
    pub(crate) fn buckets(&self, now: i64) -> Vec<QuotaBucketView> {
        let limits = self
            .data
            .as_ref()
            .map(|data| data.limits.clone())
            .unwrap_or_default();
        let mut buckets = Vec::new();
        for limit in &limits {
            let Some(slot) = limit.semantic_slot() else {
                if limit.limit_type == "TIME_LIMIT" {
                    buckets.push(zai_bucket(limit.time_label(), limit, now));
                }
                continue;
            };
            let label = match slot {
                StatusSlot::Session => "Session",
                StatusSlot::Weekly => "Weekly",
                _ => continue,
            };
            buckets.push(with_status_slot(zai_bucket(label, limit, now), Some(slot)));
        }
        buckets
    }

    pub(crate) fn plan_name(&self) -> Option<String> {
        let data = self.data.as_ref()?;
        data.plan_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or_else(|| {
                data.level
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
            })
            .map(str::to_owned)
    }
}

impl ZaiLimitRaw {
    pub(crate) fn used_percent(&self) -> Option<u8> {
        if let Some(limit) = self.usage.filter(|limit| *limit > 0) {
            let used = if let Some(remaining) = self.remaining {
                let from_remaining = limit.saturating_sub(remaining);
                self.current_value
                    .map_or(from_remaining, |current| from_remaining.max(current))
            } else {
                self.current_value?
            };
            #[expect(clippy::cast_sign_loss, reason = "clamped to 0.0..=100.0 above")]
            let percent = ((used.clamp(0, limit) as f64 / limit as f64) * 100.0)
                .round()
                .clamp(0.0, 100.0) as u8;
            return Some(percent);
        }
        self.percentage.map(|percent| {
            #[expect(clippy::cast_sign_loss, reason = "clamped to 0.0..=100.0 above")]
            {
                percent.round().clamp(0.0, 100.0) as u8
            }
        })
    }

    /// Window length in minutes from the `(unit, number)` period pair:
    /// `1` = day, `3` = hour, `5` = minute, `6` = week. Unknown unit codes
    /// yield no window (never a guessed duration).
    pub(crate) fn window_minutes(&self) -> Option<i64> {
        let number = self.number?;
        if number <= 0 {
            return None;
        }
        match self.unit {
            Some(5) => Some(number),
            Some(3) => Some(number * 60),
            Some(1) => Some(number * 24 * 60),
            Some(6) => Some(number * 7 * 24 * 60),
            _ => None,
        }
    }

    /// `TIME_LIMIT` covers both short tool/MCP quotas and the monthly
    /// web-search count; the window tells them apart. The ≥28d `Web search`
    /// split is a window-size heuristic with a known residual risk: a 28d+
    /// MCP window would mislabel. No sharper signal exists in the quota
    /// payload, so the guess stays, documented.
    pub(crate) fn time_label(&self) -> &'static str {
        if self
            .window_minutes()
            .is_some_and(|minutes| minutes >= 28 * 24 * 60)
        {
            "Web search"
        } else {
            "MCP"
        }
    }

    fn semantic_slot(&self) -> Option<StatusSlot> {
        if !matches!(self.limit_type.as_str(), "TOKENS_LIMIT" | "CREDIT_LIMIT") {
            return None;
        }
        match self.window_minutes()? {
            minutes if minutes < 24 * 60 => Some(StatusSlot::Session),
            minutes if minutes >= 3 * 24 * 60 => Some(StatusSlot::Weekly),
            _ => None,
        }
    }
}

/// Peak-rate window: Mon–Fri 06:00–10:00 UTC burns credits at 1×,
/// everything else at 0.5×. Source: the GLM Coding Plan rate note captured
/// in `ref-contracts-B.md` §2 — no endpoint exposes the rate, so it is
/// derived client-side from the clock and goes stale silently if the plan
/// changes; pace-note only, never quota math.
pub(crate) const ZAI_PEAK_START_HOUR_UTC: u32 = 6;
pub(crate) const ZAI_PEAK_END_HOUR_UTC: u32 = 10;

/// True while the spend clock is inside the peak-rate window.
pub(crate) fn zai_is_peak(now: i64) -> bool {
    let Some(moment) = chrono::DateTime::from_timestamp(now, 0).map(|date| date.naive_utc()) else {
        return false;
    };
    matches!(
        moment.weekday(),
        chrono::Weekday::Mon
            | chrono::Weekday::Tue
            | chrono::Weekday::Wed
            | chrono::Weekday::Thu
            | chrono::Weekday::Fri
    ) && (ZAI_PEAK_START_HOUR_UTC..ZAI_PEAK_END_HOUR_UTC).contains(&moment.hour())
}

pub(crate) fn zai_credit_rate_note(now: i64) -> &'static str {
    if zai_is_peak(now) {
        "peak 1× rate"
    } else {
        "off-peak 0.5× rate"
    }
}

/// Top-two models by `usageDetails` consumption, e.g.
/// `top glm-5 71% · glm-4.5 29%`. `None` when no model breakdown is present.
pub(crate) fn zai_model_note(limit: &ZaiLimitRaw) -> Option<String> {
    let mut details = limit.usage_details.clone();
    details.retain(|detail| {
        detail.usage.is_some_and(|usage| usage > 0)
            && detail
                .model_code
                .as_deref()
                .is_some_and(|code| !code.trim().is_empty())
    });
    if details.is_empty() {
        return None;
    }
    details.sort_by_key(|detail| std::cmp::Reverse(detail.usage.unwrap_or(0)));
    let total: i64 = details.iter().filter_map(|detail| detail.usage).sum();
    let parts = details
        .iter()
        .take(2)
        .map(|detail| {
            let code = detail.model_code.as_deref().unwrap_or_default().trim();
            if total > 0 {
                let used = detail.usage.unwrap_or(0).max(0);
                let share = (i128::from(used) * 100 / i128::from(total)).clamp(0, 100) as i64;
                format!("{code} {share}%")
            } else {
                code.to_owned()
            }
        })
        .collect::<Vec<_>>();
    Some(format!("top {}", parts.join(" · ")))
}

pub(crate) fn zai_bucket(label: &str, limit: &ZaiLimitRaw, now: i64) -> QuotaBucketView {
    let used_percent = limit.used_percent();
    let remaining = used_percent.map(|used| 100u8.saturating_sub(used));
    let reset_at = limit.next_reset_time.map(epoch_seconds_from_maybe_ms);
    let mut parts = Vec::new();
    if matches!(label, "MCP" | "Web search") {
        parts.extend(zai_count_line(limit));
    } else if limit.limit_type == "CREDIT_LIMIT" {
        parts.push(zai_credit_rate_note(now).to_owned());
    }
    parts.extend(zai_model_note(limit));
    let detail = (!parts.is_empty()).then(|| parts.join(" · "));
    timed_bucket(
        label,
        limit
            .current_value
            .map(|value| compact_count(u64::try_from(value.max(0)).unwrap_or(0))),
        limit
            .usage
            .map(|value| compact_count(u64::try_from(value.max(0)).unwrap_or(0))),
        remaining,
        reset_at,
        now,
        detail.as_deref(),
        UsageSnapshotStatus::Fresh,
    )
}

pub(crate) fn zai_count_line(limit: &ZaiLimitRaw) -> Option<String> {
    let total = limit.usage.filter(|value| *value > 0)?;
    let used = if let Some(remaining) = limit.remaining {
        let from_remaining = total.saturating_sub(remaining);
        limit
            .current_value
            .map_or(from_remaining, |current| from_remaining.max(current))
    } else {
        limit.current_value?
    }
    .clamp(0, total);
    let remaining = total.saturating_sub(used);
    Some(format!(
        "{} / {} ({} remaining)",
        compact_count(u64::try_from(used).unwrap_or(0)),
        compact_count(u64::try_from(total).unwrap_or(0)),
        compact_count(u64::try_from(remaining).unwrap_or(0))
    ))
}

/// CN team scope: `?type=2` on the quota path plus the
/// `Bigmodel-Organization` / `Bigmodel-Project` headers.
#[derive(Debug, Clone, Default)]
pub(crate) struct ZaiTeamScope {
    pub(crate) quota_type: Option<String>,
    pub(crate) organization: Option<String>,
    pub(crate) project: Option<String>,
}

pub(crate) fn resolve_zai_team_scope() -> ZaiTeamScope {
    zai_team_scope_from(
        env_value("ZAI_QUOTA_TYPE").as_deref(),
        env_value("BIGMODEL_ORGANIZATION")
            .or_else(|| env_value("ZAI_TEAM_ORG"))
            .as_deref(),
        env_value("BIGMODEL_PROJECT")
            .or_else(|| env_value("ZAI_TEAM_PROJECT"))
            .as_deref(),
    )
}

pub(crate) fn zai_team_scope_from(
    quota_type: Option<&str>,
    organization: Option<&str>,
    project: Option<&str>,
) -> ZaiTeamScope {
    let clean = |value: Option<&str>| {
        value
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    ZaiTeamScope {
        quota_type: clean(quota_type),
        organization: clean(organization),
        project: clean(project),
    }
}

impl ZaiTeamScope {
    pub(crate) fn active(&self) -> bool {
        self.quota_type.is_some() || self.organization.is_some() || self.project.is_some()
    }

    pub(crate) fn query(&self) -> Option<&str> {
        self.quota_type.as_deref()
    }
}

pub(crate) fn fetch_zai_usage(token: &str) -> Result<ZaiQuotaResponse, String> {
    let mut url = resolve_zai_quota_url();
    let scope = resolve_zai_team_scope();
    if let Some(quota_type) = scope.query() {
        url = format!("{url}?type={quota_type}");
    }
    let quota: ZaiQuotaResponse = provider_request(
        jackin_telemetry::schema::enums::ProviderName::Zai,
        "GET",
        "/api/monitor/usage/quota/limit",
        || {
            let client = provider_http_client()?;
            let mut request = client
                .get(&url)
                .bearer_auth(token)
                .header(reqwest::header::ACCEPT, "application/json");
            if let Some(organization) = scope.organization.as_deref() {
                request = request.header("Bigmodel-Organization", organization);
            }
            if let Some(project) = scope.project.as_deref() {
                request = request.header("Bigmodel-Project", project);
            }
            let response = request
                .send()
                .map_err(|err| format!("Z.AI quota request failed: {err}"))?;
            let status = response.status();
            if !status.is_success() {
                return Err(format!("Z.AI quota HTTP {status}"));
            }
            response
                .json::<ZaiQuotaResponse>()
                .map_err(|err| format!("Z.AI quota decode failed: {err}"))
        },
    )?;
    // HTTP success with `success: false` is a valid key without a GLM Coding
    // Plan — surfaced distinctly so it is never mistaken for key presence.
    if quota.success == Some(false) {
        let detail = quota.msg.unwrap_or_else(|| "quota rejected".to_owned());
        return Err(format!("Z.AI key has no GLM Coding Plan ({detail})"));
    }
    if quota.code.is_some_and(|code| code != 200) {
        let detail = quota.msg.unwrap_or_else(|| "unknown error".to_owned());
        return Err(format!("Z.AI quota rejected response: {detail}"));
    }
    // HTTP success with no windows: a team key missing its selectors, or no
    // plan entitlement — never rendered as empty-but-fresh quota.
    let empty = quota
        .data
        .as_ref()
        .is_none_or(|data| data.limits.is_empty());
    if empty {
        return Err(if scope.active() {
            "Z.AI quota returned no usage windows for team scope; verify organization/project selectors".to_owned()
        } else {
            "Z.AI quota returned no usage windows; verify Coding Plan entitlement".to_owned()
        });
    }
    Ok(quota)
}

pub(crate) fn resolve_zai_quota_url() -> String {
    let override_url = env_value("ZAI_QUOTA_URL").or_else(|| env_value("Z_AI_QUOTA_URL"));
    let host = env_value("ZAI_API_HOST")
        .or_else(|| env_value("Z_AI_API_HOST"))
        .unwrap_or_else(|| "https://api.z.ai".to_owned());
    resolve_zai_quota_url_from(override_url.as_deref(), Some(&host))
}

pub(crate) fn resolve_zai_quota_url_from(override_url: Option<&str>, host: Option<&str>) -> String {
    if let Some(url) = override_url {
        return normalize_url_or_host(url, "");
    }
    let host = host.unwrap_or("https://api.z.ai");
    normalize_url_or_host(&zai_quota_host(host), "api/monitor/usage/quota/limit")
}

pub(crate) fn zai_quota_host(value: &str) -> String {
    let normalized = normalize_url_or_host(value, "");
    let Ok(mut url) = url::Url::parse(&normalized) else {
        return normalized;
    };
    url.set_path("");
    url.set_query(None);
    url.set_fragment(None);
    url.to_string().trim_end_matches('/').to_owned()
}

pub(crate) fn json_epoch_seconds(value: &serde_json::Value) -> Option<i64> {
    let number = json_number(value)?;
    if number > 1_000_000_000_000.0 {
        Some((number / 1000.0).floor() as i64)
    } else {
        Some(number.floor() as i64)
    }
}

#[cfg(test)]
mod tests;
