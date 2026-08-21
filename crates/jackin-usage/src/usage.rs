// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![expect(
    dead_code,
    reason = "provider-adapter fixtures remain testable while production dispatch is broker-only"
)]

//! Focused-agent usage snapshots for Capsule.
//!
//! The TUI reads normalized cached snapshots from this module. Provider-specific
//! details stay here so status chrome and dialogs render strings, not API
//! branches.

use jackin_core::container_paths;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::future::Future;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use jackin_protocol::control::{
    AccountUsageSnapshotView, FocusedAccountHeader, FocusedUsageView, Money, QuotaBucketView,
    StatusSlot, UsageConfidence, UsageProviderTab, UsageSeverity, UsageSnapshotStatus, UsageSource,
};
use jackin_telemetry::ResultTelemetryExt as _;
use serde::Serialize;

#[path = "process_telemetry.rs"]
pub(crate) mod process_telemetry;

mod format;

mod amp;
mod claude;
mod codex;
mod grok;
mod kimi;
mod minimax;
mod opencode;
mod refresh;
mod view;
mod zai;

#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) use self::amp::{
    AmpSuccessContext, AmpUsage, AmpWorkspaceBalance, amp_api_key_snapshot, amp_snapshot,
    amp_view_from_usage, fetch_amp_api_usage, fetch_amp_cli_usage, load_amp_api_key,
    parse_amp_usage_output,
};
pub use self::claude::ClaudeUsageDiagnostic;
#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) use self::claude::{
    ClaudeCliUsage, ClaudeKeychainRead, ClaudeOAuthCredentials, ClaudeOAuthExtraUsage,
    ClaudeOAuthLimit, ClaudeOAuthLimitModel, ClaudeOAuthLimitScope, ClaudeOAuthMoney,
    ClaudeOAuthSpend, ClaudeOAuthUsageResponse, ClaudeOAuthUsageWindow, ClaudeQuotaWindow,
    ClaudeResolved, ClaudeSpend, ClaudeWavePolicy, ClaudeWaveResolution, claude_account_identity,
    claude_code_user_agent, claude_code_user_agent_with, claude_code_version_from_text,
    claude_email_from_value, claude_oauth_candidates, claude_oauth_from_value,
    claude_organization_type_from_value, claude_snapshot, claude_spend_bucket,
    claude_view_from_wave, claude_wave_policy, fetch_claude_cli_usage, fetch_claude_oauth_usage,
    load_claude_account_email, normalize_claude_spend, push_claude_dollar_windows,
    read_claude_keychain_item, resolve_claude_wave,
};
#[cfg(test)]
pub(crate) use self::claude::{
    ClaudeFileProbe, ClaudeKeychainState, classify_claude_keychain_status,
    load_claude_oauth_credentials, load_claude_organization_type, resolve_claude_refresh_wave_with,
};
#[cfg(test)]
pub(crate) use self::codex::load_codex_oauth_credentials;
#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) use self::codex::{
    CodexAdditionalRateLimit, CodexCreditDetails, CodexIndividualLimit, CodexOAuthCredentials,
    CodexRateLimitDetails, CodexResetCredit, CodexResetCredits, CodexRpcAccountDetails,
    CodexRpcAccountResponse, CodexRpcCredits, CodexRpcLimitEntry, CodexRpcRateLimitWindow,
    CodexRpcRateLimits, CodexRpcRateLimitsResponse, CodexRpcResetCredits, CodexRpcUsage,
    CodexSpendControl, CodexUsageResponse, CodexWindowSnapshot, codex_access_token_from_response,
    codex_account_identity, codex_account_label_from_id_token, codex_auth_candidates,
    codex_oauth_from_value, codex_plan_display_name, codex_plan_exact_display,
    codex_plan_word_display, codex_profile_snapshot, codex_refresh_request_body,
    codex_rpc_notification, codex_rpc_request, codex_snapshot, fetch_codex_oauth_reset_credits,
    fetch_codex_oauth_usage, fetch_codex_oauth_usage_refreshing, fetch_codex_rpc_usage,
    push_codex_window, refresh_codex_access_token, resolve_codex_base_url,
    resolve_codex_reset_credits_url, resolve_codex_usage_url,
};
#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) use self::grok::{
    GrokBillingConfig, GrokBillingResponse, GrokBillingSnapshot, GrokCent, GrokCurrentPeriod,
    GrokWebBillingSnapshot, fetch_grok_billing, fetch_grok_rest_billing, fetch_grok_rpc_billing,
    grok_account_label, grok_account_label_or_presence, grok_bearer_token,
    grok_bearer_token_from_entry, grok_binary_path, grok_cycle_label_from_minutes,
    grok_cycle_label_from_reset, grok_rpc_request, grok_rpc_request_payload, grok_snapshot,
    grok_snapshot_from_rpc_result, grpc_web_data_frames, parse_grok_rest_billing_response,
    parse_grok_web_billing_response, scan_protobuf,
};
#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) use self::kimi::{
    KimiRateLimit, KimiUsageDetail, KimiUsageItem, KimiUsageResponse, KimiWindow, fetch_kimi_usage,
    kimi_bucket, kimi_local_token_from_value, kimi_snapshot, kimi_window_seconds,
    load_kimi_local_token, load_kimi_local_token_from_home,
};
#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) use self::minimax::{
    MiniMaxBaseResponse, MiniMaxComboCard, MiniMaxModelRemain, MiniMaxUsageData,
    MiniMaxUsageResponse, MiniMaxWindow, fetch_minimax_usage, first_minimax_usage, minimax_bucket,
    minimax_bucket_label, minimax_is_general_model, minimax_operation_path, minimax_remains_host,
    minimax_reset_epoch, minimax_snapshot, minimax_usage_count_line, resolve_minimax_remains_urls,
    resolve_minimax_remains_urls_from,
};
pub(crate) use self::opencode::opencode_profile_snapshot;
#[cfg(test)]
pub(crate) use self::opencode::{load_opencode_api_key, parse_opencode_usage};
#[cfg(test)]
pub(crate) use self::refresh::MaterializedUsageAccounts;
#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) use self::refresh::{
    MATERIALIZED_TMP_COUNTER, atomic_write_usage_json, parse_retry_after_seconds,
    usage_error_is_rate_limited, usage_error_is_unauthorized, write_materialized_usage_accounts,
};
#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) use self::view::{
    UsageViewInput, account_snapshot_views_from_cache, amp_status_bar_headline, bucket,
    cached_refreshing_view, cached_unavailable_view, compact_account_identity, contains_word,
    decorate_surface_view, enrich_provider_tabs, mark_active_tab, most_constrained_fresh_bucket,
    preserve_cached_quota_on_failed_refresh, provider_matches_usage_label, provider_tabs,
    quota_amounts_for_account_snapshot, spend_headline_label, status_bar_fresh_or_stale,
    status_bar_headline_for_surface, status_bar_label, status_bar_quota_labels, surface_from_text,
    timed_bucket, usage_tab_source_label, usage_tab_status_label, usage_view, with_status_slot,
};
#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) use self::zai::{
    ZaiLimitRaw, ZaiQuotaData, ZaiQuotaResponse, fetch_zai_usage, json_epoch_seconds,
    provider_key_snapshot, resolve_zai_quota_url, resolve_zai_quota_url_from, zai_bucket,
    zai_count_line, zai_quota_host,
};

use format::{
    CliOutput, codex_account_from_value, codex_limit_label, compact_count, dollar_amounts,
    env_value, expiry_label, first_string_key, format_amount_with_unit, format_cents,
    format_currency, home_path, humanize_plan_label, humanize_words_with, json_number,
    oauth_origin, parse_iso_epoch, percent_before_used, quota_pace_label, remaining_from_fraction,
    reset_label, run_cli_with_timeout, run_cli_with_timeout_full, titlecase_ascii,
    used_percent_from_fraction, used_percent_label, window_minutes_label,
};
// Crate-visible re-exports for host overview/compact presentation (plan 008).
pub(crate) use format::{
    compact_duration_label, exact_reset_parenthetical, percent_headline, reset_label_with_prefs,
};

pub(crate) const PROVIDER_HTTP_TIMEOUT: Duration = Duration::from_secs(10);
pub(crate) const PROVIDER_CLI_TIMEOUT: Duration = Duration::from_secs(10);
pub(crate) const CODEX_RPC_INIT_TIMEOUT: Duration = Duration::from_secs(8);
pub(crate) const CODEX_RPC_REQUEST_TIMEOUT: Duration = Duration::from_secs(3);
pub(crate) const CODEX_RPC_LAUNCH_COOLDOWN: Duration = Duration::from_mins(30);
pub(crate) const CLAUDE_VERSION_TIMEOUT: Duration = Duration::from_secs(2);
pub(crate) const CLAUDE_CODE_USER_AGENT_FALLBACK: &str = "claude-code/2.1.0";
pub(crate) const GROK_RPC_INIT_TIMEOUT: Duration = Duration::from_secs(8);
pub(crate) const GROK_RPC_REQUEST_TIMEOUT: Duration = Duration::from_secs(12);
pub(crate) const MATERIALIZED_USAGE_ACCOUNTS_PATH: &str = container_paths::USAGE_ACCOUNTS;
pub(crate) const CODEX_HANDOFF_AUTH_PATH: &str = container_paths::CODEX_AUTH;
pub(crate) const AMP_HANDOFF_SECRETS_PATH: &str = container_paths::AMP_SECRETS;
pub(crate) const KIMI_HANDOFF_HOME: &str = container_paths::KIMI_CODE_DIR;
pub(crate) const GROK_HANDOFF_AUTH_PATH: &str = container_paths::GROK_AUTH;
pub(crate) const CLAUDE_HANDOFF_CREDENTIALS_PATH: &str = container_paths::CLAUDE_CREDENTIALS;
pub const USAGE_SNAPSHOT_STORE_PATH: &str = container_paths::USAGE_SNAPSHOT_STORE;

#[derive(Debug, Clone)]
pub struct UsageCache {
    snapshots: HashMap<String, CachedUsage>,
    /// Destination for accounts.json materialization. Production uses
    /// [`MATERIALIZED_USAGE_ACCOUNTS_PATH`]; benches/tests inject a temp path
    /// via [`UsageCache::set_accounts_materialize_path`].
    accounts_materialize_path: PathBuf,
}

#[derive(Debug, Clone)]
pub(crate) struct CachedUsage {
    pub(crate) view: FocusedUsageView,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageRefreshTarget {
    pub agent: String,
    pub provider: Option<String>,
}

impl UsageRefreshTarget {
    pub(crate) fn cache_key(&self) -> String {
        canonical_usage_cache_key(&self.agent, self.provider.as_deref())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UsageSurface {
    Claude,
    Codex,
    Amp,
    Grok,
    Zai,
    Kimi,
    Minimax,
    OpenCode,
    Unsupported,
}

impl UsageSurface {
    const fn id(self) -> Option<&'static str> {
        match self {
            Self::Claude => Some("claude"),
            Self::Codex => Some("codex"),
            Self::Amp => Some("amp"),
            Self::Grok => Some("grok"),
            Self::Zai => Some("zai"),
            Self::Kimi => Some("kimi"),
            Self::Minimax => Some("minimax"),
            Self::OpenCode => Some("opencode"),
            Self::Unsupported => None,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Claude => "Anthropic",
            Self::Codex => "OpenAI",
            Self::Amp => "Amp",
            Self::Grok => "xAI",
            Self::Zai => "Z.AI",
            Self::Kimi => "Kimi",
            Self::Minimax => "MiniMax",
            Self::OpenCode => "OpenCode",
            Self::Unsupported => "Usage",
        }
    }

    pub(crate) fn account_label(self) -> &'static str {
        match self {
            Self::Claude => "Anthropic",
            Self::Codex => "OpenAI",
            Self::Amp => "Amp",
            Self::Grok => "xAI",
            Self::Zai => "Z.AI",
            Self::Kimi => "Kimi",
            Self::Minimax => "MiniMax",
            Self::OpenCode => "OpenCode",
            Self::Unsupported => "Usage",
        }
    }

    /// Every surface, in resolution-precedence order. The single source of truth
    /// for "which providers exist" — iterate this instead of re-listing variants.
    const ALL: &'static [UsageSurface] = &[
        Self::Claude,
        Self::Codex,
        Self::Amp,
        Self::Grok,
        Self::Zai,
        Self::Kimi,
        Self::Minimax,
        Self::OpenCode,
        Self::Unsupported,
    ];

    /// Canonical identity tokens for free-text provider matching — the one alias
    /// table per variant. `surface_from_text` substring-scans these (Amp on a word
    /// boundary); `OpenCode`/`Unsupported` carry none so unknown text resolves to
    /// no surface. Entries must be lowercase: `surface_from_text` lowercases the
    /// haystack before comparing, so an uppercase token would never match. Order
    /// within a variant is a match-only alias set — not significant.
    pub(crate) fn synonyms(self) -> &'static [&'static str] {
        match self {
            Self::Claude => &["claude", "anthropic"],
            Self::Codex => &["codex", "openai"],
            Self::Amp => &["amp"],
            Self::Grok => &["grok", "xai"],
            Self::Zai => &["glm", "z.ai", "zai"],
            Self::Kimi => &["kimi"],
            Self::Minimax => &["minimax"],
            Self::OpenCode | Self::Unsupported => &[],
        }
    }
}

impl UsageCache {
    /// Test-only helper: seed a snapshot into the cache. Kept `pub` for the
    /// Capsule daemon tests in a separate crate.
    #[doc(hidden)]
    pub fn insert_snapshot_for_test(
        &mut self,
        agent: &str,
        focused_provider: Option<&str>,
        view: FocusedUsageView,
    ) {
        self.snapshots.insert(
            canonical_usage_cache_key(agent, focused_provider),
            CachedUsage { view },
        );
    }

    /// Bench/test helper: write materialized accounts to `path` instead of the
    /// container path. Cross-crate like `insert_snapshot_for_test`.
    #[doc(hidden)]
    pub fn set_accounts_materialize_path(&mut self, path: PathBuf) {
        self.accounts_materialize_path = path;
    }

    /// Bench/test entry: materialize the cache to the configured path.
    /// Production refresh calls the same body via [`Self::materialize_accounts`].
    #[doc(hidden)]
    pub fn materialize_accounts_for_bench(&self, generated_at_epoch: i64) -> Result<(), String> {
        self.materialize_accounts(generated_at_epoch)
    }

    pub fn focused_status_bar_label(
        &self,
        focused_agent: Option<&str>,
        focused_provider: Option<&str>,
    ) -> Option<String> {
        let agent = focused_agent?;
        // Label-only fast path: the status bar needs just `status_bar_label`, which
        // `cached_focused_usage_view`'s clone + enrich/mark-active never touch. Read
        // it straight from the stored view instead of cloning the whole snapshot.
        let cache_key = canonical_usage_cache_key(agent, focused_provider);
        if let Some(cached) = self.snapshots.get(&cache_key) {
            return Some(cached.view.status_bar_label.clone());
        }
        // A focused agent with no snapshot yet is mid-load — show `refreshing`
        // (clickable to force a load), never blank or a stale headline. The
        // segment is hidden only when there is no focused agent at all (the
        // `focused_agent?` above returns `None` → caller renders nothing).
        Some("refreshing".to_owned())
    }

    pub fn account_snapshot_views(&self) -> Vec<AccountUsageSnapshotView> {
        account_snapshot_views_from_cache(&self.snapshots)
    }

    pub fn focused_snapshot(
        &mut self,
        focused_agent: Option<&str>,
        focused_provider: Option<&str>,
    ) -> FocusedUsageView {
        let Some(agent) = focused_agent else {
            if let Some(provider) = focused_provider {
                return cached_unavailable_view("usage", Some(provider), now_epoch());
            }
            return FocusedUsageView::unavailable("no focused agent session", now_epoch());
        };
        let now = now_epoch();
        if let Some(view) = self.cached_focused_usage_view(agent, focused_provider) {
            return view;
        }
        // Agent is focused but no snapshot is cached yet: the agent has started
        // and the fetch is in flight — an honest "refreshing" state, not the
        // "usage unavailable" we reserve for a genuine absence.
        cached_refreshing_view(agent, focused_provider, now)
    }

    pub(crate) fn cached_focused_usage_view(
        &self,
        agent: &str,
        focused_provider: Option<&str>,
    ) -> Option<FocusedUsageView> {
        let cache_key = canonical_usage_cache_key(agent, focused_provider);
        let mut view = self.snapshots.get(&cache_key)?.view.clone();
        refresh_cached_updated_label(&mut view, now_epoch());
        if view.focused_agent.is_none() {
            view.focused_agent = Some(agent.to_owned());
        }
        if view.focused_provider.is_none() {
            view.focused_provider = focused_provider.map(str::to_owned);
        }
        enrich_provider_tabs(&mut view, &self.snapshots);
        mark_active_tab(&mut view);
        Some(view)
    }

    pub(crate) fn materialize_accounts(&self, generated_at_epoch: i64) -> Result<(), String> {
        let snapshots: Vec<&FocusedUsageView> =
            self.snapshots.values().map(|cached| &cached.view).collect();
        write_materialized_usage_accounts(
            &self.accounts_materialize_path,
            generated_at_epoch,
            &snapshots,
        )
    }
}

impl Default for UsageCache {
    fn default() -> Self {
        Self {
            snapshots: HashMap::new(),
            accounts_materialize_path: PathBuf::from(MATERIALIZED_USAGE_ACCOUNTS_PATH),
        }
    }
}

pub(crate) fn canonical_usage_cache_key(agent: &str, focused_provider: Option<&str>) -> String {
    let surface = resolve_surface(agent, focused_provider);
    if surface == UsageSurface::Unsupported {
        return format!("{agent}:{}", focused_provider.unwrap_or_default());
    }
    surface.label().to_owned()
}

pub(crate) fn env_dir_or_home(env_var: &str, home_default: &str) -> PathBuf {
    std::env::var(env_var).map_or_else(|_| home_path(home_default), PathBuf::from)
}

#[cfg(test)]
pub fn resolved_usage_provider_label(
    agent: &str,
    focused_provider: Option<&str>,
) -> Option<&'static str> {
    let surface = resolve_surface(agent, focused_provider);
    (surface != UsageSurface::Unsupported).then_some(surface.label())
}

/// Closed host-broker surface id for a Capsule refresh target.
#[must_use]
pub fn broker_surface_id(agent: &str, focused_provider: Option<&str>) -> Option<&'static str> {
    resolve_surface(agent, focused_provider).id()
}

/// Shared provider display remap for Capsule tabs and jackin❯ desktop overview.
///
/// Single mapping so Desktop never grows a second Swift-side provider rename.
#[must_use]
pub fn provider_display_label(label: &str) -> &str {
    match label {
        "Codex" | "OpenAI / Codex" => "OpenAI",
        "Claude" | "Anthropic / Claude" => "Anthropic",
        "Grok Build" | "xAI / Grok" => "xAI",
        "GLM / Z.AI" => "Z.AI",
        other => other,
    }
}

/// Honesty caption when numbers are estimated / local-log derived.
#[must_use]
pub fn estimate_caption(view: &FocusedUsageView) -> Option<String> {
    if matches!(view.confidence, UsageConfidence::Estimated)
        || matches!(view.source, UsageSource::LocalLogs)
    {
        Some("Estimated from token usage · not a subscription bill".to_owned())
    } else {
        None
    }
}

pub use self::format::{
    PercentStyle, ResetStyle, UsageBucketPresentation, UsageFormatPrefs, usage_bucket_presentation,
    usage_detail_presentation, usage_display_status_label, usage_identity_presentation,
};

pub fn usage_status_storage_label(status: UsageSnapshotStatus) -> &'static str {
    match status {
        UsageSnapshotStatus::Fresh => "fresh",
        UsageSnapshotStatus::Stale => "stale",
        UsageSnapshotStatus::NeedsLogin => "needs_login",
        UsageSnapshotStatus::NeedsSecret => "needs_secret",
        UsageSnapshotStatus::Unsupported => "unsupported",
        UsageSnapshotStatus::Unavailable => "unavailable",
        UsageSnapshotStatus::Error => "error",
    }
}

pub fn usage_source_storage_label(source: UsageSource) -> &'static str {
    match source {
        UsageSource::ProviderApi => "provider_api",
        UsageSource::Cli => "cli",
        UsageSource::LocalLogs => "local_logs",
        UsageSource::Cache => "cache",
        UsageSource::None => "none",
    }
}

pub fn usage_confidence_storage_label(confidence: UsageConfidence) -> &'static str {
    match confidence {
        UsageConfidence::Authoritative => "authoritative",
        UsageConfidence::Estimated => "estimated",
        UsageConfidence::PresenceOnly => "presence_only",
        UsageConfidence::None => "none",
    }
}

/// Build one explicit configured-provider snapshot while the caller retains the
/// credential. This is the tier-3 probe body used by tier-4 protected-source
/// adapters; the secret is never returned or persisted.
#[must_use]
pub fn provider_credential_snapshot(
    surface_id: &str,
    key_name: &str,
    secret: &str,
) -> FocusedUsageView {
    let now = now_epoch();
    match surface_id {
        "claude" => claude_view_from_wave(
            "claude",
            Some("Claude"),
            now,
            ClaudeWaveResolution::Resolved(Box::new(ClaudeResolved {
                access_token: secret.to_owned(),
                subscription_type: None,
                account_email: None,
                organization_type: None,
                credential_origin: "OAuth · configured source".to_owned(),
                is_anonymous: true,
            })),
        ),
        "amp" => amp_api_key_snapshot("amp", secret, now),
        "zai" => provider_key_snapshot("codex", UsageSurface::Zai, key_name, Some(secret), now),
        "kimi" => kimi_snapshot("kimi", Some(secret), now),
        "minimax" => minimax_snapshot("codex", Some(secret), now),
        "grok" => grok_snapshot_from_rpc_result(
            "grok",
            now,
            Path::new(GROK_HANDOFF_AUTH_PATH),
            false,
            key_name == jackin_core::XAI_API_KEY_ENV_NAME,
            key_name == jackin_core::GROK_DEPLOYMENT_KEY_ENV_NAME,
            Err("Grok billing requires an authenticated profile".to_owned()),
        ),
        "codex" => usage_view(UsageViewInput {
            agent: "codex",
            provider: Some("OpenAI"),
            surface: UsageSurface::Codex,
            account_label: String::new(),
            username: None,
            plan_label: None,
            credential_origin: Some("API key · configured source".to_owned()),
            buckets: Vec::new(),
            status: UsageSnapshotStatus::Unsupported,
            source: UsageSource::None,
            confidence: UsageConfidence::None,
            now,
            last_error: Some("OpenAI API-key subscription quota is unavailable".to_owned()),
        }),
        _ => unsupported_snapshot(surface_id, None, now),
    }
}

pub(crate) fn resolve_surface(agent: &str, provider: Option<&str>) -> UsageSurface {
    if matches!(
        provider,
        Some("Claude" | "Claude Code" | "Anthropic" | "Anthropic / Claude")
    ) {
        return UsageSurface::Claude;
    }
    if matches!(provider, Some("Codex" | "OpenAI" | "OpenAI / Codex")) {
        return UsageSurface::Codex;
    }
    if matches!(provider, Some("Amp")) {
        return UsageSurface::Amp;
    }
    if matches!(provider, Some("Grok" | "Grok Build" | "xAI" | "xAI / Grok")) {
        return UsageSurface::Grok;
    }
    if matches!(provider, Some("Z.AI" | "GLM" | "GLM / Z.AI")) {
        return UsageSurface::Zai;
    }
    if matches!(provider, Some("Kimi")) {
        return UsageSurface::Kimi;
    }
    if matches!(provider, Some("MiniMax")) {
        return UsageSurface::Minimax;
    }
    match agent {
        "claude" => UsageSurface::Claude,
        "codex" => UsageSurface::Codex,
        "amp" => UsageSurface::Amp,
        "grok" => UsageSurface::Grok,
        "kimi" => UsageSurface::Kimi,
        "opencode" => UsageSurface::OpenCode,
        _ => UsageSurface::Unsupported,
    }
}

/// Split an optional provider fetch into its `(data, error)` pair: `None` token
/// → no attempt, `Some(Ok)` → data, `Some(Err)` → error. Replaces the
/// `match token { Some => match fetch { … }, None => (None, None) }` boilerplate
/// at every provider fetch site (`token.map(fetch)` feeds this).
pub(crate) fn split_fetch<U>(result: Option<Result<U, String>>) -> (Option<U>, Option<String>) {
    match result {
        Some(Ok(value)) => (Some(value), None),
        Some(Err(error)) => (None, Some(error)),
        None => (None, None),
    }
}

/// Inputs to [`provider_outcome`]. Named fields so the two booleans can't be
/// silently swapped at a call site.
pub(crate) struct ProviderPresence {
    pub(crate) has_data: bool,
    pub(crate) has_secret: bool,
}

/// Lifecycle triad for the simple "API key or nothing" providers: data present →
/// fresh/authoritative; a secret present but no data → unsupported/presence-only;
/// neither → needs-secret. Providers with login/CLI/error nuances (Claude, Codex,
/// Amp, Grok) keep their bespoke logic.
pub(crate) fn provider_outcome(
    presence: ProviderPresence,
) -> (UsageSnapshotStatus, UsageSource, UsageConfidence) {
    let ProviderPresence {
        has_data,
        has_secret,
    } = presence;
    if has_data {
        (
            UsageSnapshotStatus::Fresh,
            UsageSource::ProviderApi,
            UsageConfidence::Authoritative,
        )
    } else if has_secret {
        (
            UsageSnapshotStatus::Unsupported,
            UsageSource::None,
            UsageConfidence::PresenceOnly,
        )
    } else {
        (
            UsageSnapshotStatus::NeedsSecret,
            UsageSource::None,
            UsageConfidence::None,
        )
    }
}

pub(crate) fn opencode_snapshot(agent: &str, provider: Option<&str>, now: i64) -> FocusedUsageView {
    usage_view(UsageViewInput {
        agent,
        provider,
        surface: UsageSurface::OpenCode,
        account_label: "OpenCode account (unresolved)".to_owned(),
        username: None,
        plan_label: None,
        credential_origin: None,
        buckets: vec![bucket(
            "Usage",
            None,
            None,
            None,
            None,
            Some("OpenCode Go credential is unavailable"),
            UsageSnapshotStatus::Unsupported,
        )],
        status: UsageSnapshotStatus::Unsupported,
        source: UsageSource::None,
        confidence: UsageConfidence::None,
        now,
        last_error: Some(
            "OpenCode account identity is provisional until the provider exposes a non-secret identifier".to_owned(),
        ),
    })
}

pub(crate) fn unsupported_snapshot(
    agent: &str,
    provider: Option<&str>,
    now: i64,
) -> FocusedUsageView {
    usage_view(UsageViewInput {
        agent,
        provider,
        surface: UsageSurface::Unsupported,
        account_label: "unsupported focused agent".to_owned(),
        username: None,
        plan_label: None,
        credential_origin: None,
        buckets: Vec::new(),
        status: UsageSnapshotStatus::Unsupported,
        source: UsageSource::None,
        confidence: UsageConfidence::None,
        now,
        last_error: Some(format!("no usage adapter for agent {agent:?}")),
    })
}

/// Resolve a credential from an ordered candidate list, returning the first path
/// that yields a usable value via `load` together with that winning path. Used
/// by Amp for its single file credential; the home-first / handoff-last ordering
/// it encodes — the agent's own home location(s) first (the live source of truth
/// the agent reads and refreshes), then the runtime-forwarded `/jackin/<provider>/`
/// handoff as the last-resort fallback — is the same ordering `resolve_identity`
/// applies for the dual-concern providers (Claude, Codex), so credential order is
/// uniform across providers. The winning path is returned so the `Auth:` origin
/// can name the file that actually produced the credential instead of re-`stat`ing
/// and guessing.
pub(crate) fn first_credential_with_path<T>(
    paths: &[PathBuf],
    load: impl Fn(&Path) -> Option<T>,
) -> Option<(PathBuf, T)> {
    paths
        .iter()
        .find_map(|path| load(path.as_path()).map(|value| (path.clone(), value)))
}

#[cfg(test)]
pub(crate) fn first_credential<T>(
    paths: &[PathBuf],
    load: impl Fn(&Path) -> Option<T>,
) -> Option<T> {
    first_credential_with_path(paths, load).map(|(_, value)| value)
}

/// Read and parse a JSON credential/config file, distinguishing expected
/// absence from a present-but-broken typed telemetry error.
pub(crate) fn read_json_file(path: &Path) -> Option<serde_json::Value> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        result => result
            .record_telemetry_error(jackin_telemetry::schema::enums::ErrorType::IoError)
            .ok()?,
    };
    serde_json::from_str(&text)
        .record_telemetry_error(jackin_telemetry::schema::enums::ErrorType::ConfigError)
        .ok()
}

/// Resolve a provider credential (with the winning path, for the `Auth:`
/// origin) and its account label in one home-first walk, reading and parsing
/// each candidate file at most once. `extract_credential` pulls the token from a
/// parsed file; `extract_label` pulls the account email/label. The walk stops as
/// soon as both are found, so a later candidate never re-reads a resolved file.
pub(crate) fn resolve_identity<T>(
    candidates: &[PathBuf],
    extract_credential: impl Fn(&serde_json::Value) -> Option<T>,
    extract_label: impl Fn(&serde_json::Value) -> Option<String>,
) -> (Option<(PathBuf, T)>, Option<String>) {
    let (result, label, _) =
        resolve_identity_with_extra(candidates, extract_credential, extract_label, |_| {
            None::<String>
        });
    (result, label)
}

/// Like `resolve_identity` but also extracts a third field in the same walk,
/// avoiding a second pass over the candidate files.
pub(crate) fn resolve_identity_with_extra<T>(
    candidates: &[PathBuf],
    extract_credential: impl Fn(&serde_json::Value) -> Option<T>,
    extract_label: impl Fn(&serde_json::Value) -> Option<String>,
    extract_extra: impl Fn(&serde_json::Value) -> Option<String>,
) -> (Option<(PathBuf, T)>, Option<String>, Option<String>) {
    let mut credential = None;
    let mut label = None;
    let mut extra = None;
    for path in candidates {
        if credential.is_some() && label.is_some() && extra.is_some() {
            break;
        }
        let Some(value) = read_json_file(path) else {
            continue;
        };
        if credential.is_none()
            && let Some(found) = extract_credential(&value)
        {
            credential = Some((path.clone(), found));
        }
        if label.is_none() {
            label = extract_label(&value);
        }
        if extra.is_none() {
            extra = extract_extra(&value);
        }
    }
    (credential, label, extra)
}

pub(crate) fn severity_from_label(label: Option<&str>) -> UsageSeverity {
    match label.map(str::to_ascii_lowercase).as_deref() {
        Some("warn" | "warning" | "elevated") => UsageSeverity::Warn,
        Some("danger" | "critical" | "exceeded") => UsageSeverity::Danger,
        _ => UsageSeverity::Normal,
    }
}

/// Turn an API reason slug (`out_of_credits`) into a human phrase
/// (`out of credits`) for the disabled-spend pace label.
pub(crate) fn humanize_reason(reason: &str) -> String {
    reason.replace(['_', '-'], " ")
}

/// Title-case a codename window key (`amber_ladder` → `Amber Ladder`) for use as
/// a bucket label. Distinct from [`humanize_reason`] (which yields a lowercase
/// phrase for inline pace text); a window label is a proper-noun-style heading
/// shown beside `Session`/`Weekly`.
pub(crate) fn humanize_window_label(key: &str) -> String {
    key.split(['_', '-'])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ManagedCliLaunchGate {
    pub(crate) cooldown_until: Option<Instant>,
    pub(crate) last_error: Option<String>,
}

impl ManagedCliLaunchGate {
    pub(crate) fn can_launch(&self, label: &str, now: Instant) -> Result<(), String> {
        if let Some(until) = self.cooldown_until
            && now < until
        {
            let remaining = until.saturating_duration_since(now).as_secs() / 60;
            return Err(format!(
                "{label} launch cooldown active for {}m: {}",
                remaining.max(1),
                self.last_error
                    .as_deref()
                    .unwrap_or("previous launch failed")
            ));
        }
        Ok(())
    }

    pub(crate) fn record_launch_failure(&mut self, message: String) {
        self.cooldown_until = Some(Instant::now() + CODEX_RPC_LAUNCH_COOLDOWN);
        self.last_error = Some(message);
    }

    pub(crate) fn record_success(&mut self) {
        self.cooldown_until = None;
        self.last_error = None;
    }
}

#[derive(Debug, Default)]
pub(crate) struct ProtobufScan {
    pub(crate) fixed32_fields: Vec<Fixed32Field>,
    pub(crate) varint_fields: Vec<VarintField>,
}

#[derive(Debug)]
pub(crate) struct Fixed32Field {
    pub(crate) path: Vec<u64>,
    pub(crate) value: f32,
    pub(crate) order: usize,
}

#[derive(Debug)]
pub(crate) struct VarintField {
    pub(crate) path: Vec<u64>,
    pub(crate) value: u64,
}

impl ProtobufScan {
    pub(crate) fn merge(&mut self, other: Self) {
        self.fixed32_fields.extend(other.fixed32_fields);
        self.varint_fields.extend(other.varint_fields);
    }
}

pub(crate) fn looks_like_protobuf_payload(data: &[u8]) -> bool {
    let Some(first) = data.first() else {
        return false;
    };
    let field_number = first >> 3;
    let wire_type = first & 0x07;
    field_number > 0 && matches!(wire_type, 0 | 1 | 2 | 5)
}

pub(crate) fn read_varint(data: &[u8], index: &mut usize) -> Option<u64> {
    let mut value = 0u64;
    let mut shift = 0u32;
    while *index < data.len() && shift < 64 {
        let byte = data[*index];
        *index += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
        shift += 7;
    }
    None
}

pub(crate) fn write_json_line(
    stdin: &mut impl Write,
    payload: &serde_json::Value,
    encode_context: &str,
    write_context: &str,
) -> Result<(), String> {
    serde_json::to_writer(&mut *stdin, payload)
        .map_err(|err| format!("{encode_context}: {err}"))?;
    stdin
        .write_all(b"\n")
        .and_then(|()| stdin.flush())
        .map_err(|err| format!("{write_context}: {err}"))
}

/// `OpenAI` OAuth token endpoint and the Codex CLI's public client id (the same
/// values the CLI uses for its own refresh grant — neither is a secret).
pub(crate) const CODEX_OAUTH_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
pub(crate) const CODEX_OAUTH_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";

#[derive(Clone)]
struct ProviderConnectionLayer {
    dispatcher: tracing::Dispatch,
}

impl ProviderConnectionLayer {
    fn capture() -> Self {
        Self {
            dispatcher: tracing::dispatcher::get_default(Clone::clone),
        }
    }
}

impl<S> tower::Layer<S> for ProviderConnectionLayer {
    type Service = ProviderConnectionService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        ProviderConnectionService {
            inner,
            dispatcher: self.dispatcher.clone(),
        }
    }
}

#[derive(Clone)]
struct ProviderConnectionService<S> {
    inner: S,
    dispatcher: tracing::Dispatch,
}

impl<S, Request> tower::Service<Request> for ProviderConnectionService<S>
where
    S: tower::Service<Request> + Send,
    S::Future: Send + 'static,
    S::Response: 'static,
    S::Error: 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = std::pin::Pin<
        Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>,
    >;

    fn poll_ready(
        &mut self,
        context: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(context)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let operation = tracing::dispatcher::with_default(&self.dispatcher, || {
            jackin_telemetry::operation_or_disabled(
                &jackin_telemetry::operation::CONNECTION_ATTEMPT,
                &[jackin_telemetry::Attr {
                    key: jackin_telemetry::schema::attrs::CONNECTION_PEER_TYPE,
                    value: jackin_telemetry::Value::Str(
                        jackin_telemetry::schema::enums::ConnectionPeerType::Provider.as_str(),
                    ),
                }],
            )
        });
        let future = self.inner.call(request);
        Box::pin(async move {
            let result = future.await;
            operation.complete(
                if result.is_ok() {
                    jackin_telemetry::schema::enums::OutcomeValue::Success
                } else {
                    jackin_telemetry::schema::enums::OutcomeValue::Error
                },
                result
                    .as_ref()
                    .err()
                    .map(|_| jackin_telemetry::schema::enums::ErrorType::IoError),
            );
            result
        })
    }
}

pub(crate) fn parse_chatgpt_base_url(contents: &str) -> Option<String> {
    for raw_line in contents.lines() {
        let line = raw_line.split('#').next().unwrap_or_default().trim();
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "chatgpt_base_url" {
            continue;
        }
        let value = value
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
            .trim()
            .to_owned();
        if !value.is_empty() {
            return Some(value);
        }
    }
    None
}

pub(crate) fn provider_http_client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(PROVIDER_HTTP_TIMEOUT)
        .connect_timeout(PROVIDER_HTTP_TIMEOUT)
        .connector_layer(ProviderConnectionLayer::capture())
        .build()
        .map_err(|err| format!("provider HTTP client unavailable: {err}"))
}

pub(crate) fn provider_request<T>(
    provider: jackin_telemetry::schema::enums::ProviderName,
    method: &'static str,
    template: &'static str,
    request: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let attrs = [
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::GEN_AI_PROVIDER_NAME,
            value: jackin_telemetry::Value::Str(provider.as_str()),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::HTTP_REQUEST_METHOD,
            value: jackin_telemetry::Value::Str(method),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::URL_TEMPLATE,
            value: jackin_telemetry::Value::Str(template),
        },
    ];
    let operation =
        jackin_telemetry::operation_or_disabled(&jackin_telemetry::operation::HTTP_CLIENT, &attrs);
    let result = request();
    operation.complete(
        if result.is_ok() {
            jackin_telemetry::schema::enums::OutcomeValue::Success
        } else {
            jackin_telemetry::schema::enums::OutcomeValue::Failure
        },
        result
            .as_ref()
            .err()
            .map(|_| jackin_telemetry::schema::enums::ErrorType::HttpError),
    );
    result
}

/// Shared GET → bearer-auth → JSON skeleton for provider quota endpoints. The
/// caller supplies the human label (used verbatim in every error string so the
/// per-provider wording is unchanged), the URL, the bearer token, and any extra
/// request headers beyond the always-sent `Accept: application/json`. Per-
/// provider response validation stays at the call site.
pub(crate) fn get_json_bearer<T: serde::de::DeserializeOwned>(
    provider: jackin_telemetry::schema::enums::ProviderName,
    template: &'static str,
    label: &str,
    url: &str,
    token: &str,
    extra_headers: &[(reqwest::header::HeaderName, &str)],
) -> Result<T, String> {
    provider_request(provider, "GET", template, || {
        let client = provider_http_client()?;
        let mut request = client
            .get(url)
            .bearer_auth(token)
            .header(reqwest::header::ACCEPT, "application/json");
        for (name, value) in extra_headers {
            request = request.header(name.clone(), *value);
        }
        let response = request
            .send()
            .map_err(|err| format!("{label} request failed: {err}"))?;
        let status = response.status();
        if !status.is_success() {
            return Err(format!("{label} HTTP {status}"));
        }
        response
            .json::<T>()
            .map_err(|err| format!("{label} decode failed: {err}"))
    })
}

pub(crate) fn epoch_seconds_from_maybe_ms(value: i64) -> i64 {
    if value > 1_000_000_000_000 {
        value / 1000
    } else {
        value
    }
}

pub(crate) fn normalize_url_or_host(value: &str, suffix: &str) -> String {
    let mut cleaned = value
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .to_owned();
    if !cleaned.starts_with("http://") && !cleaned.starts_with("https://") {
        cleaned = format!("https://{cleaned}");
    }
    if suffix.is_empty() {
        return cleaned;
    }
    let trimmed = cleaned.trim_end_matches('/');
    if trimmed.ends_with(suffix) {
        trimmed.to_owned()
    } else {
        format!("{trimmed}/{suffix}")
    }
}

pub fn run_claude_usage_diagnostic() -> Result<ClaudeUsageDiagnostic, String> {
    run_claude_usage_diagnostic_with(|command, args, timeout| {
        run_cli_with_timeout_full(command, args, timeout)
    })
}

pub(crate) fn run_claude_usage_diagnostic_with<F>(
    mut runner: F,
) -> Result<ClaudeUsageDiagnostic, String>
where
    F: FnMut(&str, &[&str], Duration) -> Result<CliOutput, String>,
{
    let args = ["-p", "/usage"];
    let output = runner("claude", &args, PROVIDER_CLI_TIMEOUT)?;
    Ok(ClaudeUsageDiagnostic {
        command: "claude".to_owned(),
        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        success: output.success,
        exit_code: output.exit_code,
        stdout: output.stdout,
        stderr: output.stderr,
        fetched_at_epoch: now_epoch(),
    })
}

pub(crate) fn parse_claude_usage_output(text: &str) -> Option<ClaudeCliUsage> {
    let mut usage = ClaudeCliUsage::default();
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if line.starts_with("Current session:") {
            usage.session_used = percent_before_used(line);
        } else if line.starts_with("Current week (all models):") {
            usage.weekly_used = percent_before_used(line);
        } else if line.starts_with("Current week (Sonnet only):") {
            usage.sonnet_used = percent_before_used(line);
        } else if let Some(rest) = line.strip_prefix("Current week (") {
            // Per-model weekly line, e.g. "Current week (Fable): 35% used · …".
            // The model name is the text between the parens; "all models" and
            // "Sonnet only" are handled by the explicit branches above, so
            // anything reaching here is a model-scoped window (Fable today,
            // future codenames tomorrow). Surfaced generically so a new model
            // prints without a per-model parser edit.
            if let Some(close) = rest.find(')') {
                let label = rest[..close].trim();
                if !label.is_empty()
                    && let Some(percent) = percent_before_used(line)
                {
                    usage.scoped_weekly.push((label.to_owned(), percent));
                }
            }
        }
    }
    (usage.session_used.is_some()
        || usage.weekly_used.is_some()
        || usage.sonnet_used.is_some()
        || !usage.scoped_weekly.is_empty())
    .then_some(usage)
}

/// `Auth:` origin label for an OAuth credential resolved from `path`, with the
/// home dir collapsed to `~` (so it reads `~/.codex/auth.json`, not an absolute
/// container path). Shared by the Claude and Codex snapshots.
pub(crate) fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

pub fn relative_updated_label(fetched_at: i64, now_epoch: i64) -> String {
    let age = now_epoch.saturating_sub(fetched_at).max(0);
    if age < 60 {
        "Updated now".to_owned()
    } else if age < 3_600 {
        format!("Updated {}m ago", age / 60)
    } else {
        format!("Updated {}h ago", age / 3_600)
    }
}

pub(crate) fn refresh_cached_updated_label(view: &mut FocusedUsageView, now_epoch: i64) {
    if matches!(
        view.status,
        UsageSnapshotStatus::Fresh | UsageSnapshotStatus::Stale
    ) || view.updated_label.trim().is_empty()
    {
        view.updated_label = relative_updated_label(view.fetched_at_epoch, now_epoch);
    }
}

#[cfg(test)]
mod tests;
