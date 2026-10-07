// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Claude` refresh-wave policy and resolved views.

use jackin_protocol::control::{
    FocusedUsageView, QuotaBucketView, UsageConfidence, UsageSnapshotStatus, UsageSource,
};
use jackin_usage_provider_core::{ProviderError, ProviderRateLimit, split_provider_fetch};
use jackin_usage_provider_core::{
    UsageSurface, UsageViewInput, bucket, oauth_origin, resolve_identity_with_extra, usage_view,
};
use std::path::{Path, PathBuf};

use super::{
    ClaudeCliUsage, ClaudeFileProbe, ClaudeOAuthEnvToken, ClaudeResolved, ClaudeWaveResolution,
    claude_email_from_value, claude_oauth_candidates, claude_oauth_from_value,
    claude_organization_type_from_value, fetch_claude_cli_usage, fetch_claude_oauth_usage,
};

/// Read only the Claude Code OAuth environment credential. Anthropic API keys
/// use a different authentication protocol and must never reach the OAuth
/// usage endpoint through the standalone resolver.
pub(crate) fn read_claude_oauth_env_token<F>(mut read: F) -> Option<ClaudeOAuthEnvToken>
where
    F: FnMut(&str) -> Result<String, std::env::VarError>,
{
    read(jackin_core::CLAUDE_CODE_OAUTH_TOKEN_ENV_NAME)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(ClaudeOAuthEnvToken::new)
}

/// One-pass file/metadata probe for a Keychain scope. Default scope keeps
/// today's home-first candidate order (config dir, `~/.claude`, `~/.claude.json`,
/// handoff); a custom scope reads only its own normalized dir and never the
/// default home, default service, or handoff.
pub(crate) fn claude_scope_file_probe(
    scope: &jackin_core::ClaudeKeychainScope,
    config: &Path,
) -> ClaudeFileProbe {
    let candidates: Vec<PathBuf> = if scope.is_default {
        claude_oauth_candidates(config).to_vec()
    } else {
        vec![
            scope.normalized_config_dir.join(".credentials.json"),
            scope.normalized_config_dir.join(".claude.json"),
        ]
    };
    let (resolved, account_email, organization_type) = resolve_identity_with_extra(
        &candidates,
        claude_oauth_from_value,
        claude_email_from_value,
        claude_organization_type_from_value,
    );
    let (path, credential) = resolved.unzip();
    ClaudeFileProbe {
        credential,
        origin: path.as_deref().map(oauth_origin),
        account_email,
        organization_type,
    }
}

/// Classify the typed cache/coordination policy for a resolved wave. Denied,
/// Missing, and anonymous-credential resolutions are local-only.
pub(crate) fn claude_wave_policy(resolution: &ClaudeWaveResolution) -> ClaudeWavePolicy {
    match resolution {
        ClaudeWaveResolution::Denied => ClaudeWavePolicy::LocalDenied,
        ClaudeWaveResolution::Missing => ClaudeWavePolicy::LocalMissing,
        ClaudeWaveResolution::Resolved(resolved) if resolved.is_anonymous => {
            ClaudeWavePolicy::LocalAnonymous
        }
        ClaudeWaveResolution::Resolved(_) => ClaudeWavePolicy::Shared,
    }
}

/// Typed policy outcome for a Claude wave — the source of the cache/coordination
/// policy so downstream code never inspects error text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClaudeWavePolicy {
    Shared,
    LocalDenied,
    LocalMissing,
    LocalAnonymous,
}

pub(crate) fn claude_view_from_wave_with_rate_limit(
    agent: &str,
    provider: Option<&str>,
    now: i64,
    resolution: ClaudeWaveResolution,
) -> (FocusedUsageView, Option<ProviderRateLimit>) {
    match resolution {
        ClaudeWaveResolution::Denied => (claude_denied_view(agent, provider, now), None),
        ClaudeWaveResolution::Missing => (claude_missing_view(agent, provider, now), None),
        ClaudeWaveResolution::Resolved(resolved) => {
            claude_resolved_view(agent, provider, now, *resolved)
        }
    }
}

/// Terminal denial view: `NeedsLogin` with no bucket/account/plan/origin and the
/// exact non-secret error. Cached quota is never restored onto this (the typed
/// local-only policy blocks preservation in the refresh cache).
pub(crate) fn claude_denied_view(
    agent: &str,
    provider: Option<&str>,
    now: i64,
) -> FocusedUsageView {
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

pub(crate) fn claude_pending_buckets(
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

pub(crate) fn claude_missing_view(
    agent: &str,
    provider: Option<&str>,
    now: i64,
) -> FocusedUsageView {
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

/// True when the OAuth usage fetch failed because the token lacks the quota
/// scope (an inference-only grant): only a typed HTTP 403. A 401, another
/// status, or any transport/decode/CLI failure is not scope restriction. Pure
/// so the inference-only state is unit-testable without provider I/O.
pub(crate) fn claude_error_is_scope_restriction(error: &ProviderError) -> bool {
    error.status() == Some(403)
}

/// Pick the provider error label for a resolved view: OAuth first, CLI second.
/// A scope-restricted OAuth failure normalizes to the explicit inference-only
/// message so the operator sees *why* quota is unavailable instead of a bare
/// HTTP status; every other error passes through verbatim.
pub(crate) fn claude_provider_error_label(
    oauth_error: Option<&ProviderError>,
    cli_error: Option<&ProviderError>,
) -> Option<String> {
    let error = oauth_error.or(cli_error)?;
    if oauth_error.is_some_and(claude_error_is_scope_restriction) {
        return Some(
            "Claude token lacks usage scope (inference-only); quota unavailable".to_owned(),
        );
    }
    Some(error.message().to_owned())
}

pub(crate) fn claude_resolved_view(
    agent: &str,
    provider: Option<&str>,
    now: i64,
    resolved: ClaudeResolved,
) -> (FocusedUsageView, Option<ProviderRateLimit>) {
    let (oauth_quota, oauth_error) = split_provider_fetch(Some(
        fetch_claude_oauth_usage(&resolved.access_token).map_err(ProviderError::from),
    ));
    let (cli_usage, cli_error) =
        split_provider_fetch(oauth_quota.is_none().then(fetch_claude_cli_usage));
    let provider_error = claude_provider_error_label(oauth_error.as_ref(), cli_error.as_ref());
    let status = if oauth_quota.is_some() || cli_usage.is_some() {
        UsageSnapshotStatus::Fresh
    } else {
        UsageSnapshotStatus::Stale
    };
    let rate_limit = (status != UsageSnapshotStatus::Fresh)
        .then_some(oauth_error.as_ref().or(cli_error.as_ref()))
        .flatten()
        .and_then(ProviderError::rate_limit);
    let buckets = oauth_quota
        .map(|usage| usage.into_buckets(now))
        .or_else(|| cli_usage.as_ref().map(ClaudeCliUsage::buckets))
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
            if cli_usage.is_some() {
                UsageSource::Cli
            } else {
                UsageSource::ProviderApi
            }
        } else {
            UsageSource::None
        },
        confidence: if status == UsageSnapshotStatus::Fresh {
            if cli_usage.is_some() {
                UsageConfidence::Estimated
            } else {
                UsageConfidence::Authoritative
            }
        } else {
            UsageConfidence::None
        },
        now,
        last_error: claude_resolved_last_error(status, provider_error, cli_usage.is_some()),
    });
    (view, rate_limit)
}

/// `last_error` for a resolved view: the normalized provider error when stale,
/// the (already normalized) provider error on CLI fallback so the explicit
/// scope text surfaces there too, else none. Pure so the routing is
/// unit-testable without provider I/O.
pub(crate) fn claude_resolved_last_error(
    status: UsageSnapshotStatus,
    provider_error: Option<String>,
    cli_fallback: bool,
) -> Option<String> {
    match status {
        UsageSnapshotStatus::Stale => Some(provider_error.unwrap_or_else(|| {
            "Claude provider usage unavailable; cached quota is stale".to_owned()
        })),
        _ if cli_fallback => Some(provider_error.unwrap_or_else(|| {
            "Claude OAuth usage unavailable; showing reduced CLI snapshot".to_owned()
        })),
        _ => None,
    }
}
