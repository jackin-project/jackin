// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `Claude` / `Anthropic` usage snapshot.
//!
//! Carved out of `usage.rs` for the file-size ratchet. Items in this module
//! are `pub(crate)` so the coordinator (`usage.rs`) can re-export them.

#[cfg_attr(
    not(test),
    expect(clippy::wildcard_imports, reason = "target-dependent")
)]
use super::*;
use serde::Deserialize;

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

/// Claude account identity (the `oauthAccount` email) from the same credential
/// candidates `claude_snapshot` uses, without fetching usage.
pub(crate) fn claude_account_identity() -> Option<String> {
    let config = env_dir_or_home("CLAUDE_CONFIG_DIR", ".claude");
    claude_oauth_candidates(&config)
        .iter()
        .find_map(|path| load_claude_account_email(path))
}

pub(crate) fn claude_snapshot(agent: &str, provider: Option<&str>, now: i64) -> FocusedUsageView {
    claude_view_from_wave(agent, provider, now, resolve_claude_wave())
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
    resolve_claude_refresh_wave_with(
        &scope,
        claude_keychain_state(),
        read_claude_keychain_item,
        || claude_scope_file_probe(&scope, &config),
        || {
            std::env::var("ANTHROPIC_API_KEY")
                .ok()
                .filter(|value| !value.is_empty())
                .or_else(|| {
                    std::env::var("ANTHROPIC_AUTH_TOKEN")
                        .ok()
                        .filter(|value| !value.is_empty())
                })
        },
    )
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

/// Build the Claude view for a resolved wave.
pub(crate) fn claude_view_from_wave(
    agent: &str,
    provider: Option<&str>,
    now: i64,
    resolution: ClaudeWaveResolution,
) -> FocusedUsageView {
    match resolution {
        ClaudeWaveResolution::Denied => claude_denied_view(agent, provider, now),
        ClaudeWaveResolution::Missing => claude_missing_view(agent, provider, now),
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

fn claude_resolved_view(
    agent: &str,
    provider: Option<&str>,
    now: i64,
    resolved: ClaudeResolved,
) -> FocusedUsageView {
    let (oauth_quota, oauth_error) =
        split_fetch(Some(fetch_claude_oauth_usage(&resolved.access_token)));
    let (cli_usage, cli_error) = split_fetch(oauth_quota.is_none().then(fetch_claude_cli_usage));
    let provider_error = if oauth_quota.is_some() || cli_usage.is_some() {
        None
    } else {
        oauth_error.as_ref().or(cli_error.as_ref()).cloned()
    };
    let status = if oauth_quota.is_some() || cli_usage.is_some() {
        UsageSnapshotStatus::Fresh
    } else {
        UsageSnapshotStatus::Stale
    };
    let buckets = oauth_quota
        .map(|usage| usage.into_buckets(now))
        .or_else(|| cli_usage.as_ref().map(ClaudeCliUsage::buckets))
        .filter(|buckets| !buckets.is_empty())
        .unwrap_or_else(|| claude_pending_buckets(status, provider_error.as_deref()));
    usage_view(UsageViewInput {
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
        last_error: match status {
            UsageSnapshotStatus::Stale => Some(provider_error.unwrap_or_else(|| {
                "Claude provider usage unavailable; cached quota is stale".to_owned()
            })),
            _ if cli_usage.is_some() => Some(oauth_error.clone().unwrap_or_else(|| {
                "Claude OAuth usage unavailable; showing reduced CLI snapshot".to_owned()
            })),
            _ => None,
        },
    })
}

// No `Debug`/`Display`: this carries a live access token and (optionally) the
// stable refresh token, so it must never be formatted into a log or error.
#[derive(Clone)]
pub(crate) struct ClaudeOAuthCredentials {
    pub(crate) access_token: String,
    pub(crate) subscription_type: Option<String>,
    /// Stable rotation-independent identity input. Consumed only inside wave
    /// resolution to derive the opaque account discriminator, then dropped —
    /// never carried into a view, log, snapshot, or coordination key raw.
    pub(crate) refresh_token: Option<String>,
}

/// Claude account email (F12): `~/.claude.json` carries `oauthAccount` metadata
/// (never the token), and `CodexBar` reads the address from there. Returns the
/// trimmed `oauthAccount.emailAddress`, or `None` when absent.
pub(crate) fn claude_email_from_value(value: &serde_json::Value) -> Option<String> {
    let oauth = value.get("oauthAccount")?;
    oauth
        .get("emailAddress")
        .or_else(|| oauth.get("email_address"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|email| !email.is_empty())
        .map(str::to_owned)
}

/// Claude account tier from `oauthAccount.organizationType` in `~/.claude.json`.
///
/// Enterprise/Team accounts store their billing model in `subscriptionType`
/// ("API Usage Billing"), not the account tier. `organizationType` carries the
/// tier directly (e.g. `"claude_enterprise"`, `"claude_max"`, `"claude_team"`) and is
/// the authoritative source for the plan label shown in the TUI header.
pub(crate) fn claude_organization_type_from_value(value: &serde_json::Value) -> Option<String> {
    value
        .get("oauthAccount")?
        .get("organizationType")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(humanize_plan_label)
}

pub(crate) fn load_claude_account_email(path: &Path) -> Option<String> {
    claude_email_from_value(&read_json_file(path)?)
}

#[cfg(test)]
pub(crate) fn load_claude_organization_type(path: &Path) -> Option<String> {
    claude_organization_type_from_value(&read_json_file(path)?)
}

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
    // Optional stable refresh token — used only to derive the coordination
    // discriminator when no `oauthAccount` metadata exists. Never surfaced.
    let refresh_token = oauth
        .get("refreshToken")
        .or_else(|| oauth.get("refresh_token"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(str::to_owned);
    Some(ClaudeOAuthCredentials {
        access_token,
        subscription_type,
        refresh_token,
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

/// Raw Keychain lookup outcome for one service. Secret-free in its own labels
/// (`json` carries the payload but the type is never formatted/logged).
pub(crate) enum ClaudeKeychainRead {
    #[cfg(any(target_os = "macos", test))]
    Payload {
        json: String,
    },
    Denied,
    Missing,
}

/// Classify a macOS `OSStatus` from a Keychain lookup. Only an explicit user
/// cancel (`errSecUserCanceled` = -128) or auth failure (`errSecAuthFailed` =
/// -25293) is a terminal `Denied`; item-not-found (-25300), headless
/// interaction-not-allowed (-25308), and any other failure are `Missing`
/// (absence), so file/env fallback stays available. Pure and cross-platform so
/// tests never touch the real Keychain.
#[cfg(any(target_os = "macos", test))]
pub(crate) fn classify_claude_keychain_status(code: i32) -> ClaudeKeychainRead {
    match code {
        -128 | -25293 => ClaudeKeychainRead::Denied,
        _ => ClaudeKeychainRead::Missing,
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn read_claude_keychain_item(service: &str) -> ClaudeKeychainRead {
    use security_framework::item::{ItemClass, ItemSearchOptions, SearchResult};

    let mut options = ItemSearchOptions::new();
    options
        .class(ItemClass::generic_password())
        .service(service)
        .load_data(true)
        .limit(1);
    match options.search() {
        Ok(results) => {
            for result in results {
                if let SearchResult::Data(bytes) = result {
                    return match String::from_utf8(bytes) {
                        Ok(text) if !text.trim().is_empty() => ClaudeKeychainRead::Payload {
                            json: text.trim().to_owned(),
                        },
                        _ => ClaudeKeychainRead::Missing,
                    };
                }
            }
            ClaudeKeychainRead::Missing
        }
        Err(error) => classify_claude_keychain_status(error.code()),
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn read_claude_keychain_item(_service: &str) -> ClaudeKeychainRead {
    ClaudeKeychainRead::Missing
}

/// Process-lifetime Keychain coordination: serializes reader I/O so a consent
/// sheet is prompted at most once per wave, and remembers services the operator
/// explicitly denied so a denial is terminal for that service for the process
/// (no retry-prompt storm). A *missing* item is never cached, so a later
/// `claude /login` is picked up without an app restart (flow W5).
#[derive(Default)]
pub(crate) struct ClaudeKeychainState {
    inner: std::sync::Mutex<ClaudeKeychainInner>,
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

pub(crate) struct ClaudeResolved {
    pub(crate) access_token: String,
    pub(crate) subscription_type: Option<String>,
    pub(crate) account_email: Option<String>,
    pub(crate) organization_type: Option<String>,
    pub(crate) credential_origin: String,
    /// `true` when the credential carries no proven cross-account identity (no
    /// account metadata and no refresh token) — a local-only credential.
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
/// call; `env_reader` yields an env access token. No process-global env
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
    E: FnOnce() -> Option<String>,
{
    match state.read_with(&scope.service, keychain_reader) {
        ClaudeKeychainRead::Denied => ClaudeWaveResolution::Denied,
        #[cfg(any(target_os = "macos", test))]
        ClaudeKeychainRead::Payload { json } => {
            match serde_json::from_str::<serde_json::Value>(&json)
                .ok()
                .as_ref()
                .and_then(claude_oauth_from_value)
            {
                Some(credential) => {
                    // Valid Keychain payload: may still collect account/tier
                    // metadata from the same-scope file probe, but the file
                    // credential can never replace the Keychain one.
                    let probe = file_probe();
                    let origin = format!("OAuth · macOS Keychain ({})", scope.service);
                    ClaudeWaveResolution::Resolved(Box::new(claude_resolved(
                        credential,
                        origin,
                        probe.account_email,
                        probe.organization_type,
                    )))
                }
                None => resolve_claude_fallback(scope, file_probe(), env_reader()),
            }
        }
        ClaudeKeychainRead::Missing => resolve_claude_fallback(scope, file_probe(), env_reader()),
    }
}

fn resolve_claude_fallback(
    scope: &jackin_core::ClaudeKeychainScope,
    probe: ClaudeFileProbe,
    env_token: Option<String>,
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
        )));
    }
    let _ = scope;
    if let Some(token) = env_token.filter(|value| !value.is_empty()) {
        return ClaudeWaveResolution::Resolved(Box::new(ClaudeResolved {
            access_token: token,
            subscription_type: None,
            account_email: probe.account_email,
            organization_type: probe.organization_type,
            credential_origin: "API token · env ANTHROPIC_API_KEY".to_owned(),
            is_anonymous: true,
        }));
    }
    ClaudeWaveResolution::Missing
}

fn claude_resolved(
    credential: ClaudeOAuthCredentials,
    origin: String,
    account_email: Option<String>,
    organization_type: Option<String>,
) -> ClaudeResolved {
    // Identity is proven by same-scope account metadata or the stable refresh
    // token; a rotating access token is never identity. Without either, the
    // credential is anonymous (local-only, no cross-account coordination).
    let is_anonymous = !(account_email
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
        || credential
            .refresh_token
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty()));
    ClaudeResolved {
        access_token: credential.access_token,
        subscription_type: credential.subscription_type,
        account_email,
        organization_type,
        credential_origin: origin,
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

#[derive(Debug, Clone, Default)]
pub(crate) struct ClaudeCliUsage {
    pub(crate) session_used: Option<f64>,
    pub(crate) weekly_used: Option<f64>,
    pub(crate) sonnet_used: Option<f64>,
    /// Per-model weekly windows the CLI prints as `Current week (<model>): …`
    /// (Fable today; future model codenames). Each entry is `(model label,
    /// percent used)`. Distinct from `sonnet_used`, which preserves the legacy
    /// `(Sonnet only)` line and its "Sonnet" bucket label.
    pub(crate) scoped_weekly: Vec<(String, f64)>,
}

impl ClaudeCliUsage {
    pub(crate) fn buckets(&self) -> Vec<QuotaBucketView> {
        // The CLI fallback reuses the same unified window model + builder as
        // the OAuth path, so a CLI "Weekly" line and an OAuth `weekly_all`
        // limit render identically (headline slot, over-cap label). CLI windows
        // carry no timestamps, so `now` is unused for pace/reset formatting.
        let mut windows: Vec<ClaudeQuotaWindow> = Vec::new();
        if let Some(used) = self.session_used {
            windows.push(ClaudeQuotaWindow::headline(
                "Session",
                StatusSlot::Session,
                used,
                Some(CLAUDE_SESSION_WINDOW_SECONDS),
            ));
        }
        if let Some(used) = self.weekly_used {
            windows.push(ClaudeQuotaWindow::headline(
                "Weekly",
                StatusSlot::Weekly,
                used,
                Some(CLAUDE_WEEKLY_WINDOW_SECONDS),
            ));
        }
        if let Some(used) = self.sonnet_used {
            windows.push(ClaudeQuotaWindow::scoped("Sonnet", used));
        }
        for (label, used) in &self.scoped_weekly {
            windows.push(ClaudeQuotaWindow::scoped(label, *used));
        }
        windows.into_iter().map(|w| w.into_bucket(0)).collect()
    }
}

/// Session (5-hour) window duration, shared by every source that produces one.
const CLAUDE_SESSION_WINDOW_SECONDS: i64 = 5 * 60 * 60;
/// Weekly window duration, shared by every source (`weekly_all`,
/// `weekly_scoped`, legacy `seven_day*`).
const CLAUDE_WEEKLY_WINDOW_SECONDS: i64 = 7 * 24 * 60 * 60;

/// One normalized Claude quota window — the single intermediate shape every
/// utilization source feeds before it becomes a [`QuotaBucketView`]. The
/// authoritative `limits` array, the legacy named windows (`seven_day*`), and
/// the `claude -p /usage` CLI fallback all produce `ClaudeQuotaWindow`s, so a
/// Session window, a Fable `weekly_scoped` limit, a legacy Sonnet window, and a
/// CLI "Weekly" line share one builder instead of three near-identical ones.
/// Fable is not a special case here — it is just another `weekly_scoped` entry.
#[derive(Debug, Clone)]
pub(crate) struct ClaudeQuotaWindow {
    pub(crate) label: String,
    pub(crate) slot: Option<StatusSlot>,
    /// Used fraction on the scale the shared helpers expect: a raw
    /// `utilization` (fraction-or-percent) for legacy/CLI sources, or
    /// `f64::from(percent)` for `limits`. `used_percent_label` and
    /// `remaining_from_fraction` resolve the fraction-vs-percent ambiguity, so
    /// both source shapes flow through unchanged.
    pub(crate) used: Option<f64>,
    pub(crate) reset_at: Option<i64>,
    pub(crate) window_seconds: Option<i64>,
    pub(crate) severity: UsageSeverity,
}

impl ClaudeQuotaWindow {
    /// A non-headline window with no reset/pace data (the CLI fallback shape).
    fn scoped(label: &str, used: f64) -> Self {
        Self {
            label: label.to_owned(),
            slot: None,
            used: Some(used),
            reset_at: None,
            window_seconds: None,
            severity: UsageSeverity::Normal,
        }
    }

    /// A headline window with a duration (so pace can be computed when the
    /// source also carries a reset). Used by the CLI Session/Weekly lines.
    fn headline(label: &str, slot: StatusSlot, used: f64, window_seconds: Option<i64>) -> Self {
        Self {
            label: label.to_owned(),
            slot: Some(slot),
            used: Some(used),
            reset_at: None,
            window_seconds,
            severity: UsageSeverity::Normal,
        }
    }

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
    /// name (omitted, never fabricated into an empty-label row).
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
) -> Result<ClaudeOAuthUsageResponse, String> {
    let user_agent = claude_code_user_agent();
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
            // The OAuth usage endpoint is gated to the Claude Code client UA;
            // a generic UA is rejected.
            (reqwest::header::USER_AGENT, &user_agent),
        ],
    )
}

pub(crate) fn claude_code_user_agent() -> String {
    // The Claude Code version is stable for the process lifetime, so resolve the
    // UA once instead of spawning `claude --version` on every usage fetch — that
    // per-probe subprocess was a measurable slice of the load latency (Bug 3).
    static CACHED: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    CACHED
        .get_or_init(|| {
            claude_code_user_agent_with(|command, args, timeout| {
                run_cli_with_timeout_full(command, args, timeout)
            })
            .unwrap_or_else(|| CLAUDE_CODE_USER_AGENT_FALLBACK.to_owned())
        })
        .clone()
}

pub(crate) fn claude_code_user_agent_with<F>(mut runner: F) -> Option<String>
where
    F: FnMut(&str, &[&str], Duration) -> Result<CliOutput, String>,
{
    let output = runner("claude", &["--version"], CLAUDE_VERSION_TIMEOUT).ok()?;
    if !output.success {
        return None;
    }
    let text = format!("{}\n{}", output.stdout, output.stderr);
    claude_code_version_from_text(&text).map(|version| format!("claude-code/{version}"))
}

pub(crate) fn claude_code_version_from_text(text: &str) -> Option<String> {
    text.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '.' || ch == '-'))
        .find(|part| {
            let mut segments = part.split('.');
            matches!(
                (segments.next(), segments.next(), segments.next()),
                (Some(major), Some(minor), Some(patch))
                    if major.chars().all(|ch| ch.is_ascii_digit())
                        && minor.chars().all(|ch| ch.is_ascii_digit())
                        && patch.chars().all(|ch| ch.is_ascii_digit())
            )
        })
        .map(str::to_owned)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaudeUsageDiagnostic {
    pub command: String,
    pub args: Vec<String>,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub fetched_at_epoch: i64,
}

pub(crate) fn fetch_claude_cli_usage() -> Result<ClaudeCliUsage, String> {
    let diagnostic = run_claude_usage_diagnostic()?;
    if !diagnostic.success {
        return Err(format!(
            "Claude CLI usage exited with status {:?}",
            diagnostic.exit_code
        ));
    }
    parse_claude_usage_output(&diagnostic.stdout)
        .ok_or_else(|| "Claude CLI usage output was not recognized".to_owned())
}
