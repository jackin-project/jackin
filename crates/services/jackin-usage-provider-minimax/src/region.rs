// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_usage_provider_core::env_value;

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

        return MiniMaxRegion::Global;
    }
    host_override.map_or(MiniMaxRegion::Global, minimax_region_from_value)
}
