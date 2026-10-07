// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `OpenRouter` snapshot entry points.

use jackin_protocol::control::{
    FocusedUsageView, UsageConfidence, UsageSnapshotStatus, UsageSource,
};
use jackin_usage_provider_core::{ProviderError, ProviderRateLimit};
use jackin_usage_provider_core::{
    ProviderHttpError, UsageSurface, UsageViewInput, bucket, usage_view,
};

use super::{
    OpenRouterCreditsOutcome, fetch_openrouter_credits, fetch_openrouter_key_usage,
    openrouter_base_url, openrouter_credits_bucket, openrouter_key_error_status,
    parse_openrouter_key_usage,
};

pub(crate) fn openrouter_snapshot(agent: &str, key: Option<&str>, now: i64) -> FocusedUsageView {
    openrouter_snapshot_with_rate_limit(agent, key, now).0
}

/// Key snapshot against an explicit base.
pub(crate) fn openrouter_snapshot_with_base(
    agent: &str,
    key: Option<&str>,
    base_url: &str,
    now: i64,
) -> FocusedUsageView {
    openrouter_snapshot_with_key_fetch(agent, key, base_url, now, fetch_openrouter_key_usage).0
}

pub(crate) fn openrouter_snapshot_with_rate_limit(
    agent: &str,
    key: Option<&str>,
    now: i64,
) -> (FocusedUsageView, Option<ProviderRateLimit>) {
    openrouter_snapshot_with_key_fetch(
        agent,
        key,
        &openrouter_base_url(),
        now,
        fetch_openrouter_key_usage,
    )
}

/// Snapshot boundary with an injectable key fetch. Production supplies the
/// shared HTTP fetcher; tests can drive transport/decode failures without
/// relying on an unreserved local port.
pub(crate) fn openrouter_snapshot_with_key_fetch<F>(
    agent: &str,
    key: Option<&str>,
    base_url: &str,
    now: i64,
    fetch_key: F,
) -> (FocusedUsageView, Option<ProviderRateLimit>)
where
    F: FnOnce(&str, &str) -> Result<serde_json::Value, ProviderHttpError>,
{
    let Some(key) = key.filter(|key| !key.trim().is_empty()) else {
        return (
            usage_view(UsageViewInput {
                agent,
                provider: Some("OpenRouter"),
                surface: UsageSurface::OpenRouter,
                account_label: "OpenRouter key missing".to_owned(),
                username: None,
                plan_label: None,
                credential_origin: None,
                buckets: vec![bucket(
                    "Usage",
                    None,
                    None,
                    None,
                    None,
                    Some("OpenRouter API key missing"),
                    UsageSnapshotStatus::NeedsLogin,
                )],
                status: UsageSnapshotStatus::NeedsLogin,
                source: UsageSource::None,
                confidence: UsageConfidence::None,
                now,
                last_error: Some("OpenRouter API key missing".to_owned()),
            }),
            None,
        );
    };
    let key_result = fetch_key(base_url, key)
        .map_err(ProviderError::from)
        .and_then(|value| parse_openrouter_key_usage(value, now).map_err(ProviderError::from));
    let (quota, key_error) = match key_result {
        Ok(quota) => (Some(quota), None),
        Err(error) => (None, Some(error)),
    };
    let status = key_error
        .as_ref()
        .map_or(UsageSnapshotStatus::Fresh, |error| {
            openrouter_key_error_status(error)
        });
    let rate_limit = key_error.as_ref().and_then(ProviderError::rate_limit);
    let key_error_message = key_error.as_ref().map(|error| error.message().to_owned());
    let mut buckets = quota.as_ref().map_or_else(
        || {
            vec![bucket(
                "Usage",
                None,
                None,
                None,
                None,
                key_error_message.as_deref(),
                status,
            )]
        },
        |quota| quota.buckets.clone(),
    );
    // `/credits` enriches but never suppresses: a Management-scope 403 keeps
    // the `/key` rows and surfaces as a note.
    let credits_note =
        (status == UsageSnapshotStatus::Fresh).then(|| {
            match fetch_openrouter_credits(base_url, key) {
                OpenRouterCreditsOutcome::Available {
                    spent_cents,
                    ceiling_cents,
                } => {
                    buckets.push(openrouter_credits_bucket(spent_cents, ceiling_cents));
                    None
                }
                OpenRouterCreditsOutcome::ManagementScopeDenied => Some(
                    "OpenRouter account credits need a Management key; showing key usage only"
                        .to_owned(),
                ),
                OpenRouterCreditsOutcome::Unavailable(error) => Some(error),
            }
        });
    let view = usage_view(UsageViewInput {
        agent,
        provider: Some("OpenRouter"),
        surface: UsageSurface::OpenRouter,
        account_label: "OpenRouter key".to_owned(),
        username: None,
        plan_label: quota.as_ref().and_then(|quota| quota.plan_label.clone()),
        credential_origin: Some("API token · OpenRouter key".to_owned()),
        buckets,
        status,
        source: if status == UsageSnapshotStatus::Fresh {
            UsageSource::ProviderApi
        } else {
            UsageSource::None
        },
        confidence: if status == UsageSnapshotStatus::Fresh {
            UsageConfidence::Authoritative
        } else {
            UsageConfidence::None
        },
        now,
        last_error: key_error_message.or_else(|| credits_note.flatten()),
    });
    (view, rate_limit)
}
