// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::control::{
    FocusedUsageView, QuotaBucketView, UsageConfidence, UsageSnapshotStatus, UsageSource,
};
use jackin_usage_provider_core::{
    ProviderError, ProviderFailureMetadata, ProviderRateLimit, split_provider_fetch,
};
use jackin_usage_provider_core::{UsageSurface, UsageViewInput, bucket, usage_view};

use super::{
    ClaudeOAuthUsageResponse, ClaudeResolved, ClaudeWaveResolution, fetch_claude_oauth_usage,
};

pub fn claude_wave_policy(resolution: &ClaudeWaveResolution) -> ClaudeWavePolicy {
    match resolution {
        ClaudeWaveResolution::Denied => ClaudeWavePolicy::LocalDenied,
        ClaudeWaveResolution::Missing => ClaudeWavePolicy::LocalMissing,
        ClaudeWaveResolution::Resolved(resolved) if resolved.is_anonymous => {
            ClaudeWavePolicy::LocalAnonymous
        }
        ClaudeWaveResolution::Resolved(_) => ClaudeWavePolicy::Shared,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaudeWavePolicy {
    Shared,
    LocalDenied,
    LocalMissing,
    LocalAnonymous,
}

pub fn claude_view_from_wave(
    agent: &str,
    provider: Option<&str>,
    now: i64,
    resolution: ClaudeWaveResolution,
) -> (
    FocusedUsageView,
    Option<ProviderRateLimit>,
    Option<ProviderFailureMetadata>,
) {
    match resolution {
        ClaudeWaveResolution::Denied => (claude_denied_view(agent, provider, now), None, None),
        ClaudeWaveResolution::Missing => (claude_missing_view(agent, provider, now), None, None),
        ClaudeWaveResolution::Resolved(resolved) => {
            claude_resolved_view(agent, provider, now, *resolved)
        }
    }
}

fn claude_denied_view(agent: &str, provider: Option<&str>, now: i64) -> FocusedUsageView {
    usage_view(UsageViewInput {
        agent,
        provider,
        surface: UsageSurface::Claude,
        account_label: String::new(),
        username: None,
        plan_label: None,
        credential_origin: None,
        buckets: Vec::new(),
        status: UsageSnapshotStatus::NeedsLogin,
        source: UsageSource::None,
        confidence: UsageConfidence::None,
        now,
        last_error: Some("Claude Keychain access denied".to_owned()),
    })
}

fn claude_pending_buckets(
    status: UsageSnapshotStatus,
    provider_error: Option<&str>,
) -> Vec<QuotaBucketView> {
    ["Session", "Weekly", "Daily Routines"]
        .into_iter()
        .map(|label| {
            bucket(
                label,
                None,
                None,
                None,
                None,
                provider_error.or(Some("provider API pending")),
                status,
            )
        })
        .collect()
}

fn claude_missing_view(agent: &str, provider: Option<&str>, now: i64) -> FocusedUsageView {
    usage_view(UsageViewInput {
        agent,
        provider,
        surface: UsageSurface::Claude,
        account_label: String::new(),
        username: None,
        plan_label: None,
        credential_origin: None,
        buckets: claude_pending_buckets(UsageSnapshotStatus::NeedsLogin, None),
        status: UsageSnapshotStatus::NeedsLogin,
        source: UsageSource::None,
        confidence: UsageConfidence::None,
        now,
        last_error: Some("Claude credentials not available to Capsule".to_owned()),
    })
}

pub fn claude_error_is_scope_restriction(error: &ProviderError) -> bool {
    error.status() == Some(403)
}

pub fn claude_provider_error_label(error: Option<&ProviderError>) -> Option<String> {
    let error = error?;
    if claude_error_is_scope_restriction(error) {
        return Some(format!(
            "{}; Claude token lacks usage scope (inference-only); quota unavailable",
            error.message()
        ));
    }
    Some(error.message().to_owned())
}

fn claude_resolved_view(
    agent: &str,
    provider: Option<&str>,
    now: i64,
    resolved: ClaudeResolved,
) -> (
    FocusedUsageView,
    Option<ProviderRateLimit>,
    Option<ProviderFailureMetadata>,
) {
    claude_resolved_view_with_fetch(agent, provider, now, resolved, fetch_claude_oauth_usage)
}

pub(crate) fn claude_resolved_view_with_fetch<F>(
    agent: &str,
    provider: Option<&str>,
    now: i64,
    resolved: ClaudeResolved,
    fetch: F,
) -> (
    FocusedUsageView,
    Option<ProviderRateLimit>,
    Option<ProviderFailureMetadata>,
)
where
    F: FnOnce(
        &str,
    )
        -> Result<ClaudeOAuthUsageResponse, jackin_usage_provider_core::ProviderHttpError>,
{
    let result = fetch(resolved.access_token());
    claude_resolved_view_from_result(agent, provider, now, resolved, result)
}

pub(crate) fn claude_resolved_view_from_result(
    agent: &str,
    provider: Option<&str>,
    now: i64,
    resolved: ClaudeResolved,
    result: Result<ClaudeOAuthUsageResponse, jackin_usage_provider_core::ProviderHttpError>,
) -> (
    FocusedUsageView,
    Option<ProviderRateLimit>,
    Option<ProviderFailureMetadata>,
) {
    let (oauth_quota, oauth_error) =
        split_provider_fetch(Some(result.map_err(ProviderError::from)));
    let provider_error = claude_provider_error_label(oauth_error.as_ref());
    let status = if oauth_quota.is_some() {
        UsageSnapshotStatus::Fresh
    } else if oauth_error
        .as_ref()
        .is_some_and(|error| error.status() == Some(401))
    {
        UsageSnapshotStatus::NeedsLogin
    } else {
        UsageSnapshotStatus::Stale
    };
    let rate_limit = oauth_error.as_ref().and_then(ProviderError::rate_limit);
    let failure_metadata = oauth_error.as_ref().map(ProviderError::metadata);
    let buckets = oauth_quota
        .map(|usage| usage.into_buckets(now))
        .filter(|buckets| !buckets.is_empty())
        .unwrap_or_else(|| claude_pending_buckets(status, provider_error.as_deref()));
    let view = usage_view(UsageViewInput {
        agent,
        provider,
        surface: UsageSurface::Claude,
        account_label: resolved.account_email.unwrap_or_default(),
        username: None,
        plan_label: resolved.organization_type.or(resolved.subscription_type),
        credential_origin: Some(resolved.credential_origin),
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
        last_error: claude_resolved_last_error(status, provider_error),
    });
    (view, rate_limit, failure_metadata)
}

pub(crate) fn claude_resolved_last_error(
    status: UsageSnapshotStatus,
    provider_error: Option<String>,
) -> Option<String> {
    match status {
        UsageSnapshotStatus::Stale | UsageSnapshotStatus::NeedsLogin => {
            Some(provider_error.unwrap_or_else(|| {
                "Claude provider usage unavailable; cached quota is stale".to_owned()
            }))
        }
        _ => None,
    }
}
