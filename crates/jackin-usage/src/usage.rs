// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use jackin_protocol::control::{
    AccountUsageSnapshotView, FocusedAccountHeader, FocusedUsageView, Money, QuotaBucketView,
    StatusSlot, UsageConfidence, UsageProviderTab, UsageSeverity, UsageSnapshotStatus, UsageSource,
};
use jackin_telemetry::ResultTelemetryExt as _;
use serde::Serialize;

mod format;

mod amp;
mod claude;
mod codex;
mod grok;
mod kimi;
mod minimax;
mod refresh;
mod view;
mod zai;

#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) use self::amp::{
    AmpSuccessContext, AmpUsage, AmpWorkspaceBalance, amp_snapshot, amp_view_from_usage,
    fetch_amp_api_usage, fetch_amp_cli_usage, load_amp_api_key, parse_amp_usage_output,
};
pub use self::claude::ClaudeUsageDiagnostic;
#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) use self::claude::{
    ClaudeCliUsage, ClaudeOAuthCredentials, ClaudeOAuthExtraUsage, ClaudeOAuthLimit,
    ClaudeOAuthLimitModel, ClaudeOAuthLimitScope, ClaudeOAuthMoney, ClaudeOAuthSpend,
    ClaudeOAuthUsageResponse, ClaudeOAuthUsageWindow, ClaudeQuotaWindow, ClaudeSpend,
    ClaudeWavePolicy, ClaudeWaveResolution, classify_claude_keychain_status,
    claude_account_identity, claude_code_user_agent, claude_code_user_agent_with,
    claude_code_version_from_text, claude_email_from_value, claude_oauth_candidates,
    claude_oauth_from_value, claude_organization_type_from_value, claude_snapshot,
    claude_spend_bucket, claude_view_from_wave, claude_wave_policy, fetch_claude_cli_usage,
    fetch_claude_oauth_usage, load_claude_account_email, normalize_claude_spend,
    push_claude_dollar_windows, resolve_claude_wave,
};
#[cfg(test)]
pub(crate) use self::claude::{
    ClaudeFileProbe, ClaudeKeychainRead, ClaudeKeychainState, load_claude_oauth_credentials,
    load_claude_organization_type, resolve_claude_refresh_wave_with,
};
#[cfg(test)]
pub(crate) use self::codex::load_codex_oauth_credentials;
#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) use self::codex::{
    CodexAdditionalRateLimit, CodexCreditDetails, CodexOAuthCredentials, CodexRateLimitDetails,
    CodexResetCredit, CodexResetCredits, CodexRpcAccountDetails, CodexRpcAccountResponse,
    CodexRpcCredits, CodexRpcLimitEntry, CodexRpcRateLimitWindow, CodexRpcRateLimits,
    CodexRpcRateLimitsResponse, CodexRpcResetCredits, CodexRpcUsage, CodexUsageResponse,
    CodexWindowSnapshot, codex_access_token_from_response, codex_account_identity,
    codex_account_label_from_id_token, codex_auth_candidates, codex_oauth_from_value,
    codex_plan_display_name, codex_plan_exact_display, codex_plan_word_display,
    codex_refresh_request_body, codex_rpc_notification, codex_rpc_request, codex_snapshot,
    fetch_codex_oauth_reset_credits, fetch_codex_oauth_usage, fetch_codex_oauth_usage_refreshing,
    fetch_codex_rpc_usage, push_codex_window, refresh_codex_access_token, resolve_codex_base_url,
    resolve_codex_reset_credits_url, resolve_codex_usage_url,
};
#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) use self::grok::{
    GrokBillingConfig, GrokBillingResponse, GrokBillingSnapshot, GrokCent, GrokCurrentPeriod,
    GrokWebBillingSnapshot, fetch_grok_billing, fetch_grok_rpc_billing, fetch_grok_web_billing,
    grok_account_label, grok_account_label_or_presence, grok_bearer_token,
    grok_bearer_token_from_entry, grok_binary_path, grok_cycle_label_from_minutes,
    grok_cycle_label_from_reset, grok_rpc_request, grok_rpc_request_payload, grok_snapshot,
    grok_snapshot_from_rpc_result, grpc_web_data_frames, parse_grok_web_billing_response,
    scan_protobuf,
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
#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) use self::refresh::{
    MATERIALIZED_TMP_COUNTER, MaterializedUsageAccounts, RefreshLockOutcome,
    acquire_account_refresh_lock, acquire_account_refresh_lock_in, atomic_write_usage_json,
    collect_usage_refresh_results, collect_usage_refresh_results_with_timeout,
    ordered_refresh_targets, parse_retry_after_seconds, read_shared_usage_snapshot,
    record_persist_transition, refresh_interval_for_key, shared_usage_cooldown_active,
    shared_usage_cooldown_dir, shared_usage_cooldown_marker_path, shared_usage_file_path,
    shared_usage_lock_dir, shared_usage_rate_limit_cooldown_active, shared_usage_snapshot_mtime,
    shared_usage_snapshot_path, shared_usage_snapshots_dir, usage_backoff_delay,
    usage_error_is_rate_limited, usage_error_is_unauthorized, usage_rate_limit_delay,
    write_materialized_usage_accounts, write_shared_usage_cooldown_marker,
    write_shared_usage_snapshot,
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
    quota_amounts_for_account_snapshot, spend_headline_label, stale_shared_view,
    status_bar_fresh_or_stale, status_bar_headline_for_surface, status_bar_label,
    status_bar_quota_labels, surface_from_text, timed_bucket, usage_tab_source_label,
    usage_tab_status_label, usage_view, with_status_slot,
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
pub(crate) const PROVIDER_PROBE_TIMEOUT: Duration = Duration::from_secs(35);
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
    /// Per shared-account-key mtime of the last shared snapshot file we read.
    /// Skip the JSON parse when the file has not changed since the last check.
    shared_snapshot_mtimes: HashMap<String, SystemTime>,
    codex_rpc_gate: ManagedCliLaunchGate,
    grok_rpc_gate: ManagedCliLaunchGate,
    refresh_schedule: UsageRefreshSchedule,
    usage_snapshot_store_path: PathBuf,
    /// Destination for accounts.json materialization. Production uses
    /// [`MATERIALIZED_USAGE_ACCOUNTS_PATH`]; benches/tests inject a temp path
    /// via [`UsageCache::set_accounts_materialize_path`].
    accounts_materialize_path: PathBuf,
    telemetry_persist_failed: bool,
    accounts_materialize_failed: bool,
    /// Per-cache-key typed snapshot policy from the last refresh. Local-only
    /// entries (Claude Keychain denial/missing/anonymous) are excluded from
    /// account materialization and host durable/shared history restoration.
    active_snapshot_policy: HashMap<String, UsageSnapshotPolicy>,
    /// Test seam: count of shared-snapshot JSON reads during seeding.
    #[cfg(test)]
    pub(crate) shared_snapshot_json_reads: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct CachedUsage {
    pub(crate) view: FocusedUsageView,
}

pub(crate) struct UsageRefreshResult {
    pub(crate) target: UsageRefreshTarget,
    pub(crate) view: FocusedUsageView,
    pub(crate) policy: UsageSnapshotPolicy,
    pub(crate) codex_rpc_gate: ManagedCliLaunchGate,
    pub(crate) grok_rpc_gate: ManagedCliLaunchGate,
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

    /// Key for the host-shared snapshot/cooldown files: scoped to the resolved
    /// account, not just the provider surface, so same-account instances across
    /// containers coordinate while different accounts never collide (Class III).
    pub(crate) fn shared_account_key(&self) -> String {
        shared_usage_account_key(&self.agent, self.provider.as_deref())
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct UsageRefreshSchedule {
    pub(crate) next_due: HashMap<String, Instant>,
    pub(crate) rate_limit_failures: HashMap<String, u32>,
    pub(crate) in_flight: bool,
    /// Cache keys marked by [`Self::mark_due`] / force-refresh; consume once to
    /// bypass success cooldowns while still honoring hard rate-limit backoff.
    pub(crate) force_refresh: std::collections::HashSet<String>,
}

pub(crate) const USAGE_REFRESH_BASE_INTERVAL: Duration = Duration::from_mins(5);
pub(crate) const USAGE_REFRESH_JITTER: Duration = Duration::from_mins(1);
pub(crate) const USAGE_REFRESH_BACKOFF_CAP: Duration = Duration::from_mins(30);

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
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::Codex => "Codex",
            Self::Amp => "Amp",
            Self::Grok => "Grok Build",
            Self::Zai => "GLM / Z.AI",
            Self::Kimi => "Kimi",
            Self::Minimax => "MiniMax",
            Self::OpenCode => "OpenCode",
            Self::Unsupported => "Usage",
        }
    }

    pub(crate) fn account_label(self) -> &'static str {
        match self {
            Self::Claude => "Anthropic / Claude",
            Self::Codex => "OpenAI / Codex",
            Self::Amp => "Amp",
            Self::Grok => "xAI / Grok",
            Self::Zai => "GLM / Z.AI",
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
    /// Test-only helper: pin the snapshot path. Kept `pub` (not `#[cfg(test)]`)
    /// because `jackin-capsule`'s `daemon/tests.rs` uses it from a separate
    /// crate and Rust's `cfg(test)` does not propagate across crates.
    #[doc(hidden)]
    pub fn set_usage_snapshot_store_path(&mut self, path: PathBuf) {
        self.usage_snapshot_store_path = path;
    }

    /// Test-only helper: seed a snapshot into the cache. Kept `pub` for the
    /// same cross-crate reason as `set_usage_snapshot_store_path`.
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

    pub fn refresh_active_account_snapshots(
        &mut self,
        active_targets: &[UsageRefreshTarget],
        focused: Option<UsageRefreshTarget>,
        provider_keys: &BTreeMap<jackin_protocol::Provider, String>,
        now: Instant,
    ) {
        if self.refresh_schedule.in_flight {
            return;
        }
        let targets = ordered_refresh_targets(active_targets, focused);
        if targets.is_empty() {
            return;
        }
        self.refresh_schedule.in_flight = true;
        let snapshots_dir = shared_usage_snapshots_dir();
        // Propagation: a refresh completed by any process is visible to every
        // other process within one of its poll ticks, without network I/O.
        self.adopt_shared_snapshots(&targets, &snapshots_dir);
        let mut due_targets = Vec::new();
        for target in targets {
            if self.refresh_schedule.should_refresh(&target, now) {
                due_targets.push(target);
            }
        }
        if due_targets.is_empty() {
            self.refresh_schedule.in_flight = false;
            return;
        }
        // For each due target, in one pass (resolving the account key — which reads
        // credential files — exactly once): take the cross-container per-account
        // refresh lock, then write the pre-fetch advisory marker for the targets
        // we keep. A target held by another instance is dropped — it is being
        // refreshed there, and this instance already seeded the shared snapshot
        // above (Class III-D). The pre-fetch marker makes other instances that
        // reach `should_refresh` after this point skip, closing the race window to
        // ~RAM latency. The held lock handles live until the end of this method
        // (released on drop), spanning the fetch and the shared-snapshot write so
        // no other instance re-fetches the same account in that window.
        let cooldown_dir = shared_usage_cooldown_dir();
        let prefetch_until = now_epoch()
            .saturating_add(i64::try_from(PROVIDER_PROBE_TIMEOUT.as_secs()).unwrap_or(i64::MAX));
        let mut refresh_locks = Vec::new();
        due_targets.retain(|target| {
            let account_key = target.shared_account_key();
            match acquire_account_refresh_lock(&account_key) {
                RefreshLockOutcome::Held => return false,
                RefreshLockOutcome::Acquired(file) => refresh_locks.push(file),
                RefreshLockOutcome::Unavailable => {}
            }
            write_shared_usage_cooldown_marker(&cooldown_dir, &account_key, prefetch_until, "ok");
            true
        });
        if due_targets.is_empty() {
            self.refresh_schedule.in_flight = false;
            return;
        }
        let codex_rpc_gate = self.codex_rpc_gate.clone();
        let grok_rpc_gate = self.grok_rpc_gate.clone();
        let provider_keys = provider_keys.clone();
        jackin_diagnostics::incr_accounts_refreshed(due_targets.len() as u64);
        let results = collect_usage_refresh_results(due_targets, move |target| {
            let mut codex_rpc_gate = codex_rpc_gate.clone();
            let mut grok_rpc_gate = grok_rpc_gate.clone();
            let built = build_snapshot(
                &target.agent,
                target.provider.as_deref(),
                &provider_keys,
                &mut codex_rpc_gate,
                &mut grok_rpc_gate,
            );
            UsageRefreshResult {
                target,
                view: built.view,
                policy: built.policy,
                codex_rpc_gate,
                grok_rpc_gate,
            }
        });
        let mut stored_views = Vec::new();
        for result in results {
            let UsageRefreshResult {
                target,
                mut view,
                policy,
                codex_rpc_gate,
                grok_rpc_gate,
            } = result;
            let cache_key = canonical_usage_cache_key(&target.agent, target.provider.as_deref());
            // A local-only resolution (Keychain denial, missing credential, or an
            // anonymous credential with no proven identity) never restores stale
            // cached quota and never enters shared persistence/materialization.
            if !policy.is_local_only()
                && let Some(cached) = self.snapshots.get(&cache_key)
            {
                preserve_cached_quota_on_failed_refresh(&mut view, &cached.view);
            }
            enrich_provider_tabs(&mut view, &self.snapshots);
            self.snapshots
                .insert(cache_key.clone(), CachedUsage { view: view.clone() });
            self.active_snapshot_policy
                .insert(cache_key.clone(), policy);
            match resolve_surface(&target.agent, target.provider.as_deref()) {
                UsageSurface::Codex => self.codex_rpc_gate = codex_rpc_gate,
                UsageSurface::Grok => self.grok_rpc_gate = grok_rpc_gate,
                _ => {}
            }
            self.refresh_schedule.mark_refreshed(&target, now, &view);
            if !policy.is_local_only() {
                stored_views.push(view);
            }
        }
        if !stored_views.is_empty() {
            let result = crate::usage_snapshot_store::store_usage_snapshots(
                &self.usage_snapshot_store_path,
                &stored_views,
            );
            self.telemetry_persist_failed =
                record_persist_transition(self.telemetry_persist_failed, result);
        }
        let materialize = self.materialize_accounts(now_epoch());
        self.accounts_materialize_failed =
            record_persist_transition(self.accounts_materialize_failed, materialize);
        // Release the per-account refresh locks only now — after the shared
        // snapshot has been written — so a waiting instance that next wins the
        // lock sees fresh shared data rather than re-fetching (Class III-D).
        drop(refresh_locks);
        self.refresh_schedule.in_flight = false;
    }

    pub fn request_account_refresh(&mut self, target: &UsageRefreshTarget, now: Instant) {
        self.refresh_schedule.mark_due(target, now);
    }

    /// Adopt shared per-account snapshots into the in-memory cache.
    ///
    /// Contract: a refresh completed by any process is visible to every other
    /// process within one of its poll ticks, without that process performing
    /// network I/O. Vacant entries seed on first sight; occupied entries replace
    /// when the shared `fetched_at_epoch` is strictly newer. Steady-state cost is
    /// one `stat` per enabled account per tick (mtime map skips unchanged files).
    pub(crate) fn adopt_shared_snapshots(
        &mut self,
        targets: &[UsageRefreshTarget],
        snapshots_dir: &Path,
    ) {
        let now = now_epoch();
        for target in targets {
            let cache_key = target.cache_key();
            let account_key = target.shared_account_key();
            let Some(mtime) = shared_usage_snapshot_mtime(snapshots_dir, &account_key) else {
                if !self.snapshots.contains_key(&cache_key) {
                    jackin_telemetry::cache::decision(
                        jackin_telemetry::schema::enums::CacheName::UsageSnapshot,
                        jackin_telemetry::schema::enums::CacheResult::Miss,
                    );
                }
                continue;
            };
            if self.shared_snapshot_mtimes.get(&account_key) == Some(&mtime) {
                continue;
            }
            let Some(view) = read_shared_usage_snapshot(snapshots_dir, &account_key) else {
                if !self.snapshots.contains_key(&cache_key) {
                    jackin_telemetry::cache::decision(
                        jackin_telemetry::schema::enums::CacheName::UsageSnapshot,
                        jackin_telemetry::schema::enums::CacheResult::Miss,
                    );
                }
                continue;
            };
            #[cfg(test)]
            {
                self.shared_snapshot_json_reads = self.shared_snapshot_json_reads.saturating_add(1);
            }
            self.shared_snapshot_mtimes
                .insert(account_key.clone(), mtime);
            self.insert_adopted_shared_view(cache_key, view, now);
        }
    }

    fn insert_adopted_shared_view(&mut self, cache_key: String, view: FocusedUsageView, now: i64) {
        match self.snapshots.entry(cache_key) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                jackin_telemetry::cache::decision(
                    jackin_telemetry::schema::enums::CacheName::UsageSnapshot,
                    jackin_telemetry::schema::enums::CacheResult::Stale,
                );
                entry.insert(CachedUsage {
                    view: stale_shared_view(view, now),
                });
            }
            std::collections::hash_map::Entry::Occupied(mut entry) => {
                if view.fetched_at_epoch <= entry.get().view.fetched_at_epoch {
                    return;
                }
                jackin_telemetry::cache::decision(
                    jackin_telemetry::schema::enums::CacheName::UsageSnapshot,
                    jackin_telemetry::schema::enums::CacheResult::Stale,
                );
                entry.insert(CachedUsage {
                    view: stale_shared_view(view, now),
                });
            }
        }
    }

    pub(crate) fn materialize_accounts(&self, generated_at_epoch: i64) -> Result<(), String> {
        // Local-only entries (Claude Keychain denial/missing/anonymous) carry no
        // proven cross-account identity, so they never enter account materialization.
        let snapshots: Vec<&FocusedUsageView> = self
            .snapshots
            .iter()
            .filter(|(cache_key, _)| {
                !self
                    .active_snapshot_policy
                    .get(*cache_key)
                    .copied()
                    .unwrap_or(UsageSnapshotPolicy::Shared)
                    .is_local_only()
            })
            .map(|(_, cached)| &cached.view)
            .collect();
        write_materialized_usage_accounts(
            &self.accounts_materialize_path,
            generated_at_epoch,
            &snapshots,
        )
    }

    /// Typed active snapshot policy for a surface (defaults to `Shared`).
    pub(crate) fn active_snapshot_policy(
        &self,
        agent: &str,
        provider: Option<&str>,
    ) -> UsageSnapshotPolicy {
        self.active_snapshot_policy
            .get(&canonical_usage_cache_key(agent, provider))
            .copied()
            .unwrap_or(UsageSnapshotPolicy::Shared)
    }
}

impl Default for UsageCache {
    fn default() -> Self {
        Self {
            snapshots: HashMap::new(),
            shared_snapshot_mtimes: HashMap::new(),
            codex_rpc_gate: ManagedCliLaunchGate::default(),
            grok_rpc_gate: ManagedCliLaunchGate::default(),
            refresh_schedule: UsageRefreshSchedule::default(),
            usage_snapshot_store_path: PathBuf::from(USAGE_SNAPSHOT_STORE_PATH),
            accounts_materialize_path: PathBuf::from(MATERIALIZED_USAGE_ACCOUNTS_PATH),
            telemetry_persist_failed: false,
            accounts_materialize_failed: false,
            active_snapshot_policy: HashMap::new(),
            #[cfg(test)]
            shared_snapshot_json_reads: 0,
        }
    }
}

impl UsageRefreshSchedule {
    pub(crate) fn mark_due(&mut self, target: &UsageRefreshTarget, now: Instant) {
        let key = target.cache_key();
        self.next_due.insert(key.clone(), now);
        self.force_refresh.insert(key);
    }

    pub(crate) fn should_refresh(&mut self, target: &UsageRefreshTarget, now: Instant) -> bool {
        self.should_refresh_with_cooldown_dir(target, now, &shared_usage_cooldown_dir())
    }

    pub(crate) fn should_refresh_with_cooldown_dir(
        &mut self,
        target: &UsageRefreshTarget,
        now: Instant,
        cooldown_dir: &Path,
    ) -> bool {
        // `next_due` is per-instance scheduling (provider-keyed, in-memory); the
        // shared cooldown markers are cross-process and account-scoped so a
        // refresh by any process on the same account suppresses the others
        // (Class III). Success markers suppress timer-driven due checks; force
        // (mark_due / menu-bar Refresh) bypasses success but not rate-limit.
        let key = target.cache_key();
        match self.next_due.get(&key).copied() {
            // Common steady-state case: scheduled and not yet due. Returns without
            // resolving the account key, which would read credential files.
            Some(due) if due > now => false,
            Some(_) => {
                let forced = self.force_refresh.remove(&key);
                if forced {
                    !shared_usage_rate_limit_cooldown_active(
                        cooldown_dir,
                        &target.shared_account_key(),
                        now_epoch(),
                    )
                } else {
                    !shared_usage_cooldown_active(
                        cooldown_dir,
                        &target.shared_account_key(),
                        now_epoch(),
                    )
                }
            }
            None => {
                // First check for this instance: consult all shared cooldowns
                // (both 429 and success markers) to avoid thundering herd when
                // parallel instances all start simultaneously with empty next_due.
                if shared_usage_cooldown_active(
                    cooldown_dir,
                    &target.shared_account_key(),
                    now_epoch(),
                ) {
                    return false;
                }
                self.next_due.insert(key, now);
                true
            }
        }
    }

    pub(crate) fn mark_refreshed(
        &mut self,
        target: &UsageRefreshTarget,
        now: Instant,
        view: &FocusedUsageView,
    ) {
        self.mark_refreshed_with_cooldown_dir(
            target,
            now,
            view,
            &shared_usage_cooldown_dir(),
            &shared_usage_snapshots_dir(),
        );
    }

    pub(crate) fn mark_refreshed_with_cooldown_dir(
        &mut self,
        target: &UsageRefreshTarget,
        now: Instant,
        view: &FocusedUsageView,
        cooldown_dir: &Path,
        snapshots_dir: &Path,
    ) {
        // `key` schedules this instance (provider-keyed, in-memory); `account_key`
        // names the cross-container shared files so the cooldown/snapshot a refresh
        // produces is visible to other instances on the same account (Class III).
        let key = target.cache_key();
        let account_key = target.shared_account_key();
        if let Some(error) = view.last_error.as_deref()
            && usage_error_is_rate_limited(error)
        {
            let failures = self
                .rate_limit_failures
                .entry(key.clone())
                .and_modify(|count| *count = count.saturating_add(1))
                .or_insert(1);
            let delay = usage_rate_limit_delay(error, *failures);
            let until_epoch =
                now_epoch().saturating_add(i64::try_from(delay.as_secs()).unwrap_or(i64::MAX));
            write_shared_usage_cooldown_marker(cooldown_dir, &account_key, until_epoch, error);
            self.next_due.insert(key, now + delay);
        } else {
            self.rate_limit_failures.remove(&key);
            let refresh_interval = refresh_interval_for_key(&key);
            self.next_due.insert(key.clone(), now + refresh_interval);
            // Write success marker so parallel instances starting within the base
            // interval skip re-fetching the same provider — eliminating the
            // thundering herd where all instances fire simultaneously on startup.
            let success_until = now_epoch().saturating_add(
                i64::try_from(USAGE_REFRESH_BASE_INTERVAL.as_secs()).unwrap_or(i64::MAX),
            );
            write_shared_usage_cooldown_marker(cooldown_dir, &account_key, success_until, "ok");
            write_shared_usage_snapshot(snapshots_dir, &account_key, view);
        }
    }
}

pub(crate) fn stable_usage_hash(value: &str) -> u64 {
    value.bytes().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}

/// Resolve a directory from an env override, else a path under the process home.
/// Invariant: one host root `~/.jackin/data/usage-shared/*`; containers reach
/// the same root via the `/jackin/usage-shared` bind mount (runtime sets
/// `JACKIN_USAGE_*_DIR`); env vars override for tests only.
pub(crate) fn env_dir_or_home(env_var: &str, home_default: &str) -> PathBuf {
    std::env::var(env_var).map_or_else(|_| home_path(home_default), PathBuf::from)
}

/// Cross-container account identity for the shared snapshot/cooldown files
/// (Class III). Resolves the OAuth account identity from the credential for the
/// multi-account OAuth surfaces (Claude email, Codex `account_id`) and scopes the
/// key to it, so two containers on the same provider but different accounts (e.g.
/// two Claude logins) get distinct keys — no cross-account
/// collision, and same-account instances coordinate. Surfaces with no resolvable
/// OAuth identity (API-key providers, today single-credential per container) fall
/// back to the provider surface, preserving prior behavior.
pub(crate) fn shared_usage_account_key(agent: &str, focused_provider: Option<&str>) -> String {
    let surface = resolve_surface(agent, focused_provider);
    let identity = match surface {
        UsageSurface::Claude => claude_account_identity(),
        UsageSurface::Codex => codex_account_identity(),
        _ => None,
    };
    match identity {
        Some(id) => format!("{}#{:016x}", surface.label(), stable_usage_hash(&id)),
        None => canonical_usage_cache_key(agent, focused_provider),
    }
}

pub(crate) fn canonical_usage_cache_key(agent: &str, focused_provider: Option<&str>) -> String {
    let surface = resolve_surface(agent, focused_provider);
    if surface == UsageSurface::Unsupported {
        return format!("{agent}:{}", focused_provider.unwrap_or_default());
    }
    surface.label().to_owned()
}

#[cfg(test)]
pub fn resolved_usage_provider_label(
    agent: &str,
    focused_provider: Option<&str>,
) -> Option<&'static str> {
    let surface = resolve_surface(agent, focused_provider);
    (surface != UsageSurface::Unsupported).then_some(surface.label())
}

/// Shared provider display remap for Capsule tabs and jackin❯ Desktop overview.
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
    usage_detail_presentation, usage_display_status_label,
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

/// Why a snapshot must stay local to this process — a typed reason, never
/// inferred from error text. All three block cached-quota preservation, shared
/// adoption/coordination, persisted snapshot writes, and account materialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LocalOnlyReason {
    /// Operator denied the Keychain consent — terminal for the process.
    Denied,
    /// No usable credential — needs-login/fallback with no provider I/O.
    MissingCredential,
    /// Credential present but no proven cross-account identity.
    AnonymousCredential,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UsageSnapshotPolicy {
    Shared,
    LocalOnly(LocalOnlyReason),
}

impl UsageSnapshotPolicy {
    pub(crate) fn is_local_only(self) -> bool {
        matches!(self, Self::LocalOnly(_))
    }
}

pub(crate) struct BuiltUsageSnapshot {
    pub(crate) view: FocusedUsageView,
    pub(crate) policy: UsageSnapshotPolicy,
}

pub(crate) fn build_snapshot(
    agent: &str,
    provider: Option<&str>,
    provider_keys: &BTreeMap<jackin_protocol::Provider, String>,
    codex_rpc_gate: &mut ManagedCliLaunchGate,
    grok_rpc_gate: &mut ManagedCliLaunchGate,
) -> BuiltUsageSnapshot {
    let surface = resolve_surface(agent, provider);
    let now = now_epoch();
    if surface == UsageSurface::Claude {
        let resolution = resolve_claude_wave();
        let policy = match claude_wave_policy(&resolution) {
            ClaudeWavePolicy::Shared => UsageSnapshotPolicy::Shared,
            ClaudeWavePolicy::LocalDenied => {
                UsageSnapshotPolicy::LocalOnly(LocalOnlyReason::Denied)
            }
            ClaudeWavePolicy::LocalMissing => {
                UsageSnapshotPolicy::LocalOnly(LocalOnlyReason::MissingCredential)
            }
            ClaudeWavePolicy::LocalAnonymous => {
                UsageSnapshotPolicy::LocalOnly(LocalOnlyReason::AnonymousCredential)
            }
        };
        let view = claude_view_from_wave(agent, provider, now, resolution);
        return BuiltUsageSnapshot { view, policy };
    }
    let view = build_provider_view(
        agent,
        provider,
        surface,
        now,
        provider_keys,
        codex_rpc_gate,
        grok_rpc_gate,
    );
    BuiltUsageSnapshot {
        view,
        policy: UsageSnapshotPolicy::Shared,
    }
}

fn build_provider_view(
    agent: &str,
    provider: Option<&str>,
    surface: UsageSurface,
    now: i64,
    provider_keys: &BTreeMap<jackin_protocol::Provider, String>,
    codex_rpc_gate: &mut ManagedCliLaunchGate,
    grok_rpc_gate: &mut ManagedCliLaunchGate,
) -> FocusedUsageView {
    match surface {
        UsageSurface::Claude => claude_snapshot(agent, provider, now),
        UsageSurface::Codex => codex_snapshot(agent, provider, now, codex_rpc_gate),
        UsageSurface::Amp => amp_snapshot(agent, now),
        UsageSurface::Grok => grok_snapshot(agent, now, grok_rpc_gate),
        UsageSurface::Zai => {
            let key = provider_keys
                .get(&jackin_protocol::Provider::Zai)
                .cloned()
                .or_else(|| env_value("Z_AI_API_KEY"))
                .or_else(|| env_value("ZAI_API_KEY"));
            provider_key_snapshot(agent, surface, "ZAI_API_KEY", key.as_deref(), now)
        }
        UsageSurface::Kimi => {
            let token = env_value("KIMI_AUTH_TOKEN")
                .or_else(|| env_value("kimi_auth_token"))
                .or_else(|| load_kimi_local_token(now))
                .or_else(|| load_kimi_local_token_from_home(Path::new(KIMI_HANDOFF_HOME), now))
                .or_else(|| provider_keys.get(&jackin_protocol::Provider::Kimi).cloned())
                .or_else(|| env_value("KIMI_CODE_API_KEY"));
            kimi_snapshot(agent, token.as_deref(), now)
        }
        UsageSurface::Minimax => {
            let key = env_value("MINIMAX_CODING_API_KEY")
                .or_else(|| {
                    provider_keys
                        .get(&jackin_protocol::Provider::Minimax)
                        .cloned()
                })
                .or_else(|| env_value("MINIMAX_API_KEY"));
            minimax_snapshot(agent, key.as_deref(), now)
        }
        UsageSurface::OpenCode => opencode_snapshot(agent, provider, now),
        UsageSurface::Unsupported => unsupported_snapshot(agent, provider, now),
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
        account_label: "OpenCode stats source pending".to_owned(),
        username: None,
        plan_label: None,
        credential_origin: None,
        buckets: vec![bucket(
            "Usage",
            None,
            None,
            None,
            None,
            Some("opencode stats adapter pending"),
            UsageSnapshotStatus::Unsupported,
        )],
        status: UsageSnapshotStatus::Unsupported,
        source: UsageSource::None,
        confidence: UsageConfidence::None,
        now,
        last_error: Some(
            "OpenCode usage adapter is not part of this provider priority pass".to_owned(),
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
        "Updated just now".to_owned()
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
