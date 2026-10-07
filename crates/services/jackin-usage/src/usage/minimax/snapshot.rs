// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `MiniMax` snapshot entry point.

use jackin_protocol::control::{FocusedUsageView, UsageSnapshotStatus};
use jackin_usage_provider_core::{
    ProviderPresence, UsageSurface, UsageViewInput, bucket, provider_outcome, split_fetch,
    usage_view,
};

use super::{MiniMaxFetched, MiniMaxKeyProduct, fetch_minimax_usage, minimax_key_product};

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
