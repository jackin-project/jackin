// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `MiniMax` key products and region selection.

use jackin_usage_provider_core::env_value;

/// Key product selected by key shape: secret `sk-api-*` keys are PAYG
/// balance keys; anything else is a Token Plan subscription key. Evidence:
/// `ref-contracts-B.md` §3 — `GET {base}/account/query_balance` serves
/// secret `sk-api-*` keys only, selected by `selectUsageEndpoint` on the
/// `sk-api-` prefix (`minimax-cli` `src/client/endpoints.ts:50-84`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MiniMaxKeyProduct {
    TokenPlan,
    Payg,
}

pub fn minimax_key_product(token: &str) -> MiniMaxKeyProduct {
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
pub enum MiniMaxRegion {
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

pub fn minimax_region_from_value(value: &str) -> MiniMaxRegion {
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

pub fn resolve_minimax_region_from(
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
