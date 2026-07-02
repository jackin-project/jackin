//! `Z.AI` / `GLM` usage snapshot.
//!
//! Carved out of `usage.rs` during codebase-health-enforcement Workstream W5
//! (file-size ratchet). Items in this module are `pub(crate)` so the
//! coordinator (`usage.rs`) can re-export them.

#[allow(clippy::wildcard_imports)]
use super::*;
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
    usage_view(UsageViewInput {
        agent,
        provider: Some(surface.label()),
        surface,
        account_label: String::new(),
        username: None,
        plan_label: provider_quota
            .as_ref()
            .and_then(ZaiQuotaResponse::plan_name),
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
}

impl ZaiQuotaResponse {
    pub(crate) fn buckets(&self, now: i64) -> Vec<QuotaBucketView> {
        let mut limits = self
            .data
            .as_ref()
            .map(|data| data.limits.clone())
            .unwrap_or_default();
        limits.sort_by_key(|limit| limit.window_minutes().unwrap_or(i64::MAX));

        let mut token_limits = limits
            .iter()
            .filter(|limit| limit.limit_type == "TOKENS_LIMIT")
            .collect::<Vec<_>>();
        let time_limit = limits.iter().find(|limit| limit.limit_type == "TIME_LIMIT");
        let mut buckets = Vec::new();
        let mut session_token_limit = None;
        let mut primary_token_limit = None;
        if token_limits.len() >= 2 {
            token_limits.sort_by_key(|limit| limit.window_minutes().unwrap_or(i64::MAX));
            session_token_limit = token_limits.first().copied();
            primary_token_limit = token_limits.last().copied();
        } else if let Some(limit) = token_limits.first() {
            primary_token_limit = Some(*limit);
        }
        // render order is 5-hour (short/active), then Tokens, then MCP — an
        // operator override of CodexBar's Tokens, MCP, 5-hour order.
        if let Some(limit) = session_token_limit {
            buckets.push(with_status_slot(
                zai_bucket("5-hour", limit, now),
                Some(StatusSlot::Session),
            ));
        }
        if let Some(limit) = primary_token_limit {
            buckets.push(with_status_slot(
                zai_bucket("Tokens", limit, now),
                Some(StatusSlot::Weekly),
            ));
        }
        if let Some(limit) = time_limit {
            buckets.push(zai_bucket("MCP", limit, now));
        }
        buckets
    }

    pub(crate) fn plan_name(&self) -> Option<String> {
        self.data
            .as_ref()
            .and_then(|data| data.plan_name.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
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
            let percent = ((used.clamp(0, limit) as f64 / limit as f64) * 100.0)
                .round()
                .clamp(0.0, 100.0) as u8;
            return Some(percent);
        }
        self.percentage
            .map(|percent| percent.round().clamp(0.0, 100.0) as u8)
    }

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
}

pub(crate) fn zai_bucket(label: &str, limit: &ZaiLimitRaw, now: i64) -> QuotaBucketView {
    let used_percent = limit.used_percent();
    let remaining = used_percent.map(|used| 100u8.saturating_sub(used));
    let reset_at = limit.next_reset_time.map(|epoch_ms| epoch_ms / 1000);
    let detail = if label == "MCP" {
        zai_count_line(limit)
    } else {
        None
    };
    timed_bucket(
        label,
        limit
            .current_value
            .map(|value| compact_count(value.max(0) as u64)),
        limit.usage.map(|value| compact_count(value.max(0) as u64)),
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
        compact_count(used as u64),
        compact_count(total as u64),
        compact_count(remaining as u64)
    ))
}

pub(crate) fn fetch_zai_usage(token: &str) -> Result<ZaiQuotaResponse, String> {
    let url = resolve_zai_quota_url();
    let quota: ZaiQuotaResponse = get_json_bearer("Z.AI quota", &url, token, &[])?;
    if quota.success == Some(false) || quota.code.is_some_and(|code| code != 200) {
        return Err(format!(
            "Z.AI quota rejected response: {}",
            quota.msg.unwrap_or_else(|| "unknown error".to_owned())
        ));
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
