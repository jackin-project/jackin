// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `Claude` / `Anthropic` usage snapshot.
//!
//! Carved out of `usage.rs` for the file-size ratchet. Items in this module
//! are `pub(crate)` so the coordinator (`usage.rs`) can re-export them.

use super::refresh::{
    ProviderError, ProviderFailureMetadata, ProviderRateLimit, split_provider_fetch,
};
#[cfg_attr(
    not(test),
    expect(clippy::wildcard_imports, reason = "target-dependent")
)]
use super::*;
use serde::{Deserialize, Deserializer, Serialize};
use std::any::Any;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use zeroize::Zeroizing;

const CLAUDE_KEYCHAIN_CREDENTIAL_ORIGIN: &str = "OAuth · macOS Keychain";
pub(crate) const MAX_CLAUDE_KEYCHAIN_PAYLOAD_BYTES: usize = 64 * 1024;

mod keychain;
mod lease;
#[cfg(any(target_os = "macos", test))]
pub use keychain::classify_claude_keychain_status;
pub use keychain::{
    ClaudeKeychainPolicyError, ClaudeKeychainRead, ClaudeUnattendedKeychainGuard,
    prepare_claude_keychain_auth, read_claude_keychain_item, unattended_keychain_guard,
};
pub(crate) use lease::bootstrapped_claude_service;
pub(crate) use lease::claude_credential_generation_is_current;
pub(crate) use lease::claude_service_is_bootstrapped;
pub use lease::{
    ClaudeCredentialBootstrapOutcome, ClaudeCredentialLease, bootstrap_claude_credential,
};

/// Per-foreground-generation admission gate and no-UI guard lifetime.
///
/// A timed-out provider task may outlive its broker worker while blocked in a
/// noninteractive Keychain reread. Its operation permit retains this scope
/// until the admitted operation finishes. Deactivation closes later admissions and
/// revokes the exact cached credential generation without joining that task.
pub(crate) struct ClaudeCollectorLiveness {
    state: Mutex<ClaudeCollectorState>,
    shutdown: Arc<AtomicBool>,
    _unattended_guard: Arc<dyn Any + Send + Sync>,
}

struct ClaudeCollectorState {
    active: bool,
    generation: Option<u64>,
}

/// A short-lived admission token for one operation in a foreground generation.
/// It keeps the no-UI guard alive without holding the lifecycle mutex over I/O.
/// Admission is the logical ordering point; an admitted call may run or finish
/// after deactivation and cannot be physically cancelled.
pub(crate) struct ClaudeCollectorOperationPermit {
    _liveness: Arc<ClaudeCollectorLiveness>,
    generation: u64,
}

impl ClaudeCollectorOperationPermit {
    fn generation(&self) -> u64 {
        self.generation
    }
}

impl ClaudeCollectorLiveness {
    pub(crate) fn new(guard: impl Any + Send + Sync + 'static) -> Self {
        Self {
            state: Mutex::new(ClaudeCollectorState {
                active: true,
                generation: None,
            }),
            shutdown: Arc::new(AtomicBool::new(false)),
            _unattended_guard: Arc::new(guard),
        }
    }

    pub(crate) fn bind_generation(&self, generation: u64) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert!(state.active && state.generation.is_none());
        state.generation = Some(generation);
    }

    pub(crate) fn shutdown_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.shutdown)
    }

    #[cfg(test)]
    pub(crate) fn is_current(&self) -> bool {
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.active && !self.shutdown.load(Ordering::Acquire)
    }

    pub(crate) fn is_current_if(&self, authorized: impl FnOnce(u64) -> bool) -> bool {
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.active
            && !self.shutdown.load(Ordering::Acquire)
            && state.generation.is_some_and(authorized)
    }

    /// Admit one operation while ordering its generation and consent checks
    /// against deactivation. The returned permit does not lock lifecycle state.
    pub(crate) fn admit_if(
        self: &Arc<Self>,
        authorized: impl FnOnce(u64) -> bool,
    ) -> Option<ClaudeCollectorOperationPermit> {
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !state.active || self.shutdown.load(Ordering::Acquire) {
            return None;
        }
        let generation = state.generation?;
        if !authorized(generation) {
            return None;
        }
        Some(ClaudeCollectorOperationPermit {
            _liveness: Arc::clone(self),
            generation,
        })
    }

    /// Close admissions and revoke this generation without waiting for any
    /// previously admitted provider or Keychain operation to finish.
    pub(crate) fn deactivate(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !state.active {
            return;
        }
        state.active = false;
        self.shutdown.store(true, Ordering::Release);
        if let Some(generation) = state.generation {
            lease::revoke_bootstrapped_claude_generation(generation);
        }
    }
}

/// Claude OAuth credential candidates, home-first — the single source of truth
/// for the path precedence, shared by `claude_snapshot` (token + identity) and
/// `claude_account_identity` (the shared-cache key) so the list can't drift.
pub(crate) fn claude_oauth_candidates(config: &Path) -> [PathBuf; 4] {
    [
        config.join(".credentials.json"),
        home_path(".claude/.credentials.json"),
        home_path(".claude.json"),
        PathBuf::from(CLAUDE_HANDOFF_CREDENTIALS_PATH),
    ]
}

/// Stable local source partition for one exact Claude Keychain service.
/// This value is independent of token and account metadata, so a credential
/// reread cannot move an opted-in broker row to a different account.
pub(crate) fn claude_source_capability_id_for_service(service: &str) -> String {
    let hashed = account_key_hash("claude-keychain-service-v1", service);
    hashed.strip_prefix("sha256:").unwrap_or(&hashed).to_owned()
}

/// Claude account identity (the `oauthAccount` email) from the same credential
/// candidates `claude_snapshot` uses, without fetching usage.
pub(crate) fn claude_account_identity() -> Option<String> {
    let config = env_dir_or_home("CLAUDE_CONFIG_DIR", ".claude");
    claude_oauth_candidates(&config).iter().find_map(|path| {
        load_claude_profile_payload(path).and_then(|profile| profile.account_email)
    })
}

pub(crate) fn claude_snapshot(agent: &str, provider: Option<&str>, now: i64) -> FocusedUsageView {
    claude_view_from_wave_with_rate_limit(agent, provider, now, resolve_claude_wave()).0
}

/// Claude API keys do not authenticate the OAuth quota endpoint. Keep this
/// route explicit and unsupported rather than feeding an API key into the
/// OAuth adapter and reporting a misleading login/error state.
pub(crate) fn claude_api_key_snapshot(
    agent: &str,
    provider: Option<&str>,
    key_name: &str,
    secret: &str,
    now: i64,
) -> FocusedUsageView {
    let has_secret = !secret.trim().is_empty();
    let status = if has_secret {
        UsageSnapshotStatus::Unsupported
    } else {
        UsageSnapshotStatus::NeedsSecret
    };
    let message = if has_secret {
        "Claude API-key quota is unavailable; OAuth usage requires CLAUDE_CODE_OAUTH_TOKEN"
    } else {
        "Claude API key is missing"
    };
    usage_view(UsageViewInput {
        agent,
        provider: provider.or(Some("Claude")),
        surface: UsageSurface::Claude,
        account_label: "Claude API key".to_owned(),
        username: None,
        plan_label: None,
        credential_origin: Some(format!("API key · env {key_name}")),
        buckets: vec![bucket(
            "Usage",
            None,
            None,
            None,
            None,
            Some(message),
            status,
        )],
        status,
        source: UsageSource::None,
        confidence: UsageConfidence::None,
        now,
        last_error: Some(message.to_owned()),
    })
}

/// Production Claude wave resolution: derive the Keychain scope from the
/// effective `CLAUDE_CONFIG_DIR`, then resolve Keychain-first with
/// scope-appropriate file/env fallback.
pub(crate) fn resolve_claude_wave() -> ClaudeWaveResolution {
    let config = env_dir_or_home("CLAUDE_CONFIG_DIR", ".claude");
    let home = home_path("");
    let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
    let Some(scope) = jackin_core::claude_keychain_scope(&config, &home, &current_dir) else {
        // Non-UTF-8 config path: the service is unknowable, so treat as absence.
        return ClaudeWaveResolution::Missing;
    };
    if let Some(selected_service) = bootstrapped_claude_service() {
        // A bootstrap pins every passive read in this process to one exact
        // source. Do not read files, environment, or another Keychain item on
        // a missing/mismatched lease; the normal resolver's fallback policy
        // is intentionally unavailable inside this scope.
        if selected_service != scope.service {
            return ClaudeWaveResolution::Missing;
        }
        return resolve_bootstrapped_claude_payload(&selected_service);
    }
    resolve_claude_refresh_wave_with(
        &scope,
        claude_keychain_state(),
        read_claude_keychain_item,
        || claude_scope_file_probe(&scope, &config),
        || read_claude_oauth_env_token(|name| std::env::var(name)),
    )
}

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
fn claude_scope_file_probe(
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
    let mut credential = None;
    let mut origin = None;
    let mut account_email = None;
    let mut organization_type = None;
    for path in candidates {
        let Some(profile) = load_claude_profile_payload(&path) else {
            continue;
        };
        if credential.is_none()
            && let Some(found) = profile.credential
        {
            credential = Some(found);
            origin = Some(oauth_origin(&path));
        }
        if account_email.is_none() {
            account_email = profile.account_email;
        }
        if organization_type.is_none() {
            organization_type = profile.organization_type;
        }
        if credential.is_some() && account_email.is_some() && organization_type.is_some() {
            break;
        }
    }
    ClaudeFileProbe {
        credential,
        origin,
        account_email,
        organization_type,
    }
}

fn resolve_bootstrapped_claude_payload(service: &str) -> ClaudeWaveResolution {
    let Some(json) = lease::cached_claude_keychain_payload(service) else {
        return ClaudeWaveResolution::Missing;
    };
    let Some(profile) = parse_claude_profile_payload(json.as_bytes()) else {
        return ClaudeWaveResolution::Missing;
    };
    let Some(credential) = profile.credential else {
        return ClaudeWaveResolution::Missing;
    };
    ClaudeWaveResolution::Resolved(Box::new(claude_resolved(
        credential,
        CLAUDE_KEYCHAIN_CREDENTIAL_ORIGIN.to_owned(),
        profile.account_email,
        profile.organization_type,
        Some(service.to_owned()),
    )))
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
    let (view, rate_limit, _) =
        claude_view_from_wave_with_metadata(agent, provider, now, resolution);
    (view, rate_limit)
}

pub(crate) fn claude_view_from_wave_with_metadata(
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

/// Terminal denial view: `NeedsLogin` with no bucket/account/plan/origin and the
/// exact non-secret error. Cached quota is never restored onto this (the typed
/// local-only policy blocks preservation in the refresh cache).
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

/// True when the OAuth usage fetch failed because the token lacks the quota
/// scope (an inference-only grant): only a typed HTTP 403. A 401, another
/// status, or any transport/decode failure is not scope restriction. Pure
/// so the inference-only state is unit-testable without provider I/O.
pub(crate) fn claude_error_is_scope_restriction(error: &ProviderError) -> bool {
    error.status() == Some(403)
}

/// Pick the provider error label for the OAuth provider response. A
/// scope-restricted failure normalizes to the explicit inference-only
/// message so the operator sees *why* quota is unavailable instead of a bare
/// HTTP status; every other error passes through verbatim.
pub(crate) fn claude_provider_error_label(oauth_error: Option<&ProviderError>) -> Option<String> {
    let error = oauth_error?;
    if oauth_error.is_some_and(claude_error_is_scope_restriction) {
        return Some(
            "Claude token lacks usage scope (inference-only); quota unavailable".to_owned(),
        );
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
    let view = usage_view(UsageViewInput {
        agent,
        provider,
        surface: UsageSurface::Claude,
        account_label: resolved.account_email.unwrap_or_default(),
        username: None,
        plan_label: resolved.organization_type.or(resolved.subscription_type),
        credential_origin: Some(resolved.credential_origin),
        buckets: Vec::new(),
        status: UsageSnapshotStatus::Unsupported,
        source: UsageSource::None,
        confidence: UsageConfidence::None,
        now,
        last_error: Some(
            "Claude OAuth collection requires an active experimental broker monitor".to_owned(),
        ),
    });
    (view, None, None)
}

/// Explicit broker-only Claude collector. The admission callback rechecks
/// persisted opt-in and exact current account mapping before every request and
/// reread; the current callback fences results after those operations return.
/// This function can only use the one exact foreground-bootstrap cache entry;
/// it never resolves files, environment values, or another Keychain service.
pub(crate) fn experimental_claude_usage_snapshot_for_service<C, A>(
    agent: &str,
    provider: Option<&str>,
    now: i64,
    service: &str,
    admit_operation: A,
    consent_is_current: C,
) -> Result<Option<ClaudeServiceUsageSnapshot>, ClaudeCollectionError>
where
    C: Fn() -> bool,
    A: Fn() -> Option<ClaudeCollectorOperationPermit>,
{
    if !consent_is_current() {
        return Err(ClaudeCollectionError::ConsentRevoked {
            provider_http_status: None,
        });
    }
    if !lease::valid_claude_keychain_service(service) || !claude_service_is_bootstrapped(service) {
        return Ok(None);
    }
    let Some(payload) = lease::cached_claude_keychain_payload(service) else {
        return Ok(None);
    };
    let Some(profile) = parse_claude_profile_payload(payload.as_bytes()) else {
        return Ok(None);
    };
    let Some(credential) = profile.credential else {
        return Ok(None);
    };
    let mut resolved = claude_resolved(
        credential,
        CLAUDE_KEYCHAIN_CREDENTIAL_ORIGIN.to_owned(),
        profile.account_email,
        profile.organization_type,
        Some(service.to_owned()),
    );
    let result = fetch_claude_with_one_401_reread_with_admission(
        service,
        &mut resolved,
        fetch_claude_oauth_usage,
        keychain::read_claude_keychain_item_uncached,
        admit_operation,
        consent_is_current,
    );
    let result = match result {
        Ok(response) => Ok(response),
        Err(ClaudeFetchError::Provider(error)) => Err(error),
        Err(ClaudeFetchError::ConsentRevoked {
            provider_http_status,
        }) => {
            return Err(ClaudeCollectionError::ConsentRevoked {
                provider_http_status,
            });
        }
    };
    let observed_at = if result.is_ok() {
        chrono::Utc::now().timestamp()
    } else {
        now
    };
    let (view, rate_limit, failure_metadata) =
        claude_result_view(agent, provider, observed_at, resolved, result);
    Ok(Some(ClaudeServiceUsageSnapshot {
        view,
        rate_limit,
        failure_metadata,
    }))
}

/// Secret-free collector gate outcome. Retain an HTTP status when revocation
/// races with recovery from an already observed unauthorized response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClaudeCollectionError {
    ConsentRevoked { provider_http_status: Option<u16> },
}

pub(crate) struct ClaudeServiceUsageSnapshot {
    pub(crate) view: FocusedUsageView,
    pub(crate) rate_limit: Option<ProviderRateLimit>,
    pub(crate) failure_metadata: Option<ProviderFailureMetadata>,
}

#[derive(Debug)]
enum ClaudeFetchError {
    Provider(ProviderHttpError),
    ConsentRevoked { provider_http_status: Option<u16> },
}

#[cfg(test)]
fn fetch_claude_with_one_401_reread<F, R>(
    service: &str,
    resolved: &mut ClaudeResolved,
    fetch: F,
    reread: R,
    consent_is_current: impl Fn() -> bool,
) -> Result<ClaudeOAuthUsageResponse, ClaudeFetchError>
where
    F: FnMut(&str) -> Result<ClaudeOAuthUsageResponse, ProviderHttpError>,
    R: FnOnce(&str) -> ClaudeKeychainRead,
{
    let liveness = Arc::new(ClaudeCollectorLiveness::new(()));
    if let Some(generation) = lease::claude_credential_generation(service) {
        liveness.bind_generation(generation);
    }
    let consent_is_current = Arc::new(consent_is_current);
    let consent_for_admission = Arc::clone(&consent_is_current);
    let liveness_for_admission = Arc::clone(&liveness);
    fetch_claude_with_one_401_reread_with_admission(
        service,
        resolved,
        fetch,
        reread,
        move || liveness_for_admission.admit_if(|_| consent_for_admission()),
        move || consent_is_current(),
    )
}

fn fetch_claude_with_one_401_reread_with_admission<F, R, A, C>(
    service: &str,
    resolved: &mut ClaudeResolved,
    mut fetch: F,
    reread: R,
    mut admit_operation: A,
    consent_is_current: C,
) -> Result<ClaudeOAuthUsageResponse, ClaudeFetchError>
where
    F: FnMut(&str) -> Result<ClaudeOAuthUsageResponse, ProviderHttpError>,
    R: FnOnce(&str) -> ClaudeKeychainRead,
    A: FnMut() -> Option<ClaudeCollectorOperationPermit>,
    C: Fn() -> bool,
{
    let first = {
        let Some(_permit) = admit_operation() else {
            return Err(ClaudeFetchError::ConsentRevoked {
                provider_http_status: None,
            });
        };
        fetch(resolved.access_token.as_str()).map_err(ClaudeFetchError::Provider)
    };
    if !matches!(
        &first,
        Err(ClaudeFetchError::Provider(ProviderHttpError::HttpStatus {
            status: 401,
            ..
        }))
    ) {
        if first.is_ok() && !consent_is_current() {
            return Err(ClaudeFetchError::ConsentRevoked {
                provider_http_status: None,
            });
        }
        return first;
    }
    let (generation, reread) = {
        let Some(permit) = admit_operation() else {
            return Err(ClaudeFetchError::ConsentRevoked {
                provider_http_status: Some(401),
            });
        };
        let generation = permit.generation();
        if !lease::begin_bootstrapped_claude_401_reread(service, generation) {
            return first;
        }
        (generation, reread(service))
    };
    if !consent_is_current() {
        return Err(ClaudeFetchError::ConsentRevoked {
            provider_http_status: Some(401),
        });
    }
    let ClaudeKeychainRead::Payload { json } = reread else {
        return first;
    };
    if json.len() > MAX_CLAUDE_KEYCHAIN_PAYLOAD_BYTES {
        return first;
    }
    let Some(profile) = parse_claude_profile_payload(json.as_bytes()) else {
        return first;
    };
    let Some(credential) = profile.credential else {
        return first;
    };
    // The service is the canonical local source scope. Preserve all account
    // metadata selected by discovery; reject a reread that supplies conflicting
    // explicit account evidence, and never remap from newly read email/tier.
    if resolved
        .account_email
        .as_deref()
        .zip(profile.account_email.as_deref())
        .is_some_and(|(old, new)| old != new)
        || credential.access_token.as_str() == resolved.access_token.as_str()
    {
        return first;
    }
    let Some(permit) = admit_operation() else {
        return Err(ClaudeFetchError::ConsentRevoked {
            provider_http_status: Some(401),
        });
    };
    if permit.generation() != generation {
        return Err(ClaudeFetchError::ConsentRevoked {
            provider_http_status: Some(401),
        });
    }
    resolved.access_token = credential.access_token;
    let retried = {
        let _permit = permit;
        fetch(resolved.access_token.as_str()).map_err(ClaudeFetchError::Provider)
    };
    if !consent_is_current() {
        return match retried {
            Err(provider_error) => Err(provider_error),
            Ok(_) => Err(ClaudeFetchError::ConsentRevoked {
                provider_http_status: None,
            }),
        };
    }
    if !lease::replace_bootstrapped_claude_payload(service, generation, json) {
        return match retried {
            Err(provider_error) => Err(provider_error),
            Ok(_) => Err(ClaudeFetchError::ConsentRevoked {
                provider_http_status: None,
            }),
        };
    }
    retried
}

fn claude_result_view(
    agent: &str,
    provider: Option<&str>,
    now: i64,
    resolved: ClaudeResolved,
    result: Result<ClaudeOAuthUsageResponse, ProviderHttpError>,
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

/// `last_error` for a resolved view: the normalized provider error when stale,
/// else none. Pure so the routing is unit-testable without provider I/O.
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

// No `Debug`/`Display`: this carries a live access token and (optionally) the
// stable refresh token, so it must never be formatted into a log or error.
pub(crate) struct ClaudeOAuthCredentials {
    pub(crate) access_token: Zeroizing<String>,
    pub(crate) subscription_type: Option<String>,
}

struct ClaudeSecretString(Zeroizing<String>);

impl<'de> Deserialize<'de> for ClaudeSecretString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer).map(|value| Self(Zeroizing::new(value)))
    }
}

#[derive(Deserialize)]
struct ClaudeCredentialPayload {
    #[serde(rename = "claudeAiOauth", alias = "claude_ai_oauth")]
    claude_ai_oauth: Option<ClaudeOAuthPayload>,
    #[serde(rename = "oauthAccount", alias = "oauth_account")]
    oauth_account: Option<ClaudeAccountPayload>,
}

#[derive(Deserialize)]
struct ClaudeOAuthPayload {
    #[serde(rename = "accessToken", alias = "access_token")]
    access_token: Option<ClaudeSecretString>,
    #[serde(rename = "subscriptionType", alias = "subscription_type")]
    subscription_type: Option<String>,
    #[serde(rename = "rateLimitTier", alias = "rate_limit_tier")]
    rate_limit_tier: Option<String>,
}

#[derive(Deserialize)]
struct ClaudeAccountPayload {
    #[serde(rename = "emailAddress", alias = "email_address")]
    email_address: Option<String>,
    #[serde(rename = "organizationType", alias = "organization_type")]
    organization_type: Option<String>,
}

pub(crate) struct ClaudeProfilePayload {
    pub(crate) credential: Option<ClaudeOAuthCredentials>,
    pub(crate) account_email: Option<String>,
    pub(crate) organization_type: Option<String>,
}

pub(crate) fn parse_claude_profile_payload(bytes: &[u8]) -> Option<ClaudeProfilePayload> {
    let payload = serde_json::from_slice::<ClaudeCredentialPayload>(bytes).ok()?;
    let account_email = payload
        .oauth_account
        .as_ref()
        .and_then(|account| account.email_address.as_deref())
        .map(str::trim)
        .filter(|email| !email.is_empty())
        .map(str::to_owned);
    let organization_type = payload
        .oauth_account
        .as_ref()
        .and_then(|account| account.organization_type.as_deref())
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .map(humanize_plan_label);
    let credential = payload.claude_ai_oauth.and_then(|oauth| {
        let access_token = oauth.access_token?.0;
        if access_token.trim().is_empty() {
            return None;
        }
        let subscription_type = oauth
            .subscription_type
            .or(oauth.rate_limit_tier)
            .as_deref()
            .map(humanize_plan_label);
        Some(ClaudeOAuthCredentials {
            access_token,
            subscription_type,
        })
    });
    Some(ClaudeProfilePayload {
        credential,
        account_email,
        organization_type,
    })
}

/// Fixed, value-free facts about a rejected Keychain payload. Every field name
/// and enum value is defined by this type; payload strings and unknown keys are
/// never copied into the diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClaudeCredentialPayloadDiagnostic {
    payload_bytes: usize,
    limit_bytes: usize,
    json: ClaudePayloadJsonState,
    root: ClaudePayloadFieldKind,
    oauth_container: ClaudePayloadAliasKinds,
    access_token: ClaudePayloadAccessTokenKinds,
    subscription_type: ClaudePayloadSubscriptionKinds,
    account_container: ClaudePayloadAliasKinds,
    email_address: ClaudePayloadAliasKinds,
    organization_type: ClaudePayloadAliasKinds,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ClaudePayloadJsonState {
    SkippedOversize,
    Invalid,
    Valid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ClaudePayloadFieldKind {
    Unavailable,
    Missing,
    Null,
    Object,
    String,
    Number,
    Boolean,
    Array,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ClaudePayloadAliasKinds {
    camel_case: ClaudePayloadFieldKind,
    snake_case: ClaudePayloadFieldKind,
    duplicate_alias: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ClaudePayloadAccessTokenKinds {
    camel_case: ClaudePayloadFieldKind,
    snake_case: ClaudePayloadFieldKind,
    camel_case_nonempty: Option<bool>,
    snake_case_nonempty: Option<bool>,
    duplicate_alias: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ClaudePayloadSubscriptionKinds {
    subscription_type: ClaudePayloadFieldKind,
    subscription_type_snake_case: ClaudePayloadFieldKind,
    rate_limit_tier: ClaudePayloadFieldKind,
    rate_limit_tier_snake_case: ClaudePayloadFieldKind,
    duplicate_alias: bool,
}

impl ClaudePayloadAliasKinds {
    fn unavailable() -> Self {
        Self {
            camel_case: ClaudePayloadFieldKind::Unavailable,
            snake_case: ClaudePayloadFieldKind::Unavailable,
            duplicate_alias: false,
        }
    }
}

impl ClaudePayloadAccessTokenKinds {
    fn unavailable() -> Self {
        Self {
            camel_case: ClaudePayloadFieldKind::Unavailable,
            snake_case: ClaudePayloadFieldKind::Unavailable,
            camel_case_nonempty: None,
            snake_case_nonempty: None,
            duplicate_alias: false,
        }
    }
}

impl ClaudePayloadSubscriptionKinds {
    fn unavailable() -> Self {
        Self {
            subscription_type: ClaudePayloadFieldKind::Unavailable,
            subscription_type_snake_case: ClaudePayloadFieldKind::Unavailable,
            rate_limit_tier: ClaudePayloadFieldKind::Unavailable,
            rate_limit_tier_snake_case: ClaudePayloadFieldKind::Unavailable,
            duplicate_alias: false,
        }
    }
}

#[derive(Default)]
struct ClaudePayloadRawField<'a>(Option<&'a serde_json::value::RawValue>);

impl<'de: 'a, 'a> Deserialize<'de> for ClaudePayloadRawField<'a> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        <&'de serde_json::value::RawValue>::deserialize(deserializer).map(|raw| Self(Some(raw)))
    }
}

#[derive(Default, Deserialize)]
struct ClaudePayloadRawRoot<'a> {
    #[serde(default, borrow, rename = "claudeAiOauth")]
    oauth_camel: ClaudePayloadRawField<'a>,
    #[serde(default, borrow, rename = "claude_ai_oauth")]
    oauth_snake: ClaudePayloadRawField<'a>,
    #[serde(default, borrow, rename = "oauthAccount")]
    account_camel: ClaudePayloadRawField<'a>,
    #[serde(default, borrow, rename = "oauth_account")]
    account_snake: ClaudePayloadRawField<'a>,
}

#[derive(Default, Deserialize)]
struct ClaudePayloadRawOAuth<'a> {
    #[serde(default, borrow, rename = "accessToken")]
    token_camel: ClaudePayloadRawField<'a>,
    #[serde(default, borrow, rename = "access_token")]
    token_snake: ClaudePayloadRawField<'a>,
    #[serde(default, borrow, rename = "subscriptionType")]
    subscription_camel: ClaudePayloadRawField<'a>,
    #[serde(default, borrow, rename = "subscription_type")]
    subscription_snake: ClaudePayloadRawField<'a>,
    #[serde(default, borrow, rename = "rateLimitTier")]
    rate_tier_camel: ClaudePayloadRawField<'a>,
    #[serde(default, borrow, rename = "rate_limit_tier")]
    rate_tier_snake: ClaudePayloadRawField<'a>,
}

#[derive(Default, Deserialize)]
struct ClaudePayloadRawAccount<'a> {
    #[serde(default, borrow, rename = "emailAddress")]
    email_camel: ClaudePayloadRawField<'a>,
    #[serde(default, borrow, rename = "email_address")]
    email_snake: ClaudePayloadRawField<'a>,
    #[serde(default, borrow, rename = "organizationType")]
    organization_camel: ClaudePayloadRawField<'a>,
    #[serde(default, borrow, rename = "organization_type")]
    organization_snake: ClaudePayloadRawField<'a>,
}

struct ClaudePayloadClassified {
    kind: ClaudePayloadFieldKind,
    nonempty: Option<bool>,
}

fn classify_raw(raw: &serde_json::value::RawValue, token: bool) -> ClaudePayloadClassified {
    let value = raw.get().trim_start();
    let kind = match value.as_bytes().first() {
        Some(b'n') => ClaudePayloadFieldKind::Null,
        Some(b'{') => ClaudePayloadFieldKind::Object,
        Some(b'"') => ClaudePayloadFieldKind::String,
        Some(b't' | b'f') => ClaudePayloadFieldKind::Boolean,
        Some(b'[') => ClaudePayloadFieldKind::Array,
        Some(b'-' | b'0'..=b'9') => ClaudePayloadFieldKind::Number,
        _ => ClaudePayloadFieldKind::Unavailable,
    };
    let nonempty = (token && kind == ClaudePayloadFieldKind::String)
        .then(|| {
            value
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
        })
        .flatten()
        .filter(|value| !value.contains('\\'))
        .map(|value| !value.trim().is_empty());
    ClaudePayloadClassified { kind, nonempty }
}

fn raw_field_kind(field: &ClaudePayloadRawField<'_>) -> ClaudePayloadFieldKind {
    field.0.map_or(ClaudePayloadFieldKind::Missing, |raw| {
        classify_raw(raw, false).kind
    })
}

fn raw_alias_kinds(
    camel: &ClaudePayloadRawField<'_>,
    snake: &ClaudePayloadRawField<'_>,
) -> ClaudePayloadAliasKinds {
    ClaudePayloadAliasKinds {
        camel_case: raw_field_kind(camel),
        snake_case: raw_field_kind(snake),
        duplicate_alias: camel.0.is_some() && snake.0.is_some(),
    }
}

fn raw_token_kinds(
    camel: &ClaudePayloadRawField<'_>,
    snake: &ClaudePayloadRawField<'_>,
) -> ClaudePayloadAccessTokenKinds {
    let camel_value = camel.0.map(|raw| classify_raw(raw, true));
    let snake_value = snake.0.map(|raw| classify_raw(raw, true));
    ClaudePayloadAccessTokenKinds {
        camel_case: camel_value
            .as_ref()
            .map_or(ClaudePayloadFieldKind::Missing, |value| value.kind),
        snake_case: snake_value
            .as_ref()
            .map_or(ClaudePayloadFieldKind::Missing, |value| value.kind),
        camel_case_nonempty: camel_value.and_then(|value| value.nonempty),
        snake_case_nonempty: snake_value.and_then(|value| value.nonempty),
        duplicate_alias: camel.0.is_some() && snake.0.is_some(),
    }
}

fn raw_subscription_kinds(oauth: &ClaudePayloadRawOAuth<'_>) -> ClaudePayloadSubscriptionKinds {
    let aliases = [
        &oauth.subscription_camel,
        &oauth.subscription_snake,
        &oauth.rate_tier_camel,
        &oauth.rate_tier_snake,
    ];
    let kinds = aliases.map(raw_field_kind);
    ClaudePayloadSubscriptionKinds {
        subscription_type: kinds[0],
        subscription_type_snake_case: kinds[1],
        rate_limit_tier: kinds[2],
        rate_limit_tier_snake_case: kinds[3],
        duplicate_alias: (oauth.subscription_camel.0.is_some()
            && oauth.subscription_snake.0.is_some())
            || (oauth.rate_tier_camel.0.is_some() && oauth.rate_tier_snake.0.is_some()),
    }
}

/// Describe a failed payload without exposing values. Check the size before
/// parsing so oversized Keychain data cannot cause diagnostic allocations.
pub fn diagnose_claude_profile_payload(bytes: &[u8]) -> ClaudeCredentialPayloadDiagnostic {
    let payload_bytes = bytes.len();
    let mut diagnostic = ClaudeCredentialPayloadDiagnostic {
        payload_bytes,
        limit_bytes: MAX_CLAUDE_KEYCHAIN_PAYLOAD_BYTES,
        json: ClaudePayloadJsonState::SkippedOversize,
        root: ClaudePayloadFieldKind::Unavailable,
        oauth_container: ClaudePayloadAliasKinds::unavailable(),
        access_token: ClaudePayloadAccessTokenKinds::unavailable(),
        subscription_type: ClaudePayloadSubscriptionKinds::unavailable(),
        account_container: ClaudePayloadAliasKinds::unavailable(),
        email_address: ClaudePayloadAliasKinds::unavailable(),
        organization_type: ClaudePayloadAliasKinds::unavailable(),
    };
    if payload_bytes > MAX_CLAUDE_KEYCHAIN_PAYLOAD_BYTES {
        return diagnostic;
    }

    let Ok(root_raw) = serde_json::from_slice::<&serde_json::value::RawValue>(bytes) else {
        diagnostic.json = ClaudePayloadJsonState::Invalid;
        return diagnostic;
    };
    diagnostic.json = ClaudePayloadJsonState::Valid;
    diagnostic.root = classify_raw(root_raw, false).kind;
    if diagnostic.root != ClaudePayloadFieldKind::Object {
        return diagnostic;
    }

    let Ok(root) = serde_json::from_str::<ClaudePayloadRawRoot<'_>>(root_raw.get()) else {
        return diagnostic;
    };
    diagnostic.oauth_container = raw_alias_kinds(&root.oauth_camel, &root.oauth_snake);
    diagnostic.account_container = raw_alias_kinds(&root.account_camel, &root.account_snake);

    let oauth_raw = root
        .oauth_camel
        .0
        .or(root.oauth_snake.0)
        .filter(|raw| classify_raw(raw, false).kind == ClaudePayloadFieldKind::Object);
    if let Some(oauth_raw) = oauth_raw
        && let Ok(oauth) = serde_json::from_str::<ClaudePayloadRawOAuth<'_>>(oauth_raw.get())
    {
        diagnostic.access_token = raw_token_kinds(&oauth.token_camel, &oauth.token_snake);
        diagnostic.subscription_type = raw_subscription_kinds(&oauth);
    }

    let account_raw = root
        .account_camel
        .0
        .or(root.account_snake.0)
        .filter(|raw| classify_raw(raw, false).kind == ClaudePayloadFieldKind::Object);
    if let Some(account_raw) = account_raw
        && let Ok(account) = serde_json::from_str::<ClaudePayloadRawAccount<'_>>(account_raw.get())
    {
        diagnostic.email_address = raw_alias_kinds(&account.email_camel, &account.email_snake);
        diagnostic.organization_type =
            raw_alias_kinds(&account.organization_camel, &account.organization_snake);
    }
    diagnostic
}

fn load_claude_profile_payload(path: &Path) -> Option<ClaudeProfilePayload> {
    let bytes = Zeroizing::new(fs::read(path).ok()?);
    parse_claude_profile_payload(bytes.as_slice())
}

pub(crate) fn load_claude_account_email(path: &Path) -> Option<String> {
    load_claude_profile_payload(path).and_then(|profile| profile.account_email)
}

#[cfg(test)]
pub(crate) fn load_claude_organization_type(path: &Path) -> Option<String> {
    load_claude_profile_payload(path).and_then(|profile| profile.organization_type)
}

#[cfg(test)]
pub(crate) fn claude_oauth_from_value(value: &serde_json::Value) -> Option<ClaudeOAuthCredentials> {
    let oauth = value.get("claudeAiOauth")?;
    let access_token = oauth
        .get("accessToken")
        .or_else(|| oauth.get("access_token"))
        .and_then(serde_json::Value::as_str)?
        .trim()
        .to_owned();
    if access_token.is_empty() {
        return None;
    }
    let subscription_type = oauth
        .get("subscriptionType")
        .or_else(|| oauth.get("subscription_type"))
        .or_else(|| oauth.get("rateLimitTier"))
        .or_else(|| oauth.get("rate_limit_tier"))
        .and_then(serde_json::Value::as_str)
        .map(humanize_plan_label);
    Some(ClaudeOAuthCredentials {
        access_token: Zeroizing::new(access_token),
        subscription_type,
    })
}

#[cfg(test)]
pub(crate) fn load_claude_oauth_credentials(path: &Path) -> Option<ClaudeOAuthCredentials> {
    claude_oauth_from_value(&read_json_file(path)?)
}

// ===================================================================
// macOS Keychain credential source (plan 002)
//
// Claude Code on macOS stores its OAuth credential only in the login
// Keychain (a fresh `/login` deletes the credentials file). The service
// name is derived from the effective `CLAUDE_CONFIG_DIR` by the shared
// `jackin_core::claude_keychain_scope` helper, so instance provisioning and
// this probe never disagree. Rust owns all resolution; Swift is display-only.
// ===================================================================

/// Process-lifetime Keychain coordination: serializes reader I/O so a consent
/// sheet is prompted at most once per wave, and remembers services the operator
/// explicitly denied so a denial is terminal for that service for the process
/// (no retry-prompt storm). A *missing* item is never cached, so a later
/// `claude /login` is picked up without an app restart (flow W5).
#[derive(Default)]
pub(crate) struct ClaudeKeychainState {
    inner: Mutex<ClaudeKeychainInner>,
}

#[derive(Default)]
struct ClaudeKeychainInner {
    denied_services: std::collections::HashSet<String>,
    /// Count of reader invocations — a test seam proving reads are shared and
    /// each service is queried at most once per wave.
    reads: u64,
}

impl ClaudeKeychainState {
    /// Resolve one Keychain read for `service` through `reader`, honoring the
    /// process-terminal denial cache and serializing reader I/O.
    pub(crate) fn read_with<F>(&self, service: &str, reader: F) -> ClaudeKeychainRead
    where
        F: FnOnce(&str) -> ClaudeKeychainRead,
    {
        {
            let inner = self
                .inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if inner.denied_services.contains(service) {
                return ClaudeKeychainRead::Denied;
            }
        }
        // Reader runs while holding the serialization lock so concurrent waves
        // cannot open two consent sheets for the same service at once.
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if inner.denied_services.contains(service) {
            return ClaudeKeychainRead::Denied;
        }
        inner.reads += 1;
        let read = reader(service);
        if matches!(read, ClaudeKeychainRead::Denied) {
            inner.denied_services.insert(service.to_owned());
        }
        read
    }

    #[cfg(test)]
    pub(crate) fn read_count(&self) -> u64 {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .reads
    }
}

/// Production global Keychain state (one per process).
pub(crate) fn claude_keychain_state() -> &'static ClaudeKeychainState {
    static STATE: std::sync::OnceLock<ClaudeKeychainState> = std::sync::OnceLock::new();
    STATE.get_or_init(ClaudeKeychainState::default)
}

/// Result of resolving the Claude credential for one refresh wave. Secret-safe:
/// never `Debug`/`Display`. The access token rides in `Resolved` for the fetch;
/// the opaque `discriminator` is the only identity carried into coordination.
pub(crate) enum ClaudeWaveResolution {
    Resolved(Box<ClaudeResolved>),
    /// Operator denied the Keychain consent for this service — terminal,
    /// local-only. No file/env read, no cached-quota restoration.
    Denied,
    /// No usable credential from Keychain or fallback. Local-only needs-login.
    Missing,
}

#[derive(Clone)]
pub(crate) struct ClaudeResolved {
    pub(crate) access_token: Zeroizing<String>,
    pub(crate) subscription_type: Option<String>,
    pub(crate) account_email: Option<String>,
    pub(crate) organization_type: Option<String>,
    pub(crate) credential_origin: String,
    /// Stable exact Keychain source service, when this is a profile credential.
    pub(crate) keychain_service: Option<String>,
    /// `true` when no local source identity can be attached.
    pub(crate) is_anonymous: bool,
}

/// One credential candidate probe result: the parsed OAuth credential (if any)
/// plus same-scope account/tier metadata.
pub(crate) struct ClaudeFileProbe {
    pub(crate) credential: Option<ClaudeOAuthCredentials>,
    pub(crate) origin: Option<String>,
    pub(crate) account_email: Option<String>,
    pub(crate) organization_type: Option<String>,
}

/// Resolve the Claude wave for `scope`: Keychain first, then scope-appropriate
/// file/env fallback. `keychain_reader` performs the real (or test) Keychain
/// read; `file_probe` returns the scope's file credential + metadata in one
/// call; `env_reader` yields an OAuth env token. No process-global env
/// mutation — all inputs are injected so the whole path is unit-testable.
pub(crate) fn resolve_claude_refresh_wave_with<K, P, E>(
    scope: &jackin_core::ClaudeKeychainScope,
    state: &ClaudeKeychainState,
    keychain_reader: K,
    file_probe: P,
    env_reader: E,
) -> ClaudeWaveResolution
where
    K: FnOnce(&str) -> ClaudeKeychainRead,
    P: FnOnce() -> ClaudeFileProbe,
    E: FnOnce() -> Option<ClaudeOAuthEnvToken>,
{
    match state.read_with(&scope.service, keychain_reader) {
        ClaudeKeychainRead::Denied => ClaudeWaveResolution::Denied,
        ClaudeKeychainRead::Payload { json } => {
            let Some(ClaudeProfilePayload {
                credential: Some(credential),
                account_email,
                organization_type,
            }) = parse_claude_profile_payload(json.as_bytes())
            else {
                return resolve_claude_fallback(scope, file_probe(), env_reader());
            };
            // Valid Keychain payload: may still collect account/tier metadata
            // from the same-scope file probe, but the file credential can
            // never replace the Keychain one.
            let probe = file_probe();
            let origin = format!("OAuth · macOS Keychain ({})", scope.service);
            ClaudeWaveResolution::Resolved(Box::new(claude_resolved(
                credential,
                origin,
                account_email.or(probe.account_email),
                organization_type.or(probe.organization_type),
                Some(scope.service.clone()),
            )))
        }
        ClaudeKeychainRead::Missing | ClaudeKeychainRead::ConsentRequired => {
            resolve_claude_fallback(scope, file_probe(), env_reader())
        }
    }
}

fn resolve_claude_fallback(
    scope: &jackin_core::ClaudeKeychainScope,
    probe: ClaudeFileProbe,
    env_token: Option<ClaudeOAuthEnvToken>,
) -> ClaudeWaveResolution {
    if let Some(credential) = probe.credential {
        let origin = probe
            .origin
            .unwrap_or_else(|| "OAuth · credentials file".to_owned());
        return ClaudeWaveResolution::Resolved(Box::new(claude_resolved(
            credential,
            origin,
            probe.account_email,
            probe.organization_type,
            Some(scope.service.clone()),
        )));
    }
    if let Some(token) = env_token {
        return ClaudeWaveResolution::Resolved(Box::new(ClaudeResolved {
            access_token: token.0,
            subscription_type: None,
            account_email: probe.account_email,
            organization_type: probe.organization_type,
            credential_origin: format!(
                "OAuth · env {}",
                jackin_core::CLAUDE_CODE_OAUTH_TOKEN_ENV_NAME
            ),
            keychain_service: None,
            is_anonymous: true,
        }));
    }
    ClaudeWaveResolution::Missing
}

pub(crate) struct ClaudeOAuthEnvToken(Zeroizing<String>);

impl PartialEq for ClaudeOAuthEnvToken {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_str() == other.0.as_str()
    }
}

impl Eq for ClaudeOAuthEnvToken {}

impl std::fmt::Debug for ClaudeOAuthEnvToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ClaudeOAuthEnvToken(REDACTED)")
    }
}

impl ClaudeOAuthEnvToken {
    pub(crate) fn new(value: String) -> Self {
        Self(Zeroizing::new(value))
    }
}

fn claude_resolved(
    credential: ClaudeOAuthCredentials,
    origin: String,
    account_email: Option<String>,
    organization_type: Option<String>,
    keychain_service: Option<String>,
) -> ClaudeResolved {
    // Canonical identity is the local exact source scope, never an email or a
    // rotating token. The caller that owns a profile binding supplies that
    // scope independently when it materializes the account row.
    let is_anonymous = account_email.is_none() && keychain_service.is_none();
    ClaudeResolved {
        access_token: credential.access_token,
        subscription_type: credential.subscription_type,
        account_email,
        organization_type,
        credential_origin: origin,
        keychain_service,
        is_anonymous,
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct ClaudeOAuthUsageResponse {
    #[serde(rename = "five_hour")]
    pub(crate) five_hour: Option<ClaudeOAuthUsageWindow>,
    // `seven_day` is the Weekly window. `seven_day_oauth_apps` is a SEPARATE
    // window the API also returns — it must NOT be aliased here (the API sends
    // both keys, so aliasing collides into a serde "duplicate field" and fails
    // the whole decode). It is not a CodexBar quota window, so it is ignored.
    #[serde(rename = "seven_day")]
    pub(crate) seven_day: Option<ClaudeOAuthUsageWindow>,
    #[serde(rename = "seven_day_sonnet")]
    pub(crate) seven_day_sonnet: Option<ClaudeOAuthUsageWindow>,
    #[serde(rename = "seven_day_opus")]
    pub(crate) seven_day_opus: Option<ClaudeOAuthUsageWindow>,
    #[serde(alias = "seven_day_claude_routines")]
    #[serde(alias = "claude_routines")]
    #[serde(alias = "routines")]
    #[serde(alias = "seven_day_cowork")]
    #[serde(rename = "seven_day_routines")]
    pub(crate) seven_day_routines: Option<ClaudeOAuthUsageWindow>,
    // Authoritative shape for Session / "All models" Weekly / per-model Weekly
    // (Fable, and future model-scoped limits). The API migrated model-specific
    // windows here: the legacy `seven_day_sonnet`/`seven_day_opus` keys are
    // still returned but `null` on current accounts — the data lives only in
    // `limits` as `weekly_scoped` entries. Surfaced generically so a new model
    // codename (Fable today, others tomorrow) appears without per-model code.
    #[serde(default)]
    pub(crate) limits: Vec<ClaudeOAuthLimit>,
    #[serde(rename = "extra_usage")]
    pub(crate) extra_usage: Option<ClaudeOAuthExtraUsage>,
    // The newer, self-describing money object. Preferred over `extra_usage`
    // because it states the unit scale (`exponent`) and currency explicitly, so
    // a minor-unit amount can never be mis-scaled. `extra_usage` is kept as a
    // fallback for responses that predate `spend`.
    #[serde(rename = "spend")]
    pub(crate) spend: Option<ClaudeOAuthSpend>,
    // Catch-all for the remaining keys — chiefly the rotating-codename dollar
    // budget windows (`amber_ladder`, `omelette_promotional`, …). Capturing
    // them generically, rather than enumerating each ephemeral name, is what
    // lets enterprise dollar budgets surface instead of being silently dropped
    // by a fixed-field struct.
    #[serde(flatten)]
    pub(crate) other_windows: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ClaudeOAuthSpend {
    pub(crate) used: Option<ClaudeOAuthMoney>,
    pub(crate) limit: Option<ClaudeOAuthMoney>,
    pub(crate) percent: Option<u8>,
    pub(crate) severity: Option<String>,
    pub(crate) enabled: Option<bool>,
    #[serde(rename = "disabled_reason")]
    pub(crate) disabled_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ClaudeOAuthMoney {
    #[serde(rename = "amount_minor")]
    pub(crate) amount_minor: Option<i64>,
    pub(crate) currency: Option<String>,
    pub(crate) exponent: Option<u8>,
}

impl ClaudeOAuthMoney {
    pub(crate) fn into_money(self) -> Option<Money> {
        Some(Money::new(
            self.amount_minor?,
            self.currency.unwrap_or_else(|| "credits".to_owned()),
            self.exponent.unwrap_or(2),
        ))
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct ClaudeOAuthUsageWindow {
    pub(crate) utilization: Option<f64>,
    #[serde(rename = "resets_at")]
    pub(crate) resets_at: Option<String>,
    // Dollar-denominated budget windows (enterprise contractual allocations,
    // carried under rotating codename keys like `amber_ladder`). Named in
    // major-unit dollars by the API, so no `exponent` is supplied.
    #[serde(rename = "limit_dollars")]
    pub(crate) limit_dollars: Option<f64>,
    #[serde(rename = "used_dollars")]
    pub(crate) used_dollars: Option<f64>,
}

/// One entry in the `limits` array — the authoritative shape for Session,
/// "All models" Weekly, and per-model Weekly (Fable, and future model-scoped
/// limits). `percent` is already-scaled (0..=100); `kind` selects the bucket
/// (`session` | `weekly_all` | `weekly_scoped`); `scope.model.display_name`
/// labels a `weekly_scoped` window; `severity` mirrors the web console's meter
/// color and maps to [`UsageSeverity`]. The API also sends `group`, `is_active`,
/// `scope.surface`, and `model.id`, but those carry no rendering meaning today,
/// so they are intentionally not modeled — serde ignores unknown fields, and a
/// field is added back here only when something reads it (no dead fields).
#[derive(Debug, Deserialize)]
pub(crate) struct ClaudeOAuthLimit {
    pub(crate) kind: Option<String>,
    pub(crate) percent: Option<serde_json::Value>,
    pub(crate) severity: Option<String>,
    #[serde(rename = "resets_at")]
    pub(crate) resets_at: Option<String>,
    pub(crate) scope: Option<ClaudeOAuthLimitScope>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ClaudeOAuthLimitScope {
    pub(crate) model: Option<ClaudeOAuthLimitModel>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ClaudeOAuthLimitModel {
    #[serde(rename = "display_name")]
    pub(crate) display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ClaudeOAuthExtraUsage {
    #[serde(rename = "is_enabled")]
    pub(crate) is_enabled: Option<bool>,
    #[serde(rename = "monthly_limit")]
    pub(crate) monthly_limit: Option<f64>,
    #[serde(rename = "used_credits")]
    pub(crate) used_credits: Option<f64>,
    pub(crate) utilization: Option<f64>,
    pub(crate) currency: Option<String>,
    // Unit scale for `used_credits`/`monthly_limit`: they are MINOR units
    // (e.g. cents), so the major value is `value / 10^decimal_places`. Ignoring
    // this is what produced the 100×-too-large spend display.
    #[serde(rename = "decimal_places")]
    pub(crate) decimal_places: Option<u8>,
    #[serde(rename = "disabled_reason")]
    pub(crate) disabled_reason: Option<String>,
}

/// Session (5-hour) window duration, shared by every source that produces one.
const CLAUDE_SESSION_WINDOW_SECONDS: i64 = 5 * 60 * 60;
/// Weekly window duration, shared by every source (`weekly_all`,
/// `weekly_scoped`, legacy `seven_day*`).
const CLAUDE_WEEKLY_WINDOW_SECONDS: i64 = 7 * 24 * 60 * 60;

/// One normalized Claude API quota window — the single intermediate shape
/// every supported API utilization source feeds before it becomes a
/// [`QuotaBucketView`]. The authoritative `limits` array and legacy named
/// windows (`seven_day*`) share one builder. Fable is not a special case here —
/// it is just another `weekly_scoped` entry.
#[derive(Debug, Clone)]
pub(crate) struct ClaudeQuotaWindow {
    pub(crate) label: String,
    pub(crate) slot: Option<StatusSlot>,
    /// Used fraction on the scale the shared helpers expect: a raw
    /// `utilization` (fraction-or-percent) for legacy fields, or
    /// `f64::from(percent)` for `limits`. `used_percent_label` and
    /// `remaining_from_fraction` resolve the fraction-vs-percent ambiguity.
    pub(crate) used: Option<f64>,
    pub(crate) reset_at: Option<i64>,
    pub(crate) window_seconds: Option<i64>,
    pub(crate) severity: UsageSeverity,
}

impl ClaudeQuotaWindow {
    /// The one bucket builder for every Claude utilization source. The used
    /// label is uncapped (a window over its limit renders `150% used` while
    /// `remaining` clamps at 0); pace is computed only when both a reset and a
    /// window duration are known; severity mirrors the API for meter color.
    pub(crate) fn into_bucket(self, now: i64) -> QuotaBucketView {
        let remaining = self.used.and_then(remaining_from_fraction);
        let pace = quota_pace_label(remaining, self.reset_at, self.window_seconds, now);
        let mut view = timed_bucket(
            &self.label,
            self.used.and_then(used_percent_label),
            Some("100%".to_owned()),
            remaining,
            self.reset_at,
            now,
            pace.as_deref(),
            UsageSnapshotStatus::Fresh,
        );
        view.status_slot = self.slot;
        view.severity = self.severity;
        view
    }
}

impl ClaudeOAuthUsageWindow {
    /// Normalize a legacy named window (`five_hour`, `seven_day*`) into the
    /// unified quota model. `slot` and `window_seconds` carry the semantic the
    /// fixed field name can't (Session/Weekly headline + duration for pace), so
    /// a legacy weekly Sonnet window is paced the same way as a `weekly_scoped`
    /// Fable limit — uniform handling across API generations.
    fn into_quota(
        self,
        label: &str,
        slot: Option<StatusSlot>,
        window_seconds: Option<i64>,
    ) -> ClaudeQuotaWindow {
        ClaudeQuotaWindow {
            label: label.to_owned(),
            slot,
            used: self.utilization,
            reset_at: self.resets_at.as_deref().and_then(parse_iso_epoch),
            window_seconds,
            // Legacy named windows carry no severity field; the API meter
            // color only arrived with `limits`.
            severity: UsageSeverity::Normal,
        }
    }
}

impl ClaudeOAuthLimit {
    /// Normalize a `limits`-array entry into the unified quota model. Returns
    /// `None` for an entry without a usable shape: a missing `percent`, an
    /// unknown `kind`, or a `weekly_scoped` window whose model has no display
    /// name (omitted, never fabricated into an empty-label row). The API's
    /// `is_active` flag is deliberately NOT a render gate — live responses
    /// send `false` on headline limits that still carry quota.
    fn as_quota(&self) -> Option<ClaudeQuotaWindow> {
        let percent = json_number(self.percent.as_ref()?)?;
        let (label, slot, window_seconds) = match self.kind.as_deref()? {
            "session" => (
                "Session".to_owned(),
                Some(StatusSlot::Session),
                Some(CLAUDE_SESSION_WINDOW_SECONDS),
            ),
            "weekly_all" => (
                "All models".to_owned(),
                Some(StatusSlot::Weekly),
                Some(CLAUDE_WEEKLY_WINDOW_SECONDS),
            ),
            "weekly_scoped" => (
                self.scoped_label()?,
                None,
                Some(CLAUDE_WEEKLY_WINDOW_SECONDS),
            ),
            _ => return None,
        };
        Some(ClaudeQuotaWindow {
            label,
            slot,
            used: Some(percent),
            reset_at: self.resets_at.as_deref().and_then(parse_iso_epoch),
            window_seconds,
            severity: severity_from_label(self.severity.as_deref()),
        })
    }

    /// The model display name for a `weekly_scoped` limit, trimmed and
    /// non-empty; `None` when the API supplied no name.
    fn scoped_label(&self) -> Option<String> {
        self.scope
            .as_ref()
            .and_then(|scope| scope.model.as_ref())
            .and_then(|model| model.display_name.as_deref())
            .map(str::trim)
            .filter(|label| !label.is_empty())
            .map(str::to_owned)
    }
}

impl ClaudeOAuthUsageResponse {
    pub(crate) fn into_buckets(self, now: i64) -> Vec<QuotaBucketView> {
        // Destructure so the spend/dollar data is moved out before the
        // utilization windows consume the rest — one source of truth, one
        // builder, regardless of whether the windows came from `limits` or the
        // legacy named keys.
        let Self {
            five_hour,
            seven_day,
            seven_day_sonnet,
            seven_day_opus,
            seven_day_routines,
            limits,
            extra_usage,
            spend,
            other_windows,
        } = self;
        // The `limits` array is preferred on current accounts, but it can be
        // partial while the legacy named fields still carry usable windows.
        // Build both into the same model and backfill only semantic gaps so an
        // unknown or unnamed `limits` entry cannot erase valid legacy quotas.
        let mut windows: Vec<ClaudeQuotaWindow> = limits
            .iter()
            .filter_map(ClaudeOAuthLimit::as_quota)
            .collect();
        for window in legacy_claude_quota_windows(
            five_hour,
            seven_day,
            seven_day_sonnet,
            seven_day_opus,
            seven_day_routines,
        ) {
            if !has_equivalent_claude_window(&windows, &window) {
                windows.push(window);
            }
        }
        let mut buckets: Vec<QuotaBucketView> =
            windows.into_iter().map(|w| w.into_bucket(now)).collect();
        if let Some(spend) = claude_spend_bucket(spend, extra_usage) {
            buckets.push(spend);
        }
        push_claude_dollar_windows(&mut buckets, other_windows, now);
        buckets
    }
}

fn has_equivalent_claude_window(
    windows: &[ClaudeQuotaWindow],
    candidate: &ClaudeQuotaWindow,
) -> bool {
    match candidate.slot {
        Some(StatusSlot::Session) => windows
            .iter()
            .any(|window| window.slot == Some(StatusSlot::Session)),
        Some(StatusSlot::Weekly) => windows
            .iter()
            .any(|window| window.slot == Some(StatusSlot::Weekly)),
        _ => windows.iter().any(|window| {
            window.slot.is_none() && window.label.eq_ignore_ascii_case(&candidate.label)
        }),
    }
}

/// Legacy pre-`limits` named windows normalized to the unified quota model, so
/// they share one builder with `limits`-sourced windows. Weekly-scoped windows
/// (Sonnet/Opus/Routines) get the weekly duration so they are paced uniformly
/// with a `weekly_scoped` Fable limit.
fn legacy_claude_quota_windows(
    five_hour: Option<ClaudeOAuthUsageWindow>,
    seven_day: Option<ClaudeOAuthUsageWindow>,
    seven_day_sonnet: Option<ClaudeOAuthUsageWindow>,
    seven_day_opus: Option<ClaudeOAuthUsageWindow>,
    seven_day_routines: Option<ClaudeOAuthUsageWindow>,
) -> Vec<ClaudeQuotaWindow> {
    let session = Some(CLAUDE_SESSION_WINDOW_SECONDS);
    let weekly = Some(CLAUDE_WEEKLY_WINDOW_SECONDS);
    let mut windows = Vec::new();
    if let Some(window) = five_hour {
        windows.push(window.into_quota("Session", Some(StatusSlot::Session), session));
    }
    if let Some(window) = seven_day {
        windows.push(window.into_quota("Weekly", Some(StatusSlot::Weekly), weekly));
    }
    if let Some(window) = seven_day_sonnet {
        windows.push(window.into_quota("Sonnet", None, weekly));
    }
    if let Some(window) = seven_day_opus {
        windows.push(window.into_quota("Opus", None, weekly));
    }
    if let Some(window) = seven_day_routines {
        windows.push(window.into_quota("Daily Routines", None, weekly));
    }
    windows
}

/// Surface rotating-codename dollar-budget windows (`amber_ladder` etc.) that a
/// fixed-field struct would drop. Each captured key is parsed as a window; only
/// those carrying a positive `limit_dollars` are real allocations and become a
/// (non-headline) dollar bucket labelled by the title-cased codename (the API
/// supplies no human name for these windows).
pub(crate) fn push_claude_dollar_windows(
    buckets: &mut Vec<QuotaBucketView>,
    other: BTreeMap<String, serde_json::Value>,
    now: i64,
) {
    for (key, value) in other {
        let Ok(window) = serde_json::from_value::<ClaudeOAuthUsageWindow>(value) else {
            continue;
        };
        let Some(limit) = window.limit_dollars.filter(|limit| *limit > 0.0) else {
            continue;
        };
        // `*_dollars` are major-unit dollars; scale to minor for Money.
        let used = window.used_dollars.unwrap_or(0.0).max(0.0);
        let used_money = Money::new((used * 100.0).round() as i64, "USD", 2);
        let limit_money = Money::new((limit * 100.0).round() as i64, "USD", 2);
        // `limit > 0.0` holds (filtered above), so the fraction is well-defined.
        #[expect(
            clippy::cast_sign_loss,
            reason = "fraction clamped to 0.0..=1.0; percent is rounded f64→u8"
        )]
        let remaining_percent =
            Some(((1.0 - (used / limit).clamp(0.0, 1.0)) * 100.0).round() as u8);
        let reset_at = window.resets_at.as_deref().and_then(parse_iso_epoch);
        let mut view = timed_bucket(
            &humanize_window_label(&key),
            Some(format!("{used_money} spent")),
            Some(limit_money.to_string()),
            remaining_percent,
            reset_at,
            now,
            remaining_percent
                .map(|remaining| format!("{}% used", 100u8.saturating_sub(remaining)))
                .as_deref(),
            UsageSnapshotStatus::Fresh,
        );
        view.used_money = Some(used_money);
        view.limit_money = Some(limit_money);
        buckets.push(view);
    }
}

/// The normalized inputs for the monetary "Extra usage" bucket, derived from
/// whichever source the API provided.
pub(crate) struct ClaudeSpend {
    pub(crate) used: Money,
    pub(crate) limit: Option<Money>,
    /// Percent of the cap already spent (0..=100).
    pub(crate) used_percent: Option<u8>,
    pub(crate) enabled: bool,
    pub(crate) disabled_reason: Option<String>,
    pub(crate) severity: UsageSeverity,
}

/// Build the monetary spend bucket from the API response.
///
/// Prefers the self-describing `spend{}` object (it carries `amount_minor` +
/// `exponent`, so the scale is unambiguous); falls back to `extra_usage`,
/// scaling `used_credits`/`monthly_limit` by `decimal_places`. Both paths feed
/// one [`Money`]-typed builder, so spend can never be rendered 100× too large
/// regardless of source. A disabled (e.g. out-of-credits) bucket is still
/// surfaced — with its reason — rather than silently dropped, so the cap stays
/// visible the way the web console shows it.
pub(crate) fn claude_spend_bucket(
    spend: Option<ClaudeOAuthSpend>,
    extra: Option<ClaudeOAuthExtraUsage>,
) -> Option<QuotaBucketView> {
    let spend = normalize_claude_spend(spend, extra)?;
    let remaining_percent = spend.used_percent.map(|used| 100u8.saturating_sub(used));
    let used_label = Some(format!("{} spent", spend.used));
    let limit_label = spend.limit.as_ref().map(Money::to_string);
    let pace = if spend.enabled {
        spend.used_percent.map(|used| format!("{used}% used"))
    } else {
        Some(match &spend.disabled_reason {
            Some(reason) => format!("disabled · {}", humanize_reason(reason)),
            None => "disabled".to_owned(),
        })
    };
    let mut view = bucket(
        "Extra usage",
        used_label,
        limit_label,
        remaining_percent,
        None,
        pace.as_deref(),
        UsageSnapshotStatus::Fresh,
    );
    view.status_slot = Some(StatusSlot::Spend);
    view.severity = spend.severity;
    view.used_money = Some(spend.used);
    view.limit_money = spend.limit;
    Some(view)
}

pub(crate) fn normalize_claude_spend(
    spend: Option<ClaudeOAuthSpend>,
    extra: Option<ClaudeOAuthExtraUsage>,
) -> Option<ClaudeSpend> {
    if let Some(spend) = spend
        && let Some(used) = spend.used.and_then(ClaudeOAuthMoney::into_money)
    {
        return Some(ClaudeSpend {
            used,
            limit: spend.limit.and_then(ClaudeOAuthMoney::into_money),
            used_percent: spend.percent.map(|percent| percent.min(100)),
            enabled: spend.enabled.unwrap_or(true),
            disabled_reason: spend.disabled_reason,
            severity: severity_from_label(spend.severity.as_deref()),
        });
    }
    let extra = extra?;
    let used_credits = extra.used_credits?;
    let exponent = extra.decimal_places.unwrap_or(2);
    let currency = extra.currency.unwrap_or_else(|| "credits".to_owned());
    let used = Money::new(used_credits.round() as i64, &currency, exponent);
    let limit = extra
        .monthly_limit
        .map(|limit| Money::new(limit.round() as i64, &currency, exponent));
    Some(ClaudeSpend {
        used,
        limit,
        used_percent: extra.utilization.and_then(used_percent_from_fraction),
        enabled: extra.is_enabled.unwrap_or(true),
        disabled_reason: extra.disabled_reason,
        severity: UsageSeverity::Normal,
    })
}

pub(crate) fn fetch_claude_oauth_usage(
    access_token: &str,
) -> Result<ClaudeOAuthUsageResponse, ProviderHttpError> {
    let user_agent = format!("jackin/{}", env!("CARGO_PKG_VERSION"));
    get_json_bearer(
        jackin_telemetry::schema::enums::ProviderName::Anthropic,
        "/api/oauth/usage",
        "Claude OAuth usage",
        "https://api.anthropic.com/api/oauth/usage",
        access_token,
        &[
            (reqwest::header::CONTENT_TYPE, "application/json"),
            (
                reqwest::header::HeaderName::from_static("anthropic-beta"),
                "oauth-2025-04-20",
            ),
            // Report the real collecting client. This experimental endpoint
            // must not impersonate Claude Code or launch it for an identity.
            (reqwest::header::USER_AGENT, &user_agent),
        ],
    )
}

#[cfg(test)]
mod auth_diagnostic_tests;
#[cfg(test)]
mod tests;
