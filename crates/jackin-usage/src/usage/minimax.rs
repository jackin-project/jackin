//! `MiniMax` usage snapshot.
//!
//! Carved out of `usage.rs` during codebase-health-enforcement Workstream W5
//! (file-size ratchet). Items in this module are `pub(crate)` so the
//! coordinator (`usage.rs`) can re-export them.

#[allow(clippy::wildcard_imports, reason = "documented residual allow; prefer expect when site is lint-true")]
use super::*;
use serde::Deserialize;

pub(crate) fn minimax_snapshot(agent: &str, token: Option<&str>, now: i64) -> FocusedUsageView {
    let has_token = token.is_some_and(|value| !value.is_empty());
    let (provider_usage, provider_error) = split_fetch(token.map(fetch_minimax_usage));
    let (status, source, confidence) = provider_outcome(ProviderPresence {
        has_data: provider_usage.is_some(),
        has_secret: has_token,
    });
    let buckets = provider_usage
        .as_ref()
        .map(|usage| usage.buckets(now))
        .filter(|buckets| !buckets.is_empty())
        .unwrap_or_else(|| {
            vec![bucket(
                "Coding plan",
                None,
                None,
                None,
                None,
                provider_error
                    .as_deref()
                    .or(Some("MiniMax API-token endpoint unavailable")),
                status,
            )]
        });
    usage_view(UsageViewInput {
        agent,
        provider: Some(UsageSurface::Minimax.label()),
        surface: UsageSurface::Minimax,
        account_label: String::new(),
        username: None,
        plan_label: provider_usage
            .as_ref()
            .and_then(MiniMaxUsageResponse::plan_name),
        credential_origin: Some(
            if has_token {
                "API token · env MINIMAX_API_KEY"
            } else {
                "needs MINIMAX_CODING_API_KEY"
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
                Some("MiniMax API token is not available to Capsule".to_owned())
            }
            UsageSnapshotStatus::Unsupported => {
                Some(provider_error.unwrap_or_else(|| {
                    "MiniMax API-token endpoint unavailable to Capsule".to_owned()
                }))
            }
            _ => None,
        },
    })
}

#[derive(Debug, Deserialize)]
pub(crate) struct MiniMaxUsageResponse {
    #[serde(rename = "base_resp")]
    pub(crate) base_resp: Option<MiniMaxBaseResponse>,
    pub(crate) data: Option<MiniMaxUsageData>,
    #[serde(rename = "model_remains", default)]
    pub(crate) root_model_remains: Vec<MiniMaxModelRemain>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct MiniMaxBaseResponse {
    #[serde(rename = "status_code")]
    pub(crate) status_code: Option<i64>,
    #[serde(rename = "status_msg")]
    pub(crate) status_msg: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct MiniMaxUsageData {
    #[serde(rename = "base_resp")]
    pub(crate) base_resp: Option<MiniMaxBaseResponse>,
    #[serde(rename = "current_subscribe_title")]
    pub(crate) current_subscribe_title: Option<String>,
    #[serde(rename = "plan_name")]
    pub(crate) plan_name: Option<String>,
    #[serde(rename = "combo_title")]
    pub(crate) combo_title: Option<String>,
    #[serde(rename = "current_plan_title")]
    pub(crate) current_plan_title: Option<String>,
    #[serde(rename = "current_combo_card")]
    pub(crate) current_combo_card: Option<MiniMaxComboCard>,
    #[serde(rename = "model_remains", default)]
    pub(crate) model_remains: Vec<MiniMaxModelRemain>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct MiniMaxComboCard {
    pub(crate) title: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct MiniMaxModelRemain {
    #[serde(rename = "model_name")]
    pub(crate) model_name: Option<String>,
    #[serde(rename = "current_interval_total_count")]
    pub(crate) current_interval_total_count: Option<i64>,
    #[serde(rename = "current_interval_usage_count")]
    pub(crate) current_interval_usage_count: Option<i64>,
    #[serde(rename = "current_interval_remaining_percent")]
    pub(crate) current_interval_remaining_percent: Option<f64>,
    #[serde(rename = "current_interval_status")]
    pub(crate) current_interval_status: Option<i64>,
    #[serde(rename = "end_time")]
    pub(crate) end_time: Option<i64>,
    #[serde(rename = "remains_time")]
    pub(crate) remains_time: Option<i64>,
    #[serde(rename = "current_weekly_total_count")]
    pub(crate) current_weekly_total_count: Option<i64>,
    #[serde(rename = "current_weekly_usage_count")]
    pub(crate) current_weekly_usage_count: Option<i64>,
    #[serde(rename = "current_weekly_remaining_percent")]
    pub(crate) current_weekly_remaining_percent: Option<f64>,
    #[serde(rename = "current_weekly_status")]
    pub(crate) current_weekly_status: Option<i64>,
    #[serde(rename = "weekly_end_time")]
    pub(crate) weekly_end_time: Option<i64>,
    #[serde(rename = "weekly_remains_time")]
    pub(crate) weekly_remains_time: Option<i64>,
}

impl MiniMaxUsageResponse {
    pub(crate) fn validate(&self) -> Result<(), String> {
        let base = self
            .data
            .as_ref()
            .and_then(|data| data.base_resp.as_ref())
            .or(self.base_resp.as_ref());
        if let Some(status) = base.and_then(|base| base.status_code)
            && status != 0
        {
            return Err(base
                .and_then(|base| base.status_msg.clone())
                .unwrap_or_else(|| format!("status_code {status}")));
        }
        if self.model_remains().is_empty() {
            return Err("missing MiniMax coding plan data".to_owned());
        }
        Ok(())
    }

    pub(crate) fn buckets(&self, now: i64) -> Vec<QuotaBucketView> {
        let mut buckets = Vec::new();
        for remain in self.model_remains() {
            if let Some(bucket) = minimax_bucket(
                remain.model_name.as_deref().unwrap_or("MiniMax model"),
                MiniMaxWindow::Interval,
                remain.current_interval_total_count,
                remain.current_interval_usage_count,
                remain.current_interval_remaining_percent,
                remain.current_interval_status,
                remain.end_time,
                remain.remains_time,
                now,
            ) {
                buckets.push(bucket);
            }
            if minimax_is_general_model(remain.model_name.as_deref())
                && let Some(bucket) = minimax_bucket(
                    remain.model_name.as_deref().unwrap_or("MiniMax model"),
                    MiniMaxWindow::Weekly,
                    remain.current_weekly_total_count,
                    remain.current_weekly_usage_count,
                    remain.current_weekly_remaining_percent,
                    remain.current_weekly_status,
                    remain.weekly_end_time,
                    remain.weekly_remains_time,
                    now,
                )
            {
                buckets.push(bucket);
            }
        }
        buckets
    }

    pub(crate) fn plan_name(&self) -> Option<String> {
        let data = self.data.as_ref()?;
        [
            data.current_subscribe_title.as_deref(),
            data.plan_name.as_deref(),
            data.combo_title.as_deref(),
            data.current_plan_title.as_deref(),
            data.current_combo_card
                .as_ref()
                .and_then(|card| card.title.as_deref()),
        ]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|value| !value.is_empty())
        .map(str::to_owned)
    }

    pub(crate) fn model_remains(&self) -> Vec<&MiniMaxModelRemain> {
        if let Some(data) = &self.data
            && !data.model_remains.is_empty()
        {
            return data.model_remains.iter().collect();
        }
        self.root_model_remains.iter().collect()
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum MiniMaxWindow {
    Interval,
    Weekly,
}

#[allow(clippy::too_many_arguments, reason = "documented residual allow; prefer expect when site is lint-true")]
pub(crate) fn minimax_bucket(
    model_name: &str,
    window: MiniMaxWindow,
    total: Option<i64>,
    usage: Option<i64>,
    remaining_percent: Option<f64>,
    status: Option<i64>,
    end: Option<i64>,
    remains_time: Option<i64>,
    now: i64,
) -> Option<QuotaBucketView> {
    if matches!(status, Some(value) if !matches!(value, 0 | 1)) {
        return None;
    }
    if total.is_none() && usage.is_none() && remaining_percent.is_none() {
        return None;
    }
    let remaining_percent = if let Some(remaining_percent) = remaining_percent {
        #[expect(clippy::cast_sign_loss, reason = "clamped to 0.0..=100.0 above")]
        {
            Some(remaining_percent.round().clamp(0.0, 100.0) as u8)
        }
    } else {
        let total = total?;
        if total <= 0 {
            None
        } else {
            let usage = usage?;
            #[expect(
                clippy::cast_sign_loss,
                reason = "usage/total clamped non-negative; percent is rounded f64→u8"
            )]
            {
                Some(100u8.saturating_sub(
                    ((usage.clamp(0, total) as f64 / total as f64) * 100.0).round() as u8,
                ))
            }
        }
    };
    let used_label = usage.map(|usage| compact_count(u64::try_from(usage.max(0)).unwrap_or(0)));
    let reset_epoch = minimax_reset_epoch(end, remains_time, now);
    let detail = minimax_usage_count_line(usage, total, remaining_percent);
    // Only the general model fills the status-bar slots; per-model windows are
    // detail rows the headline ignores.
    let status_slot = match (minimax_is_general_model(Some(model_name)), window) {
        (true, MiniMaxWindow::Interval) => Some(StatusSlot::Session),
        (true, MiniMaxWindow::Weekly) => Some(StatusSlot::Weekly),
        _ => None,
    };
    let mut view = timed_bucket(
        &minimax_bucket_label(model_name, window),
        used_label,
        total
            .filter(|value| *value > 0)
            .map(|value| compact_count(u64::try_from(value.max(0)).unwrap_or(0))),
        remaining_percent,
        reset_epoch,
        now,
        detail.as_deref(),
        UsageSnapshotStatus::Fresh,
    );
    view.status_slot = status_slot;
    Some(view)
}

pub(crate) fn minimax_is_general_model(model_name: Option<&str>) -> bool {
    model_name.is_some_and(|value| value.eq_ignore_ascii_case("general"))
}

pub(crate) fn minimax_bucket_label(model_name: &str, window: MiniMaxWindow) -> String {
    let model = titlecase_ascii(model_name);
    match (minimax_is_general_model(Some(model_name)), window) {
        (true, MiniMaxWindow::Interval) => "General · 5h".to_owned(),
        (true, MiniMaxWindow::Weekly) => "General · Weekly".to_owned(),
        (false, MiniMaxWindow::Interval) => model,
        (false, MiniMaxWindow::Weekly) => format!("{model} · Weekly"),
    }
}

pub(crate) fn minimax_usage_count_line(
    usage: Option<i64>,
    total: Option<i64>,
    remaining_percent: Option<u8>,
) -> Option<String> {
    let usage = u64::try_from(usage?.max(0)).unwrap_or(0);
    let total = total.filter(|value| *value > 0).map_or_else(
        || remaining_percent.map(|_| 100),
        |value| Some(u64::try_from(value.max(0)).unwrap_or(0)),
    )?;
    Some(format!(
        "Usage: {} / {}",
        compact_count(usage),
        compact_count(total)
    ))
}

pub(crate) fn fetch_minimax_usage(token: &str) -> Result<MiniMaxUsageResponse, String> {
    let client = provider_http_client()?;
    let mut last_error = None;
    for url in resolve_minimax_remains_urls() {
        let response = match client
            .get(&url)
            .bearer_auth(token)
            .header(reqwest::header::ACCEPT, "application/json")
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header("MM-API-Source", "jackin-capsule")
            .send()
        {
            Ok(response) => response,
            Err(err) => {
                last_error = Some(format!("MiniMax usage request failed for {url}: {err}"));
                continue;
            }
        };
        let status = response.status();
        if !status.is_success() {
            last_error = Some(format!("MiniMax usage HTTP {status}"));
            continue;
        }
        let usage = response
            .json::<MiniMaxUsageResponse>()
            .map_err(|err| format!("MiniMax usage decode failed: {err}"))?;
        usage.validate()?;
        return Ok(usage);
    }
    Err(last_error.unwrap_or_else(|| "MiniMax usage endpoint unavailable".to_owned()))
}

pub(crate) fn resolve_minimax_remains_urls() -> Vec<String> {
    let override_url = env_value("MINIMAX_REMAINS_URL");
    let host = env_value("MINIMAX_API_HOST").or_else(|| env_value("MINIMAX_HOST"));
    resolve_minimax_remains_urls_from(override_url.as_deref(), host.as_deref())
}

pub(crate) fn resolve_minimax_remains_urls_from(
    override_url: Option<&str>,
    host: Option<&str>,
) -> Vec<String> {
    if let Some(url) = override_url {
        return vec![normalize_url_or_host(url, "")];
    }
    let mut urls = Vec::new();
    if let Some(host) = host {
        let host = minimax_remains_host(host);
        let host = host.trim_end_matches('/');
        urls.push(format!("{host}/v1/token_plan/remains"));
        urls.push(format!("{host}/v1/api/openplatform/coding_plan/remains"));
    } else {
        urls.push("https://api.minimax.io/v1/token_plan/remains".to_owned());
        urls.push("https://api.minimax.io/v1/api/openplatform/coding_plan/remains".to_owned());
        urls.push("https://api.minimaxi.com/v1/token_plan/remains".to_owned());
        urls.push("https://api.minimaxi.com/v1/api/openplatform/coding_plan/remains".to_owned());
    }
    urls
}

pub(crate) fn minimax_remains_host(value: &str) -> String {
    let normalized = normalize_url_or_host(value, "");
    let Ok(mut url) = url::Url::parse(&normalized) else {
        return normalized;
    };
    url.set_path("");
    url.set_query(None);
    url.set_fragment(None);
    url.to_string().trim_end_matches('/').to_owned()
}

pub(crate) fn minimax_reset_epoch(
    end: Option<i64>,
    remains_time: Option<i64>,
    now: i64,
) -> Option<i64> {
    end.map(epoch_seconds_from_maybe_ms)
        .or_else(|| remains_time.map(|seconds| now.saturating_add(seconds.max(0))))
}
