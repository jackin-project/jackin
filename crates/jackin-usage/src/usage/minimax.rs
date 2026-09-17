// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `MiniMax` usage snapshot.
//!
//! Carved out of `usage.rs` for the file-size ratchet. Items in this module
//! are `pub(crate)` so the coordinator (`usage.rs`) can re-export them.
//!
//! Two products, selected by key shape (see `ref-contracts-B.md` §3): Token
//! Plan subscription keys read per-model interval/weekly remains from
//! `GET {apiBase}/v1/token_plan/remains` (legacy fallback
//! `.../v1/api/openplatform/coding_plan/remains`); secret `sk-api-*` keys
//! read PAYG balances from `GET {base}/account/query_balance`. A credential
//! is never sent to another region because the first request failed: the
//! default region is global, and CN hosts are used only when explicitly
//! selected.

#[cfg_attr(
    not(test),
    expect(clippy::wildcard_imports, reason = "target-dependent")
)]
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
                // The key shape names the product even when the fetch failed.
                match token.map(minimax_key_product) {
                    Some(MiniMaxKeyProduct::Payg) => "Balance",
                    _ => "Coding plan",
                },
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
    let credential_origin = if has_token {
        let mut origin = "API token · env MINIMAX_API_KEY".to_owned();
        if let Some(fetched) = &provider_usage {
            origin.push_str(" · ");
            origin.push_str(&fetched.host_label);
        }
        origin
    } else {
        "needs MINIMAX_CODING_API_KEY".to_owned()
    };
    usage_view(UsageViewInput {
        agent,
        provider: Some(UsageSurface::Minimax.label()),
        surface: UsageSurface::Minimax,
        account_label: String::new(),
        username: None,
        plan_label: provider_usage.as_ref().and_then(MiniMaxFetched::plan_label),
        credential_origin: Some(credential_origin),
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

/// Key product selected by key shape: secret `sk-api-*` keys are PAYG
/// balance keys; anything else is a Token Plan subscription key. Evidence:
/// `ref-contracts-B.md` §3 — `GET {base}/account/query_balance` serves
/// secret `sk-api-*` keys only, selected by `selectUsageEndpoint` on the
/// `sk-api-` prefix (`minimax-cli` `src/client/endpoints.ts:50-84`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MiniMaxKeyProduct {
    TokenPlan,
    Payg,
}

pub(crate) fn minimax_key_product(token: &str) -> MiniMaxKeyProduct {
    if token.trim_start().starts_with("sk-api-") {
        MiniMaxKeyProduct::Payg
    } else {
        MiniMaxKeyProduct::TokenPlan
    }
}

/// Billing region: global (`api.minimax.io`, USD) vs CN (`api.minimaxi.com`,
/// CNY). Region and currency travel together so a balance is never shown
/// without both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MiniMaxRegion {
    Global,
    China,
}

impl MiniMaxRegion {
    pub(crate) fn api_host(self) -> &'static str {
        match self {
            Self::Global => "https://api.minimax.io",
            Self::China => "https://api.minimaxi.com",
        }
    }

    pub(crate) fn currency(self) -> &'static str {
        match self {
            Self::Global => "USD",
            Self::China => "CNY",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::China => "CN",
        }
    }
}

pub(crate) fn minimax_region_from_value(value: &str) -> MiniMaxRegion {
    if value.to_ascii_lowercase().contains("minimaxi.com") {
        MiniMaxRegion::China
    } else {
        MiniMaxRegion::Global
    }
}

/// Region selection: explicit `MINIMAX_REGION` (`cn`/`china`/`minimaxi` →
/// CN) wins, else a CN host override implies CN, else global.
pub(crate) fn resolve_minimax_region() -> MiniMaxRegion {
    let host = env_value("MINIMAX_API_HOST").or_else(|| env_value("MINIMAX_HOST"));
    resolve_minimax_region_from(env_value("MINIMAX_REGION").as_deref(), host.as_deref())
}

pub(crate) fn resolve_minimax_region_from(
    region_env: Option<&str>,
    host_override: Option<&str>,
) -> MiniMaxRegion {
    if let Some(region) = region_env.map(str::trim).filter(|value| !value.is_empty()) {
        let region = region.to_ascii_lowercase();
        if ["cn", "china", "minimaxi", "minimaxi.com"]
            .iter()
            .any(|known| region.contains(known))
        {
            return MiniMaxRegion::China;
        }
        // Any other explicit value (or none) means global: the safe default
        // never routes a credential at the CN hosts.
        return MiniMaxRegion::Global;
    }
    host_override.map_or(MiniMaxRegion::Global, minimax_region_from_value)
}

/// A successful `MiniMax` fetch: the decoded product payload plus the region
/// and host label that actually served it.
#[derive(Debug)]
pub(crate) struct MiniMaxFetched {
    pub(crate) usage: MiniMaxUsage,
    pub(crate) region: MiniMaxRegion,
    pub(crate) host_label: String,
}

#[derive(Debug)]
pub(crate) enum MiniMaxUsage {
    TokenPlan(MiniMaxUsageResponse),
    Balance(MiniMaxBalanceResponse),
}

impl MiniMaxFetched {
    pub(crate) fn buckets(&self, now: i64) -> Vec<QuotaBucketView> {
        match &self.usage {
            MiniMaxUsage::TokenPlan(usage) => usage.buckets(now),
            MiniMaxUsage::Balance(balance) => balance.buckets(self.region),
        }
    }

    pub(crate) fn plan_label(&self) -> Option<String> {
        match &self.usage {
            MiniMaxUsage::TokenPlan(usage) => usage.plan_name(),
            MiniMaxUsage::Balance(_) => Some("PAYG".to_owned()),
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct MiniMaxUsageResponse {
    #[serde(rename = "base_resp")]
    pub(crate) base_resp: Option<MiniMaxBaseResponse>,
    pub(crate) data: Option<MiniMaxUsageData>,
    #[serde(rename = "model_remains", default)]
    pub(crate) root_model_remains: Vec<MiniMaxModelRemain>,
}

#[derive(Debug, Clone, Deserialize)]
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
    #[serde(
        rename = "interval_boost_permille",
        alias = "interval_boost_permill",
        alias = "current_interval_boost_permille",
        alias = "current_interval_boost_permill"
    )]
    pub(crate) interval_boost_permille: Option<f64>,
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
    #[serde(
        rename = "weekly_boost_permille",
        alias = "weekly_boost_permill",
        alias = "current_weekly_boost_permille",
        alias = "current_weekly_boost_permill"
    )]
    pub(crate) weekly_boost_permille: Option<f64>,
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
                remain.interval_boost_permille,
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
                    remain.weekly_boost_permille,
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

/// Window status codes: `0`/`1` normal, `2` exhausted, `3` unlimited.
pub(crate) fn minimax_window_exhausted(status: Option<i64>) -> bool {
    status == Some(2)
}

pub(crate) fn minimax_window_unlimited(status: Option<i64>) -> bool {
    status == Some(3)
}

#[expect(
    clippy::too_many_arguments,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) fn minimax_bucket(
    model_name: &str,
    window: MiniMaxWindow,
    total: Option<i64>,
    usage: Option<i64>,
    remaining_percent: Option<f64>,
    boost_permille: Option<f64>,
    status: Option<i64>,
    end: Option<i64>,
    remains_time: Option<i64>,
    now: i64,
) -> Option<QuotaBucketView> {
    // Only the general model fills the status-bar slots; per-model windows are
    // detail rows the headline ignores.
    let status_slot = match (minimax_is_general_model(Some(model_name)), window) {
        (true, MiniMaxWindow::Interval) => Some(StatusSlot::Session),
        (true, MiniMaxWindow::Weekly) => Some(StatusSlot::Weekly),
        _ => None,
    };
    let reset_epoch = minimax_reset_epoch(end, remains_time, now);
    if minimax_window_unlimited(status) {
        let mut view = timed_bucket(
            &minimax_bucket_label(model_name, window),
            usage.map(|usage| compact_count(u64::try_from(usage.max(0)).unwrap_or(0))),
            None,
            None,
            reset_epoch,
            now,
            Some("Unlimited"),
            UsageSnapshotStatus::Fresh,
        );
        view.status_slot = status_slot;
        return Some(view);
    }
    if matches!(status, Some(value) if !matches!(value, 0..=2)) {
        return None;
    }
    if total.is_none() && usage.is_none() && remaining_percent.is_none() {
        return None;
    }
    let remaining_percent = if minimax_window_exhausted(status) {
        Some(0)
    } else if let Some(remaining_percent) = remaining_percent {
        minimax_effective_remaining(Some(remaining_percent), boost_permille)
    } else {
        let total = total?;
        if total <= 0 {
            None
        } else {
            let usage = usage?;
            minimax_effective_remaining(
                Some(100.0 - (usage.clamp(0, total) as f64 / total as f64) * 100.0),
                boost_permille,
            )
        }
    };
    let used_label = usage.map(|usage| compact_count(u64::try_from(usage.max(0)).unwrap_or(0)));
    let mut pace = minimax_usage_count_line(usage, total, remaining_percent);
    if let Some(note) = minimax_boost_note(boost_permille) {
        pace = Some(match pace {
            Some(line) => format!("{line} · {note}"),
            None => note,
        });
    }
    if minimax_window_exhausted(status) {
        pace = Some(match pace {
            Some(line) => format!("{line} · Exhausted"),
            None => "Exhausted".to_owned(),
        });
    }
    let mut view = timed_bucket(
        &minimax_bucket_label(model_name, window),
        used_label,
        total
            .filter(|value| *value > 0)
            .map(|value| compact_count(u64::try_from(value.max(0)).unwrap_or(0))),
        remaining_percent,
        reset_epoch,
        now,
        pace.as_deref(),
        UsageSnapshotStatus::Fresh,
    );
    view.status_slot = status_slot;
    Some(view)
}

/// Apply the interval/weekly boost: rendered remaining = base ×
/// (`boost_permille` / 1000), which can exceed 100%. The raw over-cap value is
/// kept (bar geometry clamps at render); only the `u8` carrier bounds it.
pub(crate) fn minimax_effective_remaining(
    base_percent: Option<f64>,
    boost_permille: Option<f64>,
) -> Option<u8> {
    let base = base_percent
        .filter(|base| base.is_finite())
        .map(|base| base.max(0.0))?;
    let scaled = match boost_permille.filter(|boost| boost.is_finite() && *boost > 0.0) {
        Some(boost) => base * boost / 1000.0,
        None => base,
    };
    #[expect(
        clippy::cast_sign_loss,
        reason = "base clamped non-negative and boost positive; rounded f64→u8"
    )]
    {
        Some(scaled.round().clamp(0.0, 255.0) as u8)
    }
}

/// Human note for a non-trivial boost, e.g. `+20% boost` for permille 1200.
pub(crate) fn minimax_boost_note(boost_permille: Option<f64>) -> Option<String> {
    let boost = boost_permille.filter(|boost| boost.is_finite() && *boost > 0.0)?;
    if (boost - 1000.0).abs() < f64::EPSILON {
        return None;
    }
    if boost > 1000.0 {
        Some(format!("+{}% boost", (boost / 10.0 - 100.0).round() as i64))
    } else {
        Some(format!("{}% of base", (boost / 10.0).round() as i64))
    }
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

/// PAYG balance (`GET {base}/account/query_balance`, `sk-api-*` keys only).
/// Amounts are decimal strings; a balance is always shown with its region
/// currency, never bare.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct MiniMaxBalanceResponse {
    #[serde(rename = "base_resp")]
    pub(crate) base_resp: Option<MiniMaxBaseResponse>,
    #[serde(rename = "available_amount")]
    pub(crate) available_amount: Option<String>,
    #[serde(rename = "cash_balance")]
    pub(crate) cash_balance: Option<String>,
    #[serde(rename = "voucher_balance")]
    pub(crate) voucher_balance: Option<String>,
    #[serde(rename = "credit_balance")]
    pub(crate) credit_balance: Option<String>,
    #[serde(rename = "owed_amount")]
    pub(crate) owed_amount: Option<String>,
}

impl MiniMaxBalanceResponse {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if let Some(status) = self.base_resp.as_ref().and_then(|base| base.status_code)
            && status != 0
        {
            return Err(self
                .base_resp
                .as_ref()
                .and_then(|base| base.status_msg.clone())
                .unwrap_or_else(|| format!("status_code {status}")));
        }
        if self.available_amount.is_none()
            && self.cash_balance.is_none()
            && self.voucher_balance.is_none()
            && self.credit_balance.is_none()
            && self.owed_amount.is_none()
        {
            return Err("missing MiniMax balance data".to_owned());
        }
        Ok(())
    }

    pub(crate) fn buckets(&self, region: MiniMaxRegion) -> Vec<QuotaBucketView> {
        let currency = region.currency();
        let mut parts = vec![currency.to_owned()];
        for (label, amount) in [
            ("cash", self.cash_balance.as_deref()),
            ("voucher", self.voucher_balance.as_deref()),
            ("credit", self.credit_balance.as_deref()),
            ("owed", self.owed_amount.as_deref()),
        ] {
            if let Some(amount) = amount.map(str::trim).filter(|value| !value.is_empty()) {
                parts.push(format!("{label} {amount}"));
            }
        }
        // A balance is funds, not used-of-limit: amounts ride in labels only,
        // never as mislabeled `Money` spend.
        vec![bucket(
            "Balance",
            self.available_amount
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned),
            None,
            None,
            None,
            Some(&parts.join(" · ")),
            UsageSnapshotStatus::Fresh,
        )]
    }
}

/// Parse a decimal balance string to minor units (exponent 2) without float
/// rounding: `12.5` → `1250`, `-3.456` → `-345` (truncated, never rounded
/// up across the owed boundary).
pub(crate) fn minimax_decimal_minor(text: &str) -> Option<i64> {
    let text = text.trim();
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let (major, minor) = match digits.split_once('.') {
        Some((major, minor)) => (major, minor),
        None => (digits, ""),
    };
    if major.is_empty() || !major.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let mut minor_digits: String = minor
        .bytes()
        .take_while(u8::is_ascii_digit)
        .map(|byte| byte as char)
        .collect();
    if minor_digits.len() != minor.len() {
        return None;
    }
    while minor_digits.len() < 2 {
        minor_digits.push('0');
    }
    minor_digits.truncate(2);
    let major_value: i64 = major.parse().ok()?;
    let minor_value: i64 = minor_digits.parse().ok()?;
    let total = major_value.checked_mul(100)?.checked_add(minor_value)?;
    Some(if negative { -total } else { total })
}

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

pub(crate) fn fetch_minimax_usage(token: &str) -> Result<MiniMaxFetched, String> {
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
        urls.push("https://www.minimax.io/v1/token_plan/remains".to_owned());
    }
    urls
}

/// Iterate URLs in order, returning the first success or the last fetch
/// error. Extracted so fan-out order is unit-testable without provider I/O.
pub(crate) fn first_minimax_usage<T, F>(urls: Vec<String>, mut fetch: F) -> Result<T, String>
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
pub(crate) fn minimax_operation_path(url: &str) -> &'static str {
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

#[cfg(test)]
mod tests;
