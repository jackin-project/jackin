// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `MiniMax` fetch plans, endpoints, and HTTP fetch.

use jackin_usage_provider_core::{
    env_value, epoch_seconds_from_maybe_ms, normalize_url_or_host, provider_http_client,
    provider_request,
};

use super::{
    MiniMaxBalanceResponse, MiniMaxFetched, MiniMaxKeyProduct, MiniMaxRegion, MiniMaxUsage,
    MiniMaxUsageResponse, minimax_key_product, minimax_region_from_value,
    resolve_minimax_region_from,
};

/// Ordered fetch candidates for one product plus the region and host label
/// the fetch actually targets.
#[derive(Debug, Clone)]
pub(crate) struct MiniMaxFetchPlan {
    pub(crate) urls: Vec<String>,
    pub(crate) region: MiniMaxRegion,
    pub(crate) host_label: String,
}

pub(crate) fn resolve_minimax_fetch_plan(product: MiniMaxKeyProduct) -> MiniMaxFetchPlan {
    let override_url = env_value("MINIMAX_REMAINS_URL");
    let host = env_value("MINIMAX_API_HOST").or_else(|| env_value("MINIMAX_HOST"));
    let region = env_value("MINIMAX_REGION");
    minimax_fetch_plan_from(
        product,
        override_url.as_deref(),
        host.as_deref(),
        region.as_deref(),
    )
}

pub(crate) fn minimax_fetch_plan_from(
    product: MiniMaxKeyProduct,
    override_url: Option<&str>,
    host: Option<&str>,
    region_env: Option<&str>,
) -> MiniMaxFetchPlan {
    if let Some(url) = override_url
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let url = normalize_url_or_host(url, "");
        let region = minimax_region_from_value(&url);
        return MiniMaxFetchPlan {
            host_label: minimax_host_label(&url),
            urls: vec![url],
            region,
        };
    }
    if let Some(host) = host.map(str::trim).filter(|value| !value.is_empty()) {
        let base = minimax_remains_host(host);
        let base = base.trim_end_matches('/');
        let region = minimax_region_from_value(base);
        let urls = match product {
            MiniMaxKeyProduct::TokenPlan => vec![
                format!("{base}/v1/token_plan/remains"),
                format!("{base}/v1/api/openplatform/coding_plan/remains"),
            ],
            MiniMaxKeyProduct::Payg => vec![format!("{base}/account/query_balance")],
        };
        return MiniMaxFetchPlan {
            host_label: minimax_host_label(base),
            urls,
            region,
        };
    }
    let region = resolve_minimax_region_from(region_env, None);
    let base = region.api_host();
    let urls = match product {
        // Region-pinned subset of the documented candidates: a credential is
        // never sent cross-region because the first request failed.
        MiniMaxKeyProduct::TokenPlan => resolve_minimax_remains_urls_from(None, None)
            .into_iter()
            .filter(|url| minimax_region_from_value(url) == region)
            .collect(),
        MiniMaxKeyProduct::Payg => vec![format!("{base}/account/query_balance")],
    };
    MiniMaxFetchPlan {
        host_label: minimax_host_label(base),
        urls,
        region,
    }
}

pub(crate) fn minimax_host_label(url: &str) -> String {
    url.trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or_default()
        .to_owned()
}

pub fn fetch_minimax_usage(token: &str) -> Result<MiniMaxFetched, String> {
    let product = minimax_key_product(token);
    let plan = resolve_minimax_fetch_plan(product);
    let client = provider_http_client()?;
    first_minimax_usage(plan.urls.clone(), |url| {
        fetch_minimax_url(&client, token, product, url).map(|usage| MiniMaxFetched {
            usage,
            region: plan.region,
            host_label: plan.host_label.clone(),
        })
    })
}

pub(crate) fn fetch_minimax_url(
    client: &reqwest::blocking::Client,
    token: &str,
    product: MiniMaxKeyProduct,
    url: &str,
) -> Result<MiniMaxUsage, String> {
    provider_request(
        jackin_telemetry::schema::enums::ProviderName::Minimax,
        "GET",
        minimax_operation_path(url),
        || {
            let response = client
                .get(url)
                .bearer_auth(token)
                .header(reqwest::header::ACCEPT, "application/json")
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .header("MM-API-Source", "jackin-capsule")
                .send()
                .map_err(|err| format!("MiniMax usage request failed for {url}: {err}"))?;
            let status = response.status();
            if !status.is_success() {
                return Err(format!("MiniMax usage HTTP {status}"));
            }
            match product {
                MiniMaxKeyProduct::TokenPlan => {
                    let usage = response
                        .json::<MiniMaxUsageResponse>()
                        .map_err(|err| format!("MiniMax usage decode failed: {err}"))?;
                    usage.validate()?;
                    Ok(MiniMaxUsage::TokenPlan(usage))
                }
                MiniMaxKeyProduct::Payg => {
                    let balance = response
                        .json::<MiniMaxBalanceResponse>()
                        .map_err(|err| format!("MiniMax balance decode failed: {err}"))?;
                    balance.validate()?;
                    Ok(MiniMaxUsage::Balance(balance))
                }
            }
        },
    )
}

pub fn resolve_minimax_remains_urls() -> Vec<String> {
    let override_url = env_value("MINIMAX_REMAINS_URL");
    let host = env_value("MINIMAX_API_HOST").or_else(|| env_value("MINIMAX_HOST"));
    resolve_minimax_remains_urls_from(override_url.as_deref(), host.as_deref())
}

pub fn resolve_minimax_remains_urls_from(
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
        urls.push("https://www.minimax.io/v1/token_plan/remains".to_owned());
    }
    urls
}

/// Iterate URLs in order, returning the first success or the last fetch
/// error. Extracted so fan-out order is unit-testable without provider I/O.
pub fn first_minimax_usage<T, F>(urls: Vec<String>, mut fetch: F) -> Result<T, String>
where
    F: FnMut(&str) -> Result<T, String>,
{
    let mut last_error = None;
    for url in urls {
        match fetch(&url) {
            Ok(usage) => return Ok(usage),
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.unwrap_or_else(|| "MiniMax usage endpoint unavailable".to_owned()))
}

/// Governed telemetry path template for a `MiniMax` remains URL. Known
/// endpoints map to their static path; arbitrary override URLs collapse to
/// `"/custom"` so operator-provided paths never leak into telemetry.
pub fn minimax_operation_path(url: &str) -> &'static str {
    if url.ends_with("/v1/token_plan/remains") {
        "/v1/token_plan/remains"
    } else if url.ends_with("/v1/api/openplatform/coding_plan/remains") {
        "/v1/api/openplatform/coding_plan/remains"
    } else if url.ends_with("/account/query_balance") {
        "/account/query_balance"
    } else {
        "/custom"
    }
}

pub fn minimax_remains_host(value: &str) -> String {
    let normalized = normalize_url_or_host(value, "");
    let Ok(mut url) = url::Url::parse(&normalized) else {
        return normalized;
    };
    url.set_path("");
    url.set_query(None);
    url.set_fragment(None);
    url.to_string().trim_end_matches('/').to_owned()
}

pub fn minimax_reset_epoch(end: Option<i64>, remains_time: Option<i64>, now: i64) -> Option<i64> {
    end.map(epoch_seconds_from_maybe_ms).or_else(|| {
        remains_time.map(|duration| now.saturating_add(minimax_duration_seconds(duration).max(0)))
    })
}

/// `remains_time` is a remaining-duration, not an epoch. Live values arrive in
/// milliseconds (`14_400_000` = 4h); values above a million can only be
/// milliseconds (a million seconds already exceeds any interval/weekly
/// window), so they are normalized — seconds pass through.
pub(crate) fn minimax_duration_seconds(duration: i64) -> i64 {
    if duration > 1_000_000 {
        duration / 1000
    } else {
        duration
    }
}
