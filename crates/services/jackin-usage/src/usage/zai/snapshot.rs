// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Z.AI` provider-key snapshot entry point.

use jackin_protocol::control::{FocusedUsageView, UsageSnapshotStatus};
use jackin_usage_provider_core::{
    ProviderPresence, UsageSurface, UsageViewInput, bucket, provider_outcome, split_fetch,
    usage_view,
};

use super::{ZaiQuotaResponse, fetch_zai_usage, resolve_zai_team_scope};

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
