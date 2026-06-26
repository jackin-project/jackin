//! Focused-agent usage snapshots for Capsule.
//!
//! The TUI reads normalized cached snapshots from this module. Provider-specific
//! details stay here so status chrome and dialogs render strings, not API
//! branches.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use chrono::{DateTime, Local, TimeZone, Utc};
use jackin_protocol::control::{
    AccountUsageSnapshotView, FocusedAccountHeader, FocusedUsageView, QuotaBucketView, StatusSlot,
    UsageConfidence, UsageProviderTab, UsageSnapshotStatus, UsageSource,
};
use serde::{Deserialize, Serialize};

const PROVIDER_HTTP_TIMEOUT: Duration = Duration::from_secs(10);
const PROVIDER_CLI_TIMEOUT: Duration = Duration::from_secs(10);
const PROVIDER_PROBE_TIMEOUT: Duration = Duration::from_secs(35);
const CODEX_RPC_INIT_TIMEOUT: Duration = Duration::from_secs(8);
const CODEX_RPC_REQUEST_TIMEOUT: Duration = Duration::from_secs(3);
const CODEX_RPC_LAUNCH_COOLDOWN: Duration = Duration::from_mins(30);
const CLAUDE_VERSION_TIMEOUT: Duration = Duration::from_secs(2);
const CLAUDE_CODE_USER_AGENT_FALLBACK: &str = "claude-code/2.1.0";
const GROK_RPC_INIT_TIMEOUT: Duration = Duration::from_secs(8);
const GROK_RPC_REQUEST_TIMEOUT: Duration = Duration::from_secs(12);
const MATERIALIZED_USAGE_ACCOUNTS_PATH: &str = "/jackin/run/usage/accounts.json";
const CODEX_HANDOFF_AUTH_PATH: &str = "/jackin/codex/auth.json";
const AMP_HANDOFF_SECRETS_PATH: &str = "/jackin/amp/secrets.json";
const KIMI_HANDOFF_HOME: &str = "/jackin/kimi-code";
const GROK_HANDOFF_AUTH_PATH: &str = "/jackin/grok/auth.json";
const CLAUDE_HANDOFF_CREDENTIALS_PATH: &str = "/jackin/claude/credentials.json";
pub(crate) const TELEMETRY_STORE_PATH: &str = "/jackin/state/usage/telemetry.db";

static MATERIALIZED_TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone)]
pub(crate) struct UsageCache {
    snapshots: HashMap<String, CachedUsage>,
    codex_rpc_gate: ManagedCliLaunchGate,
    grok_rpc_gate: ManagedCliLaunchGate,
    refresh_schedule: UsageRefreshSchedule,
    telemetry_store_path: PathBuf,
    /// Latched on persistence failure so a persistent fault (e.g. read-only
    /// `/jackin/state`, disk-full, DB corruption) logs once on transition via
    /// always-on `clog!` rather than every 5-minute refresh — and is never
    /// invisible the way the firehose-only `cdebug!` would be in production.
    telemetry_persist_failed: bool,
    accounts_materialize_failed: bool,
}

#[derive(Debug, Clone)]
struct CachedUsage {
    view: FocusedUsageView,
}

struct UsageRefreshResult {
    target: UsageRefreshTarget,
    view: FocusedUsageView,
    codex_rpc_gate: ManagedCliLaunchGate,
    grok_rpc_gate: ManagedCliLaunchGate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UsageRefreshTarget {
    pub(crate) agent: String,
    pub(crate) provider: Option<String>,
}

impl UsageRefreshTarget {
    fn cache_key(&self) -> String {
        canonical_usage_cache_key(&self.agent, self.provider.as_deref())
    }
}

#[derive(Debug, Clone, Default)]
struct UsageRefreshSchedule {
    next_due: HashMap<String, Instant>,
    rate_limit_failures: HashMap<String, u32>,
    in_flight: bool,
}

const USAGE_REFRESH_BASE_INTERVAL: Duration = Duration::from_mins(5);
const USAGE_REFRESH_JITTER: Duration = Duration::from_mins(1);
const USAGE_REFRESH_BACKOFF_CAP: Duration = Duration::from_mins(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UsageSurface {
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
    fn label(self) -> &'static str {
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

    fn account_label(self) -> &'static str {
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
    fn synonyms(self) -> &'static [&'static str] {
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
    #[cfg(test)]
    pub(crate) fn set_telemetry_store_path(&mut self, path: PathBuf) {
        self.telemetry_store_path = path;
    }

    #[cfg(test)]
    pub(crate) fn insert_snapshot_for_test(
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

    pub(crate) fn focused_status_bar_label(
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

    pub(crate) fn account_snapshot_views(&self) -> Vec<AccountUsageSnapshotView> {
        account_snapshot_views_from_cache(&self.snapshots)
    }

    pub(crate) fn focused_snapshot(
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

    fn cached_focused_usage_view(
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

    pub(crate) fn refresh_active_account_snapshots(
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
        let due_targets = targets
            .into_iter()
            .filter(|target| self.refresh_schedule.should_refresh(target, now))
            .collect::<Vec<_>>();
        if due_targets.is_empty() {
            self.refresh_schedule.in_flight = false;
            return;
        }
        let codex_rpc_gate = self.codex_rpc_gate.clone();
        let grok_rpc_gate = self.grok_rpc_gate.clone();
        let provider_keys = provider_keys.clone();
        let results = collect_usage_refresh_results(due_targets, move |target| {
            let mut codex_rpc_gate = codex_rpc_gate.clone();
            let mut grok_rpc_gate = grok_rpc_gate.clone();
            let view = build_snapshot(
                &target.agent,
                target.provider.as_deref(),
                &provider_keys,
                &mut codex_rpc_gate,
                &mut grok_rpc_gate,
            );
            UsageRefreshResult {
                target,
                view,
                codex_rpc_gate,
                grok_rpc_gate,
            }
        });
        let mut stored_views = Vec::new();
        for result in results {
            let UsageRefreshResult {
                target,
                mut view,
                codex_rpc_gate,
                grok_rpc_gate,
            } = result;
            let cache_key = canonical_usage_cache_key(&target.agent, target.provider.as_deref());
            if let Some(cached) = self.snapshots.get(&cache_key) {
                preserve_cached_quota_on_failed_refresh(&mut view, &cached.view);
            }
            enrich_provider_tabs(&mut view, &self.snapshots);
            self.snapshots
                .insert(cache_key.clone(), CachedUsage { view: view.clone() });
            match resolve_surface(&target.agent, target.provider.as_deref()) {
                UsageSurface::Codex => self.codex_rpc_gate = codex_rpc_gate,
                UsageSurface::Grok => self.grok_rpc_gate = grok_rpc_gate,
                _ => {}
            }
            self.refresh_schedule.mark_refreshed(&target, now, &view);
            stored_views.push(view);
        }
        if !stored_views.is_empty() {
            let result = crate::telemetry_store::store_usage_snapshots(
                &self.telemetry_store_path,
                &stored_views,
            );
            self.telemetry_persist_failed = log_persist_transition(
                "usage telemetry store write",
                self.telemetry_persist_failed,
                result,
            );
        }
        let materialize = self.materialize_accounts(now_epoch());
        self.accounts_materialize_failed = log_persist_transition(
            "usage accounts materialization",
            self.accounts_materialize_failed,
            materialize,
        );
        self.refresh_schedule.in_flight = false;
    }

    pub(crate) fn request_account_refresh(&mut self, target: &UsageRefreshTarget, now: Instant) {
        self.refresh_schedule.mark_due(target, now);
    }

    fn materialize_accounts(&self, generated_at_epoch: i64) -> Result<(), String> {
        let snapshots = self
            .snapshots
            .values()
            .map(|cached| cached.view.clone())
            .collect::<Vec<_>>();
        write_materialized_usage_accounts(
            Path::new(MATERIALIZED_USAGE_ACCOUNTS_PATH),
            generated_at_epoch,
            snapshots,
        )
    }
}

fn collect_usage_refresh_results<F>(
    due_targets: Vec<UsageRefreshTarget>,
    probe: F,
) -> Vec<UsageRefreshResult>
where
    F: Fn(UsageRefreshTarget) -> UsageRefreshResult + Send + Sync + 'static,
{
    collect_usage_refresh_results_with_timeout(due_targets, probe, PROVIDER_PROBE_TIMEOUT)
}

fn collect_usage_refresh_results_with_timeout<F>(
    due_targets: Vec<UsageRefreshTarget>,
    probe: F,
    timeout: Duration,
) -> Vec<UsageRefreshResult>
where
    F: Fn(UsageRefreshTarget) -> UsageRefreshResult + Send + Sync + 'static,
{
    let probe = Arc::new(probe);
    let (tx, rx) = mpsc::channel();
    let mut pending = due_targets
        .iter()
        .map(UsageRefreshTarget::cache_key)
        .collect::<std::collections::HashSet<_>>();
    let fallback_targets = due_targets
        .iter()
        .map(|target| (target.cache_key(), target.clone()))
        .collect::<HashMap<_, _>>();
    let expected = due_targets.len();
    for target in due_targets {
        let tx = tx.clone();
        let probe = Arc::clone(&probe);
        thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| probe(target)));
            match result {
                Ok(result) => {
                    drop(tx.send(result));
                }
                Err(_) => {
                    crate::clog!("usage-refresh: provider probe panicked");
                }
            }
        });
    }
    drop(tx);

    let started = Instant::now();
    let mut results = Vec::new();
    while results.len() < expected {
        let Some(remaining) = timeout.checked_sub(started.elapsed()) else {
            break;
        };
        if remaining.is_zero() {
            break;
        }
        match rx.recv_timeout(remaining) {
            Ok(result) => {
                pending.remove(&result.target.cache_key());
                results.push(result);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => break,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    if !pending.is_empty() {
        let now = now_epoch();
        for key in pending {
            let Some(target) = fallback_targets.get(&key).cloned() else {
                continue;
            };
            crate::clog!(
                "usage-refresh: provider probe timed out for {}",
                target.cache_key()
            );
            let mut view = cached_unavailable_view(&target.agent, target.provider.as_deref(), now);
            view.last_error = Some("usage provider probe timed out".to_owned());
            results.push(UsageRefreshResult {
                target,
                view,
                codex_rpc_gate: ManagedCliLaunchGate::default(),
                grok_rpc_gate: ManagedCliLaunchGate::default(),
            });
        }
    }
    results
}

/// Log a persistence outcome at the right tier: always-on `clog!` once when a
/// fault starts and once when it clears, plus a per-cycle `cdebug!` firehose
/// line while it persists. Returns the new "failed" latch for the caller to store.
fn log_persist_transition(what: &str, was_failed: bool, result: Result<(), String>) -> bool {
    match result {
        Ok(()) => {
            if was_failed {
                crate::clog!("{what} recovered");
            }
            false
        }
        Err(error) => {
            if !was_failed {
                crate::clog!("{what} failed (suppressing repeats until recovery): {error}");
            }
            crate::cdebug!("{what} failed: {error}");
            true
        }
    }
}

impl Default for UsageCache {
    fn default() -> Self {
        Self {
            snapshots: HashMap::new(),
            codex_rpc_gate: ManagedCliLaunchGate::default(),
            grok_rpc_gate: ManagedCliLaunchGate::default(),
            refresh_schedule: UsageRefreshSchedule::default(),
            telemetry_store_path: PathBuf::from(TELEMETRY_STORE_PATH),
            telemetry_persist_failed: false,
            accounts_materialize_failed: false,
        }
    }
}

impl UsageRefreshSchedule {
    fn mark_due(&mut self, target: &UsageRefreshTarget, now: Instant) {
        self.next_due.insert(target.cache_key(), now);
    }

    fn should_refresh(&mut self, target: &UsageRefreshTarget, now: Instant) -> bool {
        self.should_refresh_with_cooldown_dir(target, now, &shared_usage_cooldown_dir())
    }

    fn should_refresh_with_cooldown_dir(
        &mut self,
        target: &UsageRefreshTarget,
        now: Instant,
        cooldown_dir: &Path,
    ) -> bool {
        let key = target.cache_key();
        if shared_usage_cooldown_active(cooldown_dir, &key, now_epoch()) {
            return false;
        }
        match self.next_due.get(&key).copied() {
            Some(due) if due > now => false,
            Some(_) => true,
            None => {
                self.next_due.insert(key, now);
                true
            }
        }
    }

    fn mark_refreshed(
        &mut self,
        target: &UsageRefreshTarget,
        now: Instant,
        view: &FocusedUsageView,
    ) {
        self.mark_refreshed_with_cooldown_dir(target, now, view, &shared_usage_cooldown_dir());
    }

    fn mark_refreshed_with_cooldown_dir(
        &mut self,
        target: &UsageRefreshTarget,
        now: Instant,
        view: &FocusedUsageView,
        cooldown_dir: &Path,
    ) {
        let key = target.cache_key();
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
            write_shared_usage_cooldown_marker(cooldown_dir, &key, until_epoch, error);
            self.next_due.insert(key, now + delay);
        } else {
            self.rate_limit_failures.remove(&key);
            self.next_due
                .insert(key.clone(), now + refresh_interval_for_key(&key));
        }
    }
}

fn ordered_refresh_targets(
    active_targets: &[UsageRefreshTarget],
    focused: Option<UsageRefreshTarget>,
) -> Vec<UsageRefreshTarget> {
    let mut seen = std::collections::HashSet::new();
    let mut targets = Vec::new();
    if let Some(target) = focused
        && seen.insert(target.cache_key())
    {
        targets.push(target);
    }
    for target in active_targets {
        if seen.insert(target.cache_key()) {
            targets.push(target.clone());
        }
    }
    targets
}

fn refresh_interval_for_key(key: &str) -> Duration {
    let jitter_span = USAGE_REFRESH_JITTER.as_secs().saturating_mul(2);
    let hash = stable_usage_hash(key);
    let offset = hash % (jitter_span.saturating_add(1));
    let min = USAGE_REFRESH_BASE_INTERVAL.saturating_sub(USAGE_REFRESH_JITTER);
    min + Duration::from_secs(offset)
}

fn stable_usage_hash(value: &str) -> u64 {
    value.bytes().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}

fn shared_usage_cooldown_dir() -> PathBuf {
    std::env::var("JACKIN_USAGE_COOLDOWN_DIR").map_or_else(
        |_| home_path(".jackin/data/daemon/usage-cooldowns"),
        PathBuf::from,
    )
}

fn shared_usage_cooldown_marker_path(cooldown_dir: &Path, key: &str) -> PathBuf {
    cooldown_dir.join(format!("usage-{:016x}.cooldown", stable_usage_hash(key)))
}

fn shared_usage_cooldown_active(cooldown_dir: &Path, key: &str, now_epoch: i64) -> bool {
    let path = shared_usage_cooldown_marker_path(cooldown_dir, key);
    let Ok(text) = fs::read_to_string(path) else {
        return false;
    };
    let Some(first) = text.lines().next() else {
        return false;
    };
    first
        .trim()
        .parse::<i64>()
        .is_ok_and(|until_epoch| until_epoch > now_epoch)
}

fn write_shared_usage_cooldown_marker(
    cooldown_dir: &Path,
    key: &str,
    until_epoch: i64,
    reason: &str,
) {
    if let Err(error) = fs::create_dir_all(cooldown_dir) {
        crate::clog!("usage cooldown marker dir create failed for {key}: {error}");
        return;
    }
    let path = shared_usage_cooldown_marker_path(cooldown_dir, key);
    let reason = reason.replace('\n', " ");
    // A dropped marker means the provider gets re-probed inside its backoff
    // window, so surface the failure rather than silently defeating the 429
    // cooldown.
    if let Err(error) = fs::write(path, format!("{until_epoch}\n{reason}\n")) {
        crate::clog!("usage cooldown marker write failed for {key}: {error}");
    }
}

fn usage_error_is_rate_limited(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("429")
        || lower.contains("rate limit")
        || lower.contains("retry-after")
        || lower.contains("retry after")
}

fn usage_rate_limit_delay(error: &str, failures: u32) -> Duration {
    let lower = error.to_ascii_lowercase();
    parse_retry_after_seconds(&lower)
        .map_or_else(
            || usage_backoff_delay(USAGE_REFRESH_BASE_INTERVAL, failures),
            Duration::from_secs,
        )
        .min(USAGE_REFRESH_BACKOFF_CAP)
}

fn parse_retry_after_seconds(error: &str) -> Option<u64> {
    for marker in ["retry-after", "retry after"] {
        let Some((_, tail)) = error.split_once(marker) else {
            continue;
        };
        let digits = tail
            .chars()
            .skip_while(|ch| !ch.is_ascii_digit())
            .take_while(char::is_ascii_digit)
            .collect::<String>();
        if let Ok(seconds) = digits.parse::<u64>() {
            return Some(seconds);
        }
    }
    None
}

fn usage_backoff_delay(base: Duration, failures: u32) -> Duration {
    let shift = failures.saturating_sub(1).min(8);
    let multiplier = 1u64.checked_shl(shift).unwrap_or(u64::MAX);
    Duration::from_secs(base.as_secs().saturating_mul(multiplier)).min(USAGE_REFRESH_BACKOFF_CAP)
}

fn canonical_usage_cache_key(agent: &str, focused_provider: Option<&str>) -> String {
    let surface = resolve_surface(agent, focused_provider);
    if surface == UsageSurface::Unsupported {
        return format!("{agent}:{}", focused_provider.unwrap_or_default());
    }
    surface.label().to_owned()
}

#[cfg(test)]
pub(crate) fn resolved_usage_provider_label(
    agent: &str,
    focused_provider: Option<&str>,
) -> Option<&'static str> {
    let surface = resolve_surface(agent, focused_provider);
    (surface != UsageSurface::Unsupported).then_some(surface.label())
}

/// Stamp the surface-derived agent, provider label, and tab strip onto a base
/// placeholder view, so a `unavailable`/`refreshing` view still shows the proper
/// header (e.g. `Anthropic / Claude`) and tabs while it loads.
fn decorate_surface_view(
    view: &mut FocusedUsageView,
    agent: &str,
    focused_provider: Option<&str>,
    surface: UsageSurface,
) {
    view.focused_agent = Some(agent.to_owned());
    view.focused_provider = focused_provider
        .map(str::to_owned)
        .or_else(|| Some(surface.label().to_owned()));
    view.account.provider_label = surface.account_label().to_owned();
    view.tabs = provider_tabs(surface);
}

fn cached_unavailable_view(
    agent: &str,
    focused_provider: Option<&str>,
    now: i64,
) -> FocusedUsageView {
    let surface = resolve_surface(agent, focused_provider);
    let mut view =
        FocusedUsageView::unavailable("usage unavailable: no cached provider snapshot", now);
    decorate_surface_view(&mut view, agent, focused_provider, surface);
    view
}

fn cached_refreshing_view(
    agent: &str,
    focused_provider: Option<&str>,
    now: i64,
) -> FocusedUsageView {
    let surface = resolve_surface(agent, focused_provider);
    let mut view = FocusedUsageView::refreshing(focused_provider, now);
    decorate_surface_view(&mut view, agent, focused_provider, surface);
    view
}

fn mark_active_tab(view: &mut FocusedUsageView) {
    let provider = view.focused_provider.as_deref().unwrap_or_default();
    for tab in &mut view.tabs {
        tab.active = provider_matches_usage_label(&tab.label, provider)
            || provider_matches_usage_label(&tab.label, &view.account.provider_label);
    }
}

fn account_snapshot_views_from_cache(
    snapshots: &HashMap<String, CachedUsage>,
) -> Vec<AccountUsageSnapshotView> {
    let mut accounts = snapshots
        .values()
        .flat_map(|cached| {
            let view = &cached.view;
            view.buckets.iter().map(|bucket| {
                let (used_amount, used_unit, limit_amount, limit_unit) =
                    quota_amounts_for_account_snapshot(bucket);
                AccountUsageSnapshotView {
                    provider: view.account.provider_label.clone(),
                    account_label: view.account.account_label.clone(),
                    source: usage_source_storage_label(view.source).to_owned(),
                    confidence: usage_confidence_storage_label(view.confidence).to_owned(),
                    window_kind: bucket.label.clone(),
                    used_amount,
                    used_unit,
                    limit_amount,
                    limit_unit,
                    resets_at: bucket.resets_at,
                    fetched_at: view.fetched_at_epoch,
                    expires_at: None,
                    status: usage_status_storage_label(bucket.status).to_owned(),
                    last_error: view.last_error.clone(),
                }
            })
        })
        .collect::<Vec<_>>();
    accounts.sort_by(|left, right| {
        left.provider
            .cmp(&right.provider)
            .then(left.window_kind.cmp(&right.window_kind))
    });
    accounts
}

fn quota_amounts_for_account_snapshot(
    bucket: &QuotaBucketView,
) -> (Option<i64>, Option<String>, Option<i64>, Option<String>) {
    let Some(remaining) = bucket.remaining_percent else {
        return (None, None, None, None);
    };
    (
        Some(i64::from(100_u8.saturating_sub(remaining.min(100)))),
        Some("percent".to_owned()),
        Some(100),
        Some("percent".to_owned()),
    )
}

pub(crate) fn usage_status_storage_label(status: UsageSnapshotStatus) -> &'static str {
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

pub(crate) fn usage_source_storage_label(source: UsageSource) -> &'static str {
    match source {
        UsageSource::ProviderApi => "provider_api",
        UsageSource::Cli => "cli",
        UsageSource::LocalLogs => "local_logs",
        UsageSource::Cache => "cache",
        UsageSource::None => "none",
    }
}

pub(crate) fn usage_confidence_storage_label(confidence: UsageConfidence) -> &'static str {
    match confidence {
        UsageConfidence::Authoritative => "authoritative",
        UsageConfidence::Estimated => "estimated",
        UsageConfidence::PresenceOnly => "presence_only",
        UsageConfidence::None => "none",
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct MaterializedUsageAccounts {
    generated_at_epoch: i64,
    snapshots: Vec<FocusedUsageView>,
}

fn write_materialized_usage_accounts(
    path: &Path,
    generated_at_epoch: i64,
    snapshots: Vec<FocusedUsageView>,
) -> Result<(), String> {
    let document = MaterializedUsageAccounts {
        generated_at_epoch,
        snapshots,
    };
    let contents = serde_json::to_string_pretty(&document)
        .map_err(|err| format!("usage accounts encode failed: {err}"))?;
    atomic_write_usage_json(path, &contents)
}

#[allow(clippy::disallowed_methods)]
fn atomic_write_usage_json(path: &Path, contents: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("create usage materialization dir failed: {err}"))?;
    }
    let counter = MATERIALIZED_TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut staged_name = path
        .file_name()
        .map(std::ffi::OsStr::to_os_string)
        .unwrap_or_default();
    staged_name.push(format!(".tmp.{}.{counter}", std::process::id()));
    let tmp = path.with_file_name(staged_name);
    let staged = (|| -> Result<(), String> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o644)
                .open(&tmp)
                .map_err(|err| format!("open staged usage accounts failed: {err}"))?;
            file.write_all(contents.as_bytes())
                .map_err(|err| format!("write staged usage accounts failed: {err}"))?;
            file.sync_all()
                .map_err(|err| format!("sync staged usage accounts failed: {err}"))?;
        }

        #[cfg(not(unix))]
        fs::write(&tmp, contents)
            .map_err(|err| format!("write staged usage accounts failed: {err}"))?;

        Ok(())
    })();
    if let Err(error) = staged {
        drop(fs::remove_file(&tmp));
        return Err(error);
    }
    if let Err(error) = fs::rename(&tmp, path) {
        drop(fs::remove_file(&tmp));
        return Err(format!("rename usage accounts into place failed: {error}"));
    }
    Ok(())
}

fn build_snapshot(
    agent: &str,
    provider: Option<&str>,
    provider_keys: &BTreeMap<jackin_protocol::Provider, String>,
    codex_rpc_gate: &mut ManagedCliLaunchGate,
    grok_rpc_gate: &mut ManagedCliLaunchGate,
) -> FocusedUsageView {
    let surface = resolve_surface(agent, provider);
    let now = now_epoch();
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

fn resolve_surface(agent: &str, provider: Option<&str>) -> UsageSurface {
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
fn split_fetch<U>(result: Option<Result<U, String>>) -> (Option<U>, Option<String>) {
    match result {
        Some(Ok(value)) => (Some(value), None),
        Some(Err(error)) => (None, Some(error)),
        None => (None, None),
    }
}

/// Inputs to [`provider_outcome`]. Named fields so the two booleans can't be
/// silently swapped at a call site.
struct ProviderPresence {
    has_data: bool,
    has_secret: bool,
}

/// Lifecycle triad for the simple "API key or nothing" providers: data present →
/// fresh/authoritative; a secret present but no data → unsupported/presence-only;
/// neither → needs-secret. Providers with login/CLI/error nuances (Claude, Codex,
/// Amp, Grok) keep their bespoke logic.
fn provider_outcome(
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

fn claude_snapshot(agent: &str, provider: Option<&str>, now: i64) -> FocusedUsageView {
    let config =
        std::env::var("CLAUDE_CONFIG_DIR").map_or_else(|_| home_path(".claude"), PathBuf::from);
    // Resolve the Claude OAuth token, home credentials first (the agent CLI
    // keeps the live token there and refreshes it in place). `~/.claude.json`
    // only carries `oauthAccount` metadata, never the token. The runtime-
    // forwarded handoff at `/jackin/claude/credentials.json` is the last-resort
    // fallback — mirroring the other providers (Codex/Amp/Kimi/Grok) — so the
    // snapshot does not silently drop to the impoverished CLI path when the
    // home copy lacks `claudeAiOauth.accessToken`. Matches CodexBar's order.
    let oauth_candidates = [
        config.join(".credentials.json"),
        home_path(".claude/.credentials.json"),
        home_path(".claude.json"),
        PathBuf::from(CLAUDE_HANDOFF_CREDENTIALS_PATH),
    ];
    // One home-first walk yields both the OAuth token (with its winning path, for
    // the `Auth:` origin — there is no keychain reader in the capsule, so the
    // origin names the file) and the `oauthAccount` email, reading each file
    // once. account_label is the real email identity — empty when none, never a
    // fabricated auth-method string; the auth source lives on `credential_origin`.
    let (oauth_resolved, account_email) = resolve_identity(
        &oauth_candidates,
        claude_oauth_from_value,
        claude_email_from_value,
    );
    let (oauth_path, oauth) = oauth_resolved.unzip();
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .ok()
        .filter(|v| !v.is_empty());
    let auth_token = std::env::var("ANTHROPIC_AUTH_TOKEN")
        .ok()
        .filter(|v| !v.is_empty());
    let has_local_creds = config.join(".credentials.json").exists();
    let needs_login =
        api_key.is_none() && auth_token.is_none() && oauth.is_none() && !has_local_creds;
    let account = account_email.unwrap_or_default();
    // The displayed numbers come from the OAuth file token (the env keys are
    // never used for the fetch), so name the OAuth path that won first; fall
    // back to the env token only when no OAuth credential resolved.
    let credential_origin = if let Some(path) = oauth_path.as_deref() {
        Some(oauth_origin(path))
    } else if api_key.is_some() {
        Some("API token · env ANTHROPIC_API_KEY".to_owned())
    } else if auth_token.is_some() {
        Some("API token · env ANTHROPIC_AUTH_TOKEN".to_owned())
    } else {
        None
    };
    let (oauth_quota, oauth_error) = split_fetch(
        oauth
            .as_ref()
            .map(|credentials| fetch_claude_oauth_usage(&credentials.access_token)),
    );
    let (cli_usage, cli_error) =
        split_fetch((oauth_quota.is_none() && oauth.is_some()).then(fetch_claude_cli_usage));
    let provider_error = if oauth_quota.is_some() || cli_usage.is_some() {
        None
    } else {
        oauth_error.as_ref().or(cli_error.as_ref()).cloned()
    };
    let status = if needs_login {
        UsageSnapshotStatus::NeedsLogin
    } else if oauth_quota.is_some() || cli_usage.is_some() {
        UsageSnapshotStatus::Fresh
    } else {
        UsageSnapshotStatus::Stale
    };
    let bucket_status = status;
    let buckets = oauth_quota
        .map(|usage| usage.into_buckets(now))
        .or_else(|| cli_usage.as_ref().map(ClaudeCliUsage::buckets))
        .filter(|buckets| !buckets.is_empty())
        .unwrap_or_else(|| {
            vec![
                bucket(
                    "Session",
                    None,
                    None,
                    None,
                    None,
                    provider_error.as_deref().or(Some("provider API pending")),
                    bucket_status,
                ),
                bucket(
                    "Weekly",
                    None,
                    None,
                    None,
                    None,
                    provider_error.as_deref().or(Some("provider API pending")),
                    bucket_status,
                ),
                bucket(
                    "Daily Routines",
                    None,
                    None,
                    None,
                    None,
                    provider_error.as_deref().or(Some("provider API pending")),
                    bucket_status,
                ),
            ]
        });
    usage_view(UsageViewInput {
        agent,
        provider,
        surface: UsageSurface::Claude,
        account_label: account,
        username: None,
        plan_label: oauth.and_then(|credentials| credentials.subscription_type),
        credential_origin,
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
            // Class fix (P0): the rich OAuth snapshot is authoritative; the CLI
            // fallback is a reduced snapshot and must be marked degraded
            // (Estimated) with a reason, never presented as authoritative.
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
            UsageSnapshotStatus::NeedsLogin => {
                Some("Claude credentials not available to Capsule".to_owned())
            }
            UsageSnapshotStatus::Stale => Some(provider_error.unwrap_or_else(|| {
                "Claude provider usage unavailable; cached quota is stale".to_owned()
            })),
            // Degraded-but-fresh: the reduced CLI snapshot is showing because
            // the OAuth fetch failed — surface why, not a confident silence.
            _ if cli_usage.is_some() => Some(oauth_error.clone().unwrap_or_else(|| {
                "Claude OAuth usage unavailable; showing reduced CLI snapshot".to_owned()
            })),
            _ => None,
        },
    })
}

/// Map a Codex/`ChatGPT` `plan_type` to its display name, mirroring `CodexBar`'s
/// `CodexPlanFormatting.displayName` (F7a): `pro` → `Pro 20x`, the pro-lite
/// variants → `Pro 5x`, machine identifiers humanized (`enterprise_cbp_usage_based`
/// → `Enterprise CBP Usage Based`), already-readable text preserved. Returns
/// `None` for blank input so an unknown plan is omitted, never shown as `pro`.
fn codex_plan_display_name(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(exact) = codex_plan_exact_display(trimmed) {
        return Some(exact);
    }
    // Strip boilerplate words (claude/codex/account/plan) the way CodexBar's
    // `UsageFormatter.cleanPlanName` does, then re-check the exact map.
    let cleaned = trimmed
        .split_whitespace()
        .filter(|word| {
            !matches!(
                word.to_ascii_lowercase().as_str(),
                "claude" | "codex" | "account" | "plan"
            )
        })
        .collect::<Vec<_>>()
        .join(" ");
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        return Some(trimmed.to_owned());
    }
    if let Some(exact) = codex_plan_exact_display(cleaned) {
        return Some(exact);
    }
    let formatted = humanize_words_with(cleaned, codex_plan_word_display);
    if formatted.is_empty() {
        return Some(cleaned.to_owned());
    }
    Some(formatted)
}

fn codex_plan_exact_display(value: &str) -> Option<String> {
    match value.to_ascii_lowercase().as_str() {
        "pro" => Some("Pro 20x".to_owned()),
        "prolite" | "pro_lite" | "pro-lite" | "pro lite" => Some("Pro 5x".to_owned()),
        _ => None,
    }
}

fn codex_plan_word_display(raw: &str) -> String {
    let lower = raw.to_ascii_lowercase();
    if matches!(lower.as_str(), "cbp" | "k12") {
        return lower.to_ascii_uppercase();
    }
    // Preserve existing acronyms (all-caps with a letter, e.g. "AI").
    if raw == raw.to_ascii_uppercase() && raw.chars().any(char::is_alphabetic) {
        return raw.to_owned();
    }
    titlecase_ascii(raw)
}

fn codex_snapshot(
    agent: &str,
    provider: Option<&str>,
    now: i64,
    rpc_gate: &mut ManagedCliLaunchGate,
) -> FocusedUsageView {
    let codex_home =
        std::env::var("CODEX_HOME").map_or_else(|_| home_path(".codex"), PathBuf::from);
    let auth_path = codex_home.join("auth.json");
    let handoff_auth_path = Path::new(CODEX_HANDOFF_AUTH_PATH);
    // Home auth first, runtime-forwarded handoff last; one walk yields the
    // credential (with its winning path, for the `Auth:` origin) and the account
    // label, reading each file once.
    let codex_candidates = [auth_path, handoff_auth_path.to_path_buf()];
    let (resolved, account_from_file) = resolve_identity(
        &codex_candidates,
        codex_oauth_from_value,
        codex_account_from_value,
    );
    let (oauth_path, credentials) = resolved.unzip();
    // account_label is the email identity only; the auth source (the resolver
    // arm that actually won) goes on `credential_origin`.
    let auth_email = credentials
        .as_ref()
        .and_then(|credentials| credentials.account_label.clone())
        .or(account_from_file);
    let has_env_key = std::env::var("OPENAI_API_KEY").is_ok_and(|v| !v.is_empty());
    let needs_login = credentials.is_none() && auth_email.is_none() && !has_env_key;
    let credential_origin = if let Some(path) = oauth_path.as_deref() {
        Some(oauth_origin(path))
    } else if has_env_key {
        Some("API token · env OPENAI_API_KEY".to_owned())
    } else {
        None
    };
    let (rpc_usage, rpc_error) = match fetch_codex_rpc_usage(rpc_gate) {
        Ok(usage) => (Some(usage), None),
        Err(error) => (None, Some(error)),
    };
    let rpc_quota = rpc_usage.as_ref().map(|usage| &usage.response);
    let (oauth_quota, oauth_error) = split_fetch(credentials.as_ref().map(|credentials| {
        fetch_codex_oauth_usage(credentials, &codex_home).map(|mut usage| {
            usage.reset_credits = fetch_codex_oauth_reset_credits(credentials, &codex_home)
                .inspect_err(|error| {
                    crate::cdebug!("codex reset-credits fetch failed: {error}");
                })
                .ok();
            usage
        })
    }));
    let provider_error = rpc_error.as_ref().or(oauth_error.as_ref()).cloned();
    let quota = rpc_quota.or(oauth_quota.as_ref());
    let account = rpc_usage
        .as_ref()
        .and_then(|usage| usage.account_label.clone())
        .or(auth_email)
        .unwrap_or_default();
    let status = if needs_login {
        UsageSnapshotStatus::NeedsLogin
    } else if quota.is_some() {
        UsageSnapshotStatus::Fresh
    } else {
        UsageSnapshotStatus::Stale
    };
    let buckets = quota
        .map(|usage| usage.buckets(now))
        .filter(|buckets| !buckets.is_empty())
        .unwrap_or_else(|| {
            vec![
                bucket(
                    "Session",
                    None,
                    None,
                    None,
                    None,
                    provider_error
                        .as_deref()
                        .or(Some("app-server/OAuth quota pending")),
                    UsageSnapshotStatus::Unsupported,
                ),
                bucket(
                    "Weekly",
                    None,
                    None,
                    None,
                    None,
                    provider_error
                        .as_deref()
                        .or(Some("app-server/OAuth quota pending")),
                    UsageSnapshotStatus::Unsupported,
                ),
                bucket(
                    "Codex Spark 5-hour",
                    None,
                    None,
                    None,
                    None,
                    provider_error.as_deref().or(Some("provider API pending")),
                    UsageSnapshotStatus::Unsupported,
                ),
                bucket(
                    "Codex Spark Weekly",
                    None,
                    None,
                    None,
                    None,
                    provider_error.as_deref().or(Some("provider API pending")),
                    UsageSnapshotStatus::Unsupported,
                ),
            ]
        });
    usage_view(UsageViewInput {
        agent,
        provider,
        surface: UsageSurface::Codex,
        account_label: account,
        username: None,
        plan_label: quota
            .and_then(|usage| usage.plan_type.as_deref())
            .and_then(codex_plan_display_name),
        credential_origin,
        buckets,
        status,
        source: if status == UsageSnapshotStatus::Fresh {
            if rpc_quota.is_some() {
                UsageSource::Cli
            } else {
                UsageSource::ProviderApi
            }
        } else {
            UsageSource::None
        },
        confidence: if status == UsageSnapshotStatus::Fresh {
            UsageConfidence::Authoritative
        } else {
            UsageConfidence::None
        },
        now,
        last_error: match status {
            UsageSnapshotStatus::NeedsLogin => {
                Some("Codex auth not available to Capsule".to_owned())
            }
            UsageSnapshotStatus::Stale => Some(provider_error.unwrap_or_else(|| {
                "Codex provider usage unavailable; cached quota is stale".to_owned()
            })),
            _ => None,
        },
    })
}

fn amp_snapshot(agent: &str, now: i64) -> FocusedUsageView {
    let data = home_path(".local/share/amp");
    let amp_secrets = data.join("secrets.json");
    let handoff_secrets = Path::new(AMP_HANDOFF_SECRETS_PATH);
    let amp_env_key = env_value("AMP_API_KEY");
    let env_present = amp_env_key.is_some();
    // Resolve the file key only when the env var is absent (env wins), capturing
    // the winning path so the origin names the file that actually produced the
    // key instead of re-`stat`ing and guessing. Home secrets first, handoff last.
    let amp_file = if env_present {
        None
    } else {
        first_credential_with_path(
            &[amp_secrets.clone(), handoff_secrets.to_path_buf()],
            load_amp_api_key,
        )
    };
    let amp_api_key = amp_env_key
        .clone()
        .or_else(|| amp_file.as_ref().map(|(_, key)| key.clone()));
    let (api_usage, api_error) = split_fetch(amp_api_key.as_deref().map(fetch_amp_api_usage));
    let (cli_usage, cli_error) = split_fetch(api_usage.is_none().then(fetch_amp_cli_usage));
    let provider_error = api_error.as_ref().or(cli_error.as_ref()).cloned();
    let has_auth = amp_api_key.is_some() || amp_secrets.exists() || handoff_secrets.exists();
    // credential_origin names the file that actually produced the key (env wins
    // first). A present-but-unparseable home `secrets.json` no longer mislabels a
    // key that actually resolved from the handoff.
    let credential_origin = if env_present {
        Some("API key · env AMP_API_KEY".to_owned())
    } else if let Some((path, _)) = amp_file.as_ref() {
        Some(if path.as_path() == handoff_secrets {
            format!("API key · {AMP_HANDOFF_SECRETS_PATH}")
        } else {
            "API key · amp secrets.json".to_owned()
        })
    } else {
        None
    };
    let status = if api_usage.is_some() || cli_usage.is_some() {
        UsageSnapshotStatus::Fresh
    } else if has_auth {
        UsageSnapshotStatus::Unsupported
    } else {
        UsageSnapshotStatus::NeedsLogin
    };
    let account_label = api_usage
        .as_ref()
        .and_then(|usage| usage.account_label.clone())
        .or_else(|| {
            cli_usage
                .as_ref()
                .and_then(|usage| usage.account_label.clone())
        })
        .unwrap_or_else(|| {
            if has_auth {
                "local Amp auth".to_owned()
            } else {
                "needs Amp login".to_owned()
            }
        });
    let buckets = api_usage
        .as_ref()
        .map(|usage| usage.buckets(now))
        .or_else(|| cli_usage.as_ref().map(|usage| usage.buckets(now)))
        .filter(|buckets| !buckets.is_empty())
        .unwrap_or_else(|| {
            vec![bucket(
                "Amp Free",
                None,
                None,
                None,
                None,
                provider_error
                    .as_deref()
                    .or(Some("Amp API/CLI usage unavailable")),
                status,
            )]
        });
    usage_view(UsageViewInput {
        agent,
        provider: None,
        surface: UsageSurface::Amp,
        account_label,
        username: None,
        plan_label: (api_usage.is_some() || cli_usage.is_some()).then_some("Amp Free".to_owned()),
        credential_origin,
        buckets,
        status,
        source: if api_usage.is_some() {
            UsageSource::ProviderApi
        } else if cli_usage.is_some() {
            UsageSource::Cli
        } else {
            UsageSource::None
        },
        confidence: if api_usage.is_some() || cli_usage.is_some() {
            UsageConfidence::Authoritative
        } else if has_auth {
            UsageConfidence::PresenceOnly
        } else {
            UsageConfidence::None
        },
        now,
        last_error: match status {
            UsageSnapshotStatus::NeedsLogin => Some("Amp auth not available to Capsule".to_owned()),
            UsageSnapshotStatus::Unsupported => Some(
                provider_error
                    .unwrap_or_else(|| "Amp API/CLI usage unavailable to Capsule".to_owned()),
            ),
            _ => None,
        },
    })
}

fn grok_snapshot(agent: &str, now: i64, rpc_gate: &mut ManagedCliLaunchGate) -> FocusedUsageView {
    let data = home_path(".grok");
    let home_auth = data.join("auth.json");
    let handoff_auth = PathBuf::from(GROK_HANDOFF_AUTH_PATH);
    let home_exists = home_auth.exists();
    let auth = if home_exists { home_auth } else { handoff_auth };
    // `home_exists` short-circuits when home won, so the resolved path is
    // checked at most once.
    let has_auth = home_exists || auth.exists();
    let has_xai_api_key = env_value("XAI_API_KEY").is_some();
    let has_deployment_key = env_value("GROK_DEPLOYMENT_KEY").is_some();
    let billing_result = fetch_grok_billing(&auth, now, rpc_gate);
    grok_snapshot_from_rpc_result(
        agent,
        now,
        &auth,
        has_auth,
        has_xai_api_key,
        has_deployment_key,
        billing_result,
    )
}

fn grok_snapshot_from_rpc_result(
    agent: &str,
    now: i64,
    auth: &Path,
    has_auth: bool,
    has_xai_api_key: bool,
    has_deployment_key: bool,
    billing_result: Result<GrokBillingSnapshot, String>,
) -> FocusedUsageView {
    let has_credentials = has_auth || has_xai_api_key || has_deployment_key;
    let (billing_usage, billing_error) = match billing_result {
        Ok(usage) => (Some(usage), None),
        Err(error) => (None, Some(error)),
    };
    // credential_origin reflects the resolver arm that actually won
    // (`auth` is the resolved path — home `~/.grok/auth.json` or the handoff).
    let credential_origin = if has_auth {
        Some(if auth == Path::new(GROK_HANDOFF_AUTH_PATH) {
            format!("OAuth · {GROK_HANDOFF_AUTH_PATH}")
        } else {
            "OAuth · ~/.grok/auth.json".to_owned()
        })
    } else if has_xai_api_key {
        Some("API token · env XAI_API_KEY".to_owned())
    } else if has_deployment_key {
        Some("API token · env GROK_DEPLOYMENT_KEY".to_owned())
    } else {
        None
    };
    let account =
        grok_account_label_or_presence(auth, has_auth, has_xai_api_key, has_deployment_key);
    let status = if billing_usage.is_some() {
        UsageSnapshotStatus::Fresh
    } else if has_credentials {
        UsageSnapshotStatus::Error
    } else {
        UsageSnapshotStatus::NeedsLogin
    };
    let buckets = billing_usage
        .as_ref()
        .map(|usage| usage.buckets(now))
        .filter(|buckets| !buckets.is_empty())
        .unwrap_or_else(|| {
            vec![bucket(
                "Credits",
                None,
                None,
                None,
                None,
                Some("ACP billing unavailable"),
                status,
            )]
        });
    usage_view(UsageViewInput {
        agent,
        provider: None,
        surface: UsageSurface::Grok,
        account_label: account,
        username: None,
        plan_label: grok_plan_label(auth),
        credential_origin,
        buckets,
        status,
        source: billing_usage
            .as_ref()
            .map_or(UsageSource::None, GrokBillingSnapshot::source),
        confidence: if billing_usage.is_some() {
            UsageConfidence::Authoritative
        } else if has_credentials {
            UsageConfidence::PresenceOnly
        } else {
            UsageConfidence::None
        },
        now,
        last_error: match status {
            UsageSnapshotStatus::NeedsLogin => Some(
                billing_error.unwrap_or_else(|| "Grok auth not available to Capsule".to_owned()),
            ),
            UsageSnapshotStatus::Error => {
                billing_error.or_else(|| Some("Grok billing unavailable to Capsule".to_owned()))
            }
            _ => None,
        },
    })
}

fn kimi_snapshot(agent: &str, token: Option<&str>, now: i64) -> FocusedUsageView {
    let has_local = home_path(".kimi-code").exists() || home_path(".kimi").exists();
    let has_token = token.is_some_and(|value| !value.is_empty());
    let (provider_usage, provider_error) = split_fetch(token.map(fetch_kimi_usage));
    let (status, source, confidence) = provider_outcome(ProviderPresence {
        has_data: provider_usage.is_some(),
        has_secret: has_token || has_local,
    });
    let buckets = provider_usage
        .as_ref()
        .map(|usage| usage.buckets(now))
        .filter(|buckets| !buckets.is_empty())
        .unwrap_or_else(|| {
            vec![
                bucket(
                    "Weekly",
                    None,
                    None,
                    None,
                    None,
                    provider_error
                        .as_deref()
                        .or(Some("Kimi billing endpoint unavailable")),
                    status,
                ),
                bucket(
                    "5-hour rate limit",
                    None,
                    None,
                    None,
                    None,
                    provider_error
                        .as_deref()
                        .or(Some("Kimi billing endpoint unavailable")),
                    status,
                ),
            ]
        });
    usage_view(UsageViewInput {
        agent,
        provider: None,
        surface: UsageSurface::Kimi,
        account_label: String::new(),
        username: None,
        plan_label: None,
        credential_origin: Some(
            if has_token {
                "API token · env KIMI_CODE_API_KEY"
            } else if has_local {
                "API key · ~/.kimi-code"
            } else {
                "needs Kimi auth"
            }
            .to_owned(),
        ),
        buckets,
        status,
        source,
        confidence,
        now,
        last_error: match status {
            UsageSnapshotStatus::NeedsSecret => {
                Some("Kimi auth not available to Capsule".to_owned())
            }
            UsageSnapshotStatus::Unsupported => Some(provider_error.unwrap_or_else(|| {
                "Kimi billing endpoint unavailable; local presence only".to_owned()
            })),
            _ => None,
        },
    })
}

fn minimax_snapshot(agent: &str, token: Option<&str>, now: i64) -> FocusedUsageView {
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
                "Coding plan",
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
    usage_view(UsageViewInput {
        agent,
        provider: Some(UsageSurface::Minimax.label()),
        surface: UsageSurface::Minimax,
        account_label: String::new(),
        username: None,
        plan_label: provider_usage
            .as_ref()
            .and_then(MiniMaxUsageResponse::plan_name),
        credential_origin: Some(
            if has_token {
                "API token · env MINIMAX_API_KEY"
            } else {
                "needs MINIMAX_CODING_API_KEY"
            }
            .to_owned(),
        ),
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

fn provider_key_snapshot(
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
    usage_view(UsageViewInput {
        agent,
        provider: Some(surface.label()),
        surface,
        account_label: String::new(),
        username: None,
        plan_label: provider_quota
            .as_ref()
            .and_then(ZaiQuotaResponse::plan_name),
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

fn opencode_snapshot(agent: &str, provider: Option<&str>, now: i64) -> FocusedUsageView {
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

fn unsupported_snapshot(agent: &str, provider: Option<&str>, now: i64) -> FocusedUsageView {
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

struct UsageViewInput<'a> {
    agent: &'a str,
    provider: Option<&'a str>,
    surface: UsageSurface,
    account_label: String,
    username: Option<String>,
    plan_label: Option<String>,
    credential_origin: Option<String>,
    buckets: Vec<QuotaBucketView>,
    status: UsageSnapshotStatus,
    source: UsageSource,
    confidence: UsageConfidence,
    now: i64,
    last_error: Option<String>,
}

fn usage_view(input: UsageViewInput<'_>) -> FocusedUsageView {
    let headline = status_bar_label(
        input.surface,
        &input.account_label,
        input.status,
        &input.buckets,
    );
    FocusedUsageView {
        focused_agent: Some(input.agent.to_owned()),
        focused_provider: input
            .provider
            .map(str::to_owned)
            .or_else(|| Some(input.surface.label().to_owned())),
        account: FocusedAccountHeader {
            provider_label: input.surface.account_label().to_owned(),
            account_label: input.account_label,
            username: input.username,
            plan_label: input.plan_label,
            credential_origin: input.credential_origin,
        },
        buckets: input.buckets,
        status: input.status,
        source: input.source,
        confidence: input.confidence,
        fetched_at_epoch: input.now,
        updated_label: match input.status {
            UsageSnapshotStatus::Fresh => "Updated just now",
            UsageSnapshotStatus::Stale => "Stale",
            UsageSnapshotStatus::NeedsLogin => "Needs login",
            UsageSnapshotStatus::NeedsSecret => "Needs secret",
            UsageSnapshotStatus::Unsupported => "Unsupported",
            UsageSnapshotStatus::Unavailable => "Unavailable",
            UsageSnapshotStatus::Error => "Error",
        }
        .to_owned(),
        status_bar_label: headline,
        tabs: provider_tabs(input.surface),
        last_error: input.last_error,
    }
}

fn status_bar_label(
    surface: UsageSurface,
    _account_label: &str,
    status: UsageSnapshotStatus,
    buckets: &[QuotaBucketView],
) -> String {
    if let Some(headline) = status_bar_headline_for_surface(surface, buckets) {
        return headline;
    }
    match status {
        UsageSnapshotStatus::Fresh => "usage cached".to_owned(),
        UsageSnapshotStatus::Stale => "stale".to_owned(),
        UsageSnapshotStatus::NeedsLogin => "login".to_owned(),
        UsageSnapshotStatus::NeedsSecret => "secret".to_owned(),
        UsageSnapshotStatus::Unsupported => "unsupported".to_owned(),
        UsageSnapshotStatus::Unavailable => "usage unavailable".to_owned(),
        UsageSnapshotStatus::Error => "error".to_owned(),
    }
}

fn status_bar_headline_for_surface(
    surface: UsageSurface,
    buckets: &[QuotaBucketView],
) -> Option<String> {
    if surface == UsageSurface::Amp {
        amp_status_bar_headline(buckets)
    } else {
        let labels = status_bar_quota_labels(buckets);
        (!labels.is_empty()).then(|| labels.join(" · "))
    }
}

fn amp_status_bar_headline(buckets: &[QuotaBucketView]) -> Option<String> {
    let free = buckets
        .iter()
        .find(|bucket| status_bar_fresh_or_stale(bucket) && bucket.label == "Amp Free")
        .and_then(|bucket| {
            bucket
                .remaining_percent
                .map(|remaining| format!("Free {remaining}%"))
        });
    let credits = buckets
        .iter()
        .find(|bucket| {
            status_bar_fresh_or_stale(bucket)
                && matches!(bucket.label.as_str(), "Individual credits" | "Credits")
        })
        .and_then(amp_credit_status_label);
    match (free, credits) {
        (Some(free), Some(credits)) => Some(format!("{free} · {credits}")),
        (Some(free), None) => Some(free),
        (None, Some(credits)) => Some(credits),
        (None, None) => None,
    }
}

fn amp_credit_status_label(bucket: &QuotaBucketView) -> Option<String> {
    bucket
        .limit_label
        .as_deref()
        .or_else(|| {
            bucket
                .pace_label
                .as_deref()
                .and_then(|label| label.strip_prefix("Individual credits: "))
        })
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .map(str::to_owned)
}

fn status_bar_quota_labels(buckets: &[QuotaBucketView]) -> Vec<String> {
    // Read the semantic slot the provider tagged at construction, not the
    // free-text label — a window rename can't silently break the headline.
    [
        (StatusSlot::Session, "Session"),
        (StatusSlot::Weekly, "Weekly"),
    ]
    .into_iter()
    .filter_map(|(slot, label)| {
        buckets
            .iter()
            .find(|bucket| bucket.status_slot == Some(slot) && status_bar_fresh_or_stale(bucket))
            .and_then(|bucket| {
                bucket
                    .remaining_percent
                    .map(|remaining| format!("{label} {remaining}%"))
            })
    })
    .collect()
}

fn status_bar_fresh_or_stale(bucket: &QuotaBucketView) -> bool {
    matches!(
        bucket.status,
        UsageSnapshotStatus::Fresh | UsageSnapshotStatus::Stale
    )
}

fn compact_account_identity(account_label: &str) -> &str {
    let trimmed = account_label.trim();
    if trimmed.is_empty()
        || trimmed.starts_with("needs ")
        || trimmed.ends_with(" unavailable")
        || trimmed.contains(" not available")
    {
        "account unavailable"
    } else {
        trimmed
    }
}

/// True when `word` appears in `text` as a whole alphanumeric token, so a short
/// provider token (`amp`) is not matched inside an unrelated word (`example`).
fn contains_word(text: &str, word: &str) -> bool {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .any(|token| token == word)
}

/// Best-effort canonical surface for any provider-ish text — a tab label
/// (`OpenAI / Codex`) or an account provider label (`codex`), case-insensitive
/// and synonym-aware. `None` for text that names no known provider.
fn surface_from_text(text: &str) -> Option<UsageSurface> {
    let text = text.to_ascii_lowercase();
    UsageSurface::ALL.iter().copied().find(|&surface| {
        surface.synonyms().iter().any(|syn| {
            // Amp matches only on a word boundary so labels like `example` or
            // `ramp` don't false-link; every other token keeps the historical
            // case-insensitive substring policy.
            if matches!(surface, UsageSurface::Amp) {
                contains_word(&text, syn)
            } else {
                text.contains(syn)
            }
        })
    })
}

fn provider_matches_usage_label(provider: &str, account_provider: &str) -> bool {
    // Compare the canonical surface each label resolves to instead of a long
    // synonym OR-chain. When both name a known surface, equality decides; when
    // both are outside the known set (e.g. OpenCode), fall back to a case-
    // insensitive substring match; a known surface never matches an unknown
    // label (else a stray substring like `amp` in `example` would link them).
    match (
        surface_from_text(provider),
        surface_from_text(account_provider),
    ) {
        (Some(left), Some(right)) => left == right,
        (None, None) => {
            let provider = provider.to_ascii_lowercase();
            let account_provider = account_provider.to_ascii_lowercase();
            provider == account_provider
                || provider.contains(&account_provider)
                || account_provider.contains(&provider)
        }
        _ => false,
    }
}

fn most_constrained_fresh_bucket(buckets: &[QuotaBucketView]) -> Option<&QuotaBucketView> {
    buckets
        .iter()
        .filter(|bucket| bucket.status == UsageSnapshotStatus::Fresh)
        .filter(|bucket| bucket.remaining_percent.is_some())
        .min_by_key(|bucket| bucket.remaining_percent.unwrap_or(u8::MAX))
}

fn preserve_cached_quota_on_failed_refresh(view: &mut FocusedUsageView, cached: &FocusedUsageView) {
    if !matches!(
        view.status,
        UsageSnapshotStatus::Stale | UsageSnapshotStatus::NeedsLogin | UsageSnapshotStatus::Error
    ) || cached.status != UsageSnapshotStatus::Fresh
        || cached.buckets.is_empty()
    {
        return;
    }

    view.status = UsageSnapshotStatus::Stale;
    view.source = UsageSource::Cache;
    view.confidence = cached.confidence;
    view.updated_label = "Stale".to_owned();
    view.buckets = cached
        .buckets
        .iter()
        .cloned()
        .map(|mut bucket| {
            bucket.status = UsageSnapshotStatus::Stale;
            bucket
        })
        .collect();
    if view.account.plan_label.is_none() {
        view.account.plan_label = cached.account.plan_label.clone();
    }
    if compact_account_identity(&view.account.account_label) == "account unavailable" {
        view.account.account_label = cached.account.account_label.clone();
    }
    if let Some(error) = &mut view.last_error {
        error.push_str("; showing last cached quota");
    } else {
        view.last_error = Some("showing last cached quota".to_owned());
    }
    view.status_bar_label = status_bar_label(
        resolve_surface(
            view.focused_agent.as_deref().unwrap_or_default(),
            view.focused_provider.as_deref(),
        ),
        &view.account.account_label,
        view.status,
        &view.buckets,
    );
}

fn provider_tabs(active: UsageSurface) -> Vec<UsageProviderTab> {
    [
        UsageSurface::Codex,
        UsageSurface::Claude,
        UsageSurface::Amp,
        UsageSurface::Grok,
        UsageSurface::Zai,
        UsageSurface::Kimi,
        UsageSurface::Minimax,
    ]
    .into_iter()
    .map(|surface| UsageProviderTab {
        label: surface.label().to_owned(),
        status_label: if surface == active { "focused" } else { "" }.to_owned(),
        account_label: "account unavailable".to_owned(),
        plan_label: None,
        source_label: None,
        active: surface == active,
    })
    .collect()
}

fn enrich_provider_tabs(view: &mut FocusedUsageView, snapshots: &HashMap<String, CachedUsage>) {
    let active_label = view.account.provider_label.clone();
    let active_account = compact_account_identity(&view.account.account_label).to_owned();
    let active_plan = view.account.plan_label.clone();
    let active_status = usage_tab_status_label(view);
    let active_source = usage_tab_source_label(view);
    for tab in &mut view.tabs {
        if tab.active || provider_matches_usage_label(&tab.label, &active_label) {
            tab.account_label = active_account.clone();
            tab.plan_label = active_plan.clone();
            tab.status_label = active_status.clone();
            tab.source_label = Some(active_source.clone());
            continue;
        }
        let Some(cached) = snapshots
            .values()
            .filter(|cached| {
                provider_matches_usage_label(&tab.label, &cached.view.account.provider_label)
            })
            .max_by_key(|cached| cached.view.fetched_at_epoch)
        else {
            tab.account_label = "account unavailable".to_owned();
            tab.plan_label = None;
            tab.status_label = "not cached".to_owned();
            tab.source_label = None;
            continue;
        };
        tab.account_label = compact_account_identity(&cached.view.account.account_label).to_owned();
        tab.plan_label = cached.view.account.plan_label.clone();
        tab.status_label = usage_tab_status_label(&cached.view);
        tab.source_label = Some(usage_tab_source_label(&cached.view));
    }
}

/// Freshness + source tag for the Overview row, e.g. "fresh · provider" or
/// "stale · local estimate", matching the CodexBar-style status column.
fn usage_tab_source_label(view: &FocusedUsageView) -> String {
    let freshness = match view.status {
        UsageSnapshotStatus::Fresh => "fresh",
        UsageSnapshotStatus::Stale => "stale",
        UsageSnapshotStatus::NeedsLogin => "needs login",
        UsageSnapshotStatus::NeedsSecret => "needs secret",
        UsageSnapshotStatus::Unsupported => "unsupported",
        UsageSnapshotStatus::Unavailable => "unavailable",
        UsageSnapshotStatus::Error => "error",
    };
    let source = match view.source {
        UsageSource::ProviderApi => "provider",
        UsageSource::Cli => "managed CLI",
        UsageSource::LocalLogs => "local estimate",
        UsageSource::Cache => "cache",
        UsageSource::None => "no source",
    };
    format!("{freshness} · {source}")
}

fn usage_tab_status_label(view: &FocusedUsageView) -> String {
    if view.status == UsageSnapshotStatus::Fresh
        && let Some(bucket) = most_constrained_fresh_bucket(&view.buckets)
        && let Some(remaining) = bucket.remaining_percent
    {
        let mut label = format!("{remaining}% left");
        if let Some(reset) = &bucket.reset_label {
            label.push_str(" · ");
            label.push_str(reset);
        }
        return label;
    }
    match view.status {
        UsageSnapshotStatus::Fresh => "fresh".to_owned(),
        UsageSnapshotStatus::Stale => "stale".to_owned(),
        UsageSnapshotStatus::NeedsLogin => "needs login".to_owned(),
        UsageSnapshotStatus::NeedsSecret => "needs secret".to_owned(),
        UsageSnapshotStatus::Unsupported => "unsupported".to_owned(),
        UsageSnapshotStatus::Unavailable => "unavailable".to_owned(),
        UsageSnapshotStatus::Error => "error".to_owned(),
    }
}

fn bucket(
    label: &str,
    used_label: Option<String>,
    limit_label: Option<String>,
    remaining_percent: Option<u8>,
    reset_label: Option<String>,
    pace_label: Option<&str>,
    status: UsageSnapshotStatus,
) -> QuotaBucketView {
    QuotaBucketView {
        label: label.to_owned(),
        used_label,
        limit_label,
        remaining_percent,
        reset_label,
        resets_at: None,
        status_slot: None,
        pace_label: pace_label.map(str::to_owned),
        status,
    }
}

/// Stamp a quota bucket's status-bar slot at construction. Returns the bucket so
/// it can be tagged and pushed in one expression (`buckets.push(with_status_slot(
/// build(...), Some(StatusSlot::Session)))`) — the slot rides with the view it
/// belongs to, so no later `last_mut`/positional step can float the tag onto the
/// wrong bucket.
fn with_status_slot(mut view: QuotaBucketView, slot: Option<StatusSlot>) -> QuotaBucketView {
    view.status_slot = slot;
    view
}

/// Build a window bucket carrying both the formatted reset label and the raw
/// reset epoch (RC2), so the CLI report can emit `resets_at`. `reset_at` is the
/// authoritative timestamp; `reset_label` is derived from it.
#[allow(clippy::too_many_arguments)]
fn timed_bucket(
    label: &str,
    used_label: Option<String>,
    limit_label: Option<String>,
    remaining_percent: Option<u8>,
    reset_at: Option<i64>,
    now: i64,
    pace_label: Option<&str>,
    status: UsageSnapshotStatus,
) -> QuotaBucketView {
    let mut view = bucket(
        label,
        used_label,
        limit_label,
        remaining_percent,
        reset_at.map(|epoch| reset_label(epoch, now)),
        pace_label,
        status,
    );
    view.resets_at = reset_at;
    view
}

#[derive(Debug, Clone)]
struct ClaudeOAuthCredentials {
    access_token: String,
    subscription_type: Option<String>,
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
fn first_credential_with_path<T>(
    paths: &[PathBuf],
    load: impl Fn(&Path) -> Option<T>,
) -> Option<(PathBuf, T)> {
    paths
        .iter()
        .find_map(|path| load(path.as_path()).map(|value| (path.clone(), value)))
}

#[cfg(test)]
fn first_credential<T>(paths: &[PathBuf], load: impl Fn(&Path) -> Option<T>) -> Option<T> {
    first_credential_with_path(paths, load).map(|(_, value)| value)
}

/// Read and parse a JSON credential/config file, distinguishing "absent"
/// (expected — `None`, no log) from "present but broken" (a real error the
/// operator must see — logged via the always-on `clog!`, then `None`). The
/// `.ok()?` idiom these loaders previously used collapsed both cases, so a
/// corrupt or permission-denied token file looked identical to a logged-out
/// provider and surfaced no diagnostic.
fn read_json_file(path: &Path) -> Option<serde_json::Value> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => {
            if error.kind() != std::io::ErrorKind::NotFound {
                crate::clog!(
                    "usage credential read failed for {}: {error}",
                    path.display()
                );
            }
            return None;
        }
    };
    match serde_json::from_str(&text) {
        Ok(value) => Some(value),
        Err(error) => {
            crate::clog!(
                "usage credential parse failed for {}: {error}",
                path.display()
            );
            None
        }
    }
}

/// Claude account email (F12): `~/.claude.json` carries `oauthAccount` metadata
/// (never the token), and `CodexBar` reads the address from there. Returns the
/// trimmed `oauthAccount.emailAddress`, or `None` when absent.
fn claude_email_from_value(value: &serde_json::Value) -> Option<String> {
    let oauth = value.get("oauthAccount")?;
    oauth
        .get("emailAddress")
        .or_else(|| oauth.get("email_address"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|email| !email.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
fn load_claude_account_email(path: &Path) -> Option<String> {
    claude_email_from_value(&read_json_file(path)?)
}

fn claude_oauth_from_value(value: &serde_json::Value) -> Option<ClaudeOAuthCredentials> {
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
        access_token,
        subscription_type,
    })
}

#[cfg(test)]
fn load_claude_oauth_credentials(path: &Path) -> Option<ClaudeOAuthCredentials> {
    claude_oauth_from_value(&read_json_file(path)?)
}

/// Resolve a provider credential (with the winning path, for the `Auth:`
/// origin) and its account label in one home-first walk, reading and parsing
/// each candidate file at most once. `extract_credential` pulls the token from a
/// parsed file; `extract_label` pulls the account email/label. The walk stops as
/// soon as both are found, so a later candidate never re-reads a resolved file.
fn resolve_identity<T>(
    candidates: &[PathBuf],
    extract_credential: impl Fn(&serde_json::Value) -> Option<T>,
    extract_label: impl Fn(&serde_json::Value) -> Option<String>,
) -> (Option<(PathBuf, T)>, Option<String>) {
    let mut credential = None;
    let mut label = None;
    for path in candidates {
        if credential.is_some() && label.is_some() {
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
    }
    (credential, label)
}

#[derive(Debug, Deserialize)]
struct ClaudeOAuthUsageResponse {
    #[serde(rename = "five_hour")]
    five_hour: Option<ClaudeOAuthUsageWindow>,
    // `seven_day` is the Weekly window. `seven_day_oauth_apps` is a SEPARATE
    // window the API also returns — it must NOT be aliased here (the API sends
    // both keys, so aliasing collides into a serde "duplicate field" and fails
    // the whole decode). It is not a CodexBar quota window, so it is ignored.
    #[serde(rename = "seven_day")]
    seven_day: Option<ClaudeOAuthUsageWindow>,
    #[serde(rename = "seven_day_sonnet")]
    seven_day_sonnet: Option<ClaudeOAuthUsageWindow>,
    #[serde(rename = "seven_day_opus")]
    seven_day_opus: Option<ClaudeOAuthUsageWindow>,
    #[serde(alias = "seven_day_claude_routines")]
    #[serde(alias = "claude_routines")]
    #[serde(alias = "routines")]
    #[serde(alias = "seven_day_cowork")]
    #[serde(rename = "seven_day_routines")]
    seven_day_routines: Option<ClaudeOAuthUsageWindow>,
    #[serde(rename = "extra_usage")]
    extra_usage: Option<ClaudeOAuthExtraUsage>,
}

#[derive(Debug, Deserialize)]
struct ClaudeOAuthUsageWindow {
    utilization: Option<f64>,
    #[serde(rename = "resets_at")]
    resets_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ClaudeOAuthExtraUsage {
    #[serde(rename = "is_enabled")]
    is_enabled: Option<bool>,
    #[serde(rename = "monthly_limit")]
    monthly_limit: Option<f64>,
    #[serde(rename = "used_credits")]
    used_credits: Option<f64>,
    utilization: Option<f64>,
    currency: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct ClaudeCliUsage {
    session_used: Option<f64>,
    weekly_used: Option<f64>,
    sonnet_used: Option<f64>,
}

impl ClaudeCliUsage {
    fn buckets(&self) -> Vec<QuotaBucketView> {
        let mut buckets = Vec::new();
        // The impoverished CLI fallback still fills the headline slots — tag them
        // so the status bar renders even when the OAuth fetch failed (the slot
        // is a semantic role, independent of which source produced the window).
        push_claude_cli_bucket(
            &mut buckets,
            "Session",
            Some(StatusSlot::Session),
            self.session_used,
        );
        push_claude_cli_bucket(
            &mut buckets,
            "Weekly",
            Some(StatusSlot::Weekly),
            self.weekly_used,
        );
        push_claude_cli_bucket(&mut buckets, "Sonnet", None, self.sonnet_used);
        buckets
    }
}

fn push_claude_cli_bucket(
    buckets: &mut Vec<QuotaBucketView>,
    label: &str,
    slot: Option<StatusSlot>,
    used: Option<f64>,
) {
    let Some(used) = used else {
        return;
    };
    buckets.push(with_status_slot(
        bucket(
            label,
            used_percent_label(used),
            Some("100%".to_owned()),
            remaining_from_fraction(used),
            None,
            None,
            UsageSnapshotStatus::Fresh,
        ),
        slot,
    ));
}

impl ClaudeOAuthUsageResponse {
    fn into_buckets(self, now: i64) -> Vec<QuotaBucketView> {
        let mut buckets = Vec::new();
        push_claude_window(
            &mut buckets,
            "Session",
            Some(StatusSlot::Session),
            self.five_hour,
            now,
        );
        push_claude_window(
            &mut buckets,
            "Weekly",
            Some(StatusSlot::Weekly),
            self.seven_day,
            now,
        );
        push_claude_window(&mut buckets, "Sonnet", None, self.seven_day_sonnet, now);
        push_claude_window(&mut buckets, "Opus", None, self.seven_day_opus, now);
        push_claude_window(
            &mut buckets,
            "Daily Routines",
            None,
            self.seven_day_routines,
            now,
        );
        if let Some(extra) = self.extra_usage
            && extra.is_enabled.unwrap_or(true)
        {
            // surface Claude credits as spent vs cap only —
            // `<currency> <spent> spent` + `NN% used`, against the monthly cap.
            let remaining_percent = extra.utilization.and_then(remaining_from_fraction);
            let currency = extra.currency.unwrap_or_else(|| "credits".to_owned());
            let used = extra
                .used_credits
                .map(|used| format!("{} spent", format_extra_usage_amount(used, &currency)));
            let limit = extra
                .monthly_limit
                .map(|limit| format_extra_usage_amount(limit, &currency));
            let pace = remaining_percent
                .map(|remaining| format!("{}% used", 100u8.saturating_sub(remaining)));
            buckets.push(bucket(
                "Extra usage",
                used,
                limit,
                remaining_percent,
                None,
                pace.as_deref(),
                UsageSnapshotStatus::Fresh,
            ));
        }
        buckets
    }
}

fn push_claude_window(
    buckets: &mut Vec<QuotaBucketView>,
    label: &str,
    slot: Option<StatusSlot>,
    window: Option<ClaudeOAuthUsageWindow>,
    now: i64,
) {
    let Some(window) = window else {
        return;
    };
    let reset_at = window.resets_at.as_deref().and_then(parse_iso_epoch);
    let window_seconds = claude_window_seconds(label);
    let remaining = window.utilization.and_then(remaining_from_fraction);
    let pace = quota_pace_label(remaining, reset_at, window_seconds, now);
    buckets.push(with_status_slot(
        timed_bucket(
            label,
            window.utilization.and_then(used_percent_label),
            Some("100%".to_owned()),
            remaining,
            reset_at,
            now,
            pace.as_deref(),
            UsageSnapshotStatus::Fresh,
        ),
        slot,
    ));
}

fn claude_window_seconds(label: &str) -> Option<i64> {
    match label {
        "Session" => Some(5 * 60 * 60),
        "Weekly" => Some(7 * 24 * 60 * 60),
        _ => None,
    }
}

fn fetch_claude_oauth_usage(access_token: &str) -> Result<ClaudeOAuthUsageResponse, String> {
    let user_agent = claude_code_user_agent();
    get_json_bearer(
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

fn claude_code_user_agent() -> String {
    claude_code_user_agent_with(|command, args, timeout| {
        run_cli_with_timeout_full(command, args, timeout)
    })
    .unwrap_or_else(|| CLAUDE_CODE_USER_AGENT_FALLBACK.to_owned())
}

fn claude_code_user_agent_with<F>(mut runner: F) -> Option<String>
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

fn claude_code_version_from_text(text: &str) -> Option<String> {
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

#[derive(Debug, Clone)]
struct CodexOAuthCredentials {
    access_token: String,
    account_id: Option<String>,
    account_label: Option<String>,
}

#[cfg(test)]
fn load_codex_oauth_credentials(path: &Path) -> Option<CodexOAuthCredentials> {
    codex_oauth_from_value(&read_json_file(path)?)
}

fn codex_oauth_from_value(value: &serde_json::Value) -> Option<CodexOAuthCredentials> {
    if let Some(api_key) = value
        .get("OPENAI_API_KEY")
        .and_then(serde_json::Value::as_str)
        && !api_key.trim().is_empty()
    {
        return Some(CodexOAuthCredentials {
            access_token: api_key.trim().to_owned(),
            account_id: None,
            account_label: Some("OPENAI_API_KEY".to_owned()),
        });
    }
    let tokens = value.get("tokens")?;
    let access_token = tokens
        .get("access_token")
        .or_else(|| tokens.get("accessToken"))
        .and_then(serde_json::Value::as_str)?
        .trim()
        .to_owned();
    if access_token.is_empty() {
        return None;
    }
    let account_id = tokens
        .get("account_id")
        .or_else(|| tokens.get("accountId"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let account_label = tokens
        .get("id_token")
        .or_else(|| tokens.get("idToken"))
        .and_then(serde_json::Value::as_str)
        .and_then(codex_account_label_from_id_token)
        .or_else(|| {
            tokens
                .get("account_id")
                .or_else(|| tokens.get("accountId"))
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        });
    Some(CodexOAuthCredentials {
        access_token,
        account_id,
        account_label,
    })
}

fn codex_account_label_from_id_token(token: &str) -> Option<String> {
    let payload = token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(payload))
        .ok()?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    first_string_key(&value, "email")
        .or_else(|| first_string_key(&value, "preferred_username"))
        .or_else(|| first_string_key(&value, "name"))
        .or_else(|| first_string_key(&value, "sub").map(|sub| format!("ChatGPT account {sub}")))
}

#[derive(Debug, Deserialize)]
struct CodexUsageResponse {
    #[serde(rename = "plan_type")]
    plan_type: Option<String>,
    #[serde(rename = "rate_limit")]
    rate_limit: Option<CodexRateLimitDetails>,
    credits: Option<CodexCreditDetails>,
    #[serde(rename = "additional_rate_limits")]
    additional_rate_limits: Option<Vec<CodexAdditionalRateLimit>>,
    #[serde(skip)]
    reset_credits: Option<CodexResetCredits>,
}

#[derive(Debug, Deserialize)]
struct CodexRateLimitDetails {
    #[serde(rename = "primary_window")]
    primary_window: Option<CodexWindowSnapshot>,
    #[serde(rename = "secondary_window")]
    secondary_window: Option<CodexWindowSnapshot>,
}

#[derive(Debug, Deserialize)]
struct CodexWindowSnapshot {
    #[serde(rename = "used_percent")]
    used_percent: Option<u8>,
    #[serde(rename = "reset_at")]
    reset_at: Option<i64>,
    #[serde(rename = "limit_window_seconds")]
    limit_window_seconds: Option<i64>,
    #[serde(skip)]
    window_duration_mins: Option<i64>,
}

impl CodexWindowSnapshot {
    fn from_rpc(window: CodexRpcRateLimitWindow) -> Self {
        Self {
            used_percent: Some(window.used_percent.round().clamp(0.0, 100.0) as u8),
            reset_at: window.resets_at,
            limit_window_seconds: None,
            window_duration_mins: window.window_duration_mins,
        }
    }

    fn window_label(&self) -> Option<String> {
        let minutes = self
            .window_duration_mins
            .or_else(|| self.limit_window_seconds.map(|seconds| seconds / 60))?;
        window_minutes_label(minutes)
    }

    fn window_seconds(&self) -> Option<i64> {
        self.limit_window_seconds
            .or_else(|| self.window_duration_mins.map(|minutes| minutes * 60))
    }
}

#[derive(Debug, Deserialize)]
struct CodexCreditDetails {
    #[serde(rename = "has_credits")]
    has_credits: Option<bool>,
    unlimited: Option<bool>,
    balance: Option<serde_json::Value>,
}

impl CodexCreditDetails {
    fn from_rpc(credits: CodexRpcCredits) -> Self {
        Self {
            has_credits: Some(credits.has_credits),
            unlimited: Some(credits.unlimited),
            balance: credits.balance.map(serde_json::Value::String),
        }
    }
}

#[derive(Debug, Deserialize)]
struct CodexAdditionalRateLimit {
    #[serde(rename = "limit_name")]
    limit_name: Option<String>,
    #[serde(rename = "metered_feature")]
    metered_feature: Option<String>,
    #[serde(rename = "rate_limit")]
    rate_limit: Option<CodexRateLimitDetails>,
}

#[derive(Debug, Clone, Default)]
struct ManagedCliLaunchGate {
    cooldown_until: Option<Instant>,
    last_error: Option<String>,
}

impl ManagedCliLaunchGate {
    fn can_launch(&self, label: &str, now: Instant) -> Result<(), String> {
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

    fn record_launch_failure(&mut self, message: String) {
        self.cooldown_until = Some(Instant::now() + CODEX_RPC_LAUNCH_COOLDOWN);
        self.last_error = Some(message);
    }

    fn record_success(&mut self) {
        self.cooldown_until = None;
        self.last_error = None;
    }
}

#[derive(Debug, Deserialize)]
struct CodexRpcAccountResponse {
    account: Option<CodexRpcAccountDetails>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum CodexRpcAccountDetails {
    #[serde(rename = "apikey")]
    ApiKey,
    Chatgpt {
        email: Option<String>,
        #[serde(rename = "planType")]
        plan_type: Option<String>,
    },
}

#[derive(Debug, Deserialize)]
struct CodexRpcRateLimitsResponse {
    #[serde(rename = "rateLimits")]
    rate_limits: CodexRpcRateLimits,
    // Per-limit-id windows. Every entry other than the main "codex" limit
    // (already surfaced as Session/Weekly) is an extra limit — the
    // "…Codex-Spark" entry carries the Codex Spark 5-hour/Weekly windows.
    #[serde(rename = "rateLimitsByLimitId", default)]
    rate_limits_by_limit_id: BTreeMap<String, CodexRpcLimitEntry>,
    #[serde(rename = "rateLimitResetCredits")]
    reset_credits: Option<CodexRpcResetCredits>,
}

#[derive(Debug, Deserialize)]
struct CodexRpcLimitEntry {
    #[serde(rename = "limitId")]
    limit_id: Option<String>,
    #[serde(rename = "limitName")]
    limit_name: Option<String>,
    primary: Option<CodexRpcRateLimitWindow>,
    secondary: Option<CodexRpcRateLimitWindow>,
}

#[derive(Debug, Deserialize)]
struct CodexRpcResetCredits {
    #[serde(rename = "availableCount")]
    available_count: i64,
}

#[derive(Debug, Deserialize)]
struct CodexRpcRateLimits {
    primary: Option<CodexRpcRateLimitWindow>,
    secondary: Option<CodexRpcRateLimitWindow>,
    credits: Option<CodexRpcCredits>,
    #[serde(rename = "planType")]
    plan_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CodexRpcRateLimitWindow {
    #[serde(rename = "usedPercent")]
    used_percent: f64,
    #[serde(rename = "windowDurationMins")]
    window_duration_mins: Option<i64>,
    #[serde(rename = "resetsAt")]
    resets_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct CodexRpcCredits {
    #[serde(rename = "hasCredits")]
    has_credits: bool,
    unlimited: bool,
    balance: Option<String>,
}

struct CodexRpcUsage {
    response: CodexUsageResponse,
    account_label: Option<String>,
}

impl CodexRpcUsage {
    fn from_rpc(
        limits: CodexRpcRateLimitsResponse,
        account: Option<CodexRpcAccountResponse>,
    ) -> Self {
        let account_details = account.and_then(|response| response.account);
        let account_label = match &account_details {
            Some(CodexRpcAccountDetails::Chatgpt { email, .. }) => email.clone(),
            Some(CodexRpcAccountDetails::ApiKey) => Some("Codex API key".to_owned()),
            None => None,
        };
        let account_plan = match account_details {
            Some(CodexRpcAccountDetails::Chatgpt { plan_type, .. }) => plan_type,
            _ => None,
        };
        let CodexRpcRateLimitsResponse {
            rate_limits,
            rate_limits_by_limit_id,
            reset_credits: rpc_reset_credits,
        } = limits;
        // Every per-limit-id entry except the main "codex" limit is an extra
        // rate limit (e.g. Codex Spark); its primary/secondary become the
        // "<label> 5-hour"/"Weekly" buckets in `buckets()`.
        let additional_rate_limits: Vec<CodexAdditionalRateLimit> = rate_limits_by_limit_id
            .into_values()
            .filter(|entry| entry.limit_id.as_deref() != Some("codex"))
            .filter_map(|entry| {
                let primary = entry.primary.map(CodexWindowSnapshot::from_rpc);
                let secondary = entry.secondary.map(CodexWindowSnapshot::from_rpc);
                if primary.is_none() && secondary.is_none() {
                    return None;
                }
                Some(CodexAdditionalRateLimit {
                    limit_name: entry.limit_name,
                    metered_feature: None,
                    rate_limit: Some(CodexRateLimitDetails {
                        primary_window: primary,
                        secondary_window: secondary,
                    }),
                })
            })
            .collect();
        let reset_credits = rpc_reset_credits
            .filter(|reset| reset.available_count > 0)
            .map(|reset| CodexResetCredits {
                credits: Vec::new(),
                available_count: reset.available_count,
            });
        let response = CodexUsageResponse {
            plan_type: account_plan.or(rate_limits.plan_type),
            rate_limit: Some(CodexRateLimitDetails {
                primary_window: rate_limits.primary.map(CodexWindowSnapshot::from_rpc),
                secondary_window: rate_limits.secondary.map(CodexWindowSnapshot::from_rpc),
            }),
            credits: rate_limits.credits.map(CodexCreditDetails::from_rpc),
            additional_rate_limits: (!additional_rate_limits.is_empty())
                .then_some(additional_rate_limits),
            reset_credits,
        };
        Self {
            response,
            account_label,
        }
    }
}

impl CodexUsageResponse {
    fn buckets(&self, now: i64) -> Vec<QuotaBucketView> {
        let mut buckets = Vec::new();
        if let Some(rate_limit) = &self.rate_limit {
            push_codex_window(
                &mut buckets,
                "Session",
                Some(StatusSlot::Session),
                rate_limit.primary_window.as_ref(),
                now,
            );
            push_codex_window(
                &mut buckets,
                "Weekly",
                Some(StatusSlot::Weekly),
                rate_limit.secondary_window.as_ref(),
                now,
            );
        }
        for limit in self.additional_rate_limits.iter().flatten() {
            let label = limit
                .limit_name
                .as_deref()
                .or(limit.metered_feature.as_deref())
                .map_or_else(|| "Codex extra limit".to_owned(), codex_limit_label);
            if let Some(rate_limit) = &limit.rate_limit {
                // Extra per-feature limits are detail rows, never the headline.
                push_codex_window(
                    &mut buckets,
                    &format!("{label} 5-hour"),
                    None,
                    rate_limit.primary_window.as_ref(),
                    now,
                );
                push_codex_window(
                    &mut buckets,
                    &format!("{label} Weekly"),
                    None,
                    rate_limit.secondary_window.as_ref(),
                    now,
                );
            }
        }
        if let Some(reset_credits) = &self.reset_credits
            && reset_credits.available_count > 0
        {
            let detail = reset_credits.detail_label(now);
            buckets.push(bucket(
                "Limit Reset Credits",
                None,
                None,
                None,
                None,
                Some(detail.as_str()),
                UsageSnapshotStatus::Fresh,
            ));
        }
        if let Some(credits) = &self.credits
            && credits.has_credits.unwrap_or(false)
        {
            let balance = credits.balance.as_ref().and_then(json_number);
            buckets.push(bucket(
                "Credits",
                None,
                balance.map(|value| format_amount_with_unit(value, "credits")),
                credits.unlimited.unwrap_or(false).then_some(100),
                None,
                credits.unlimited.unwrap_or(false).then_some("unlimited"),
                UsageSnapshotStatus::Fresh,
            ));
        }
        buckets
    }
}

#[derive(Debug, Deserialize)]
struct CodexResetCredits {
    credits: Vec<CodexResetCredit>,
    #[serde(rename = "available_count")]
    available_count: i64,
}

impl CodexResetCredits {
    fn detail_label(&self, now: i64) -> String {
        let count = if self.available_count == 1 {
            "1 manual reset available".to_owned()
        } else {
            format!("{} manual resets available", self.available_count)
        };
        let Some(expires_at) = self.next_expiring_available_epoch(now) else {
            return count;
        };
        format!("{count} · Next expires {}", expiry_label(expires_at, now))
    }

    fn next_expiring_available_epoch(&self, now: i64) -> Option<i64> {
        self.credits
            .iter()
            .filter(|credit| credit.status.as_deref() == Some("available"))
            .filter_map(|credit| {
                credit
                    .expires_at
                    .as_deref()
                    .and_then(parse_iso_epoch)
                    .filter(|epoch| *epoch > now)
            })
            .min()
    }
}

#[derive(Debug, Deserialize)]
struct CodexResetCredit {
    status: Option<String>,
    #[serde(rename = "expires_at")]
    expires_at: Option<String>,
}

fn push_codex_window(
    buckets: &mut Vec<QuotaBucketView>,
    label: &str,
    slot: Option<StatusSlot>,
    window: Option<&CodexWindowSnapshot>,
    now: i64,
) {
    let Some(window) = window else {
        return;
    };
    let used = window.used_percent.map(|value| value.min(100));
    let remaining = used.map(|value| 100u8.saturating_sub(value));
    let window_seconds = window.window_seconds();
    let pace = quota_pace_label(remaining, window.reset_at, window_seconds, now)
        .or_else(|| window.window_label());
    buckets.push(with_status_slot(
        timed_bucket(
            label,
            used.map(|value| format!("{value}% used")),
            Some("100%".to_owned()),
            remaining,
            window.reset_at,
            now,
            pace.as_deref(),
            UsageSnapshotStatus::Fresh,
        ),
        slot,
    ));
}

fn fetch_codex_rpc_usage(gate: &mut ManagedCliLaunchGate) -> Result<CodexRpcUsage, String> {
    gate.can_launch("Codex app-server", Instant::now())?;
    let mut child = match Command::new("codex")
        .args(["-s", "read-only", "-a", "untrusted", "app-server"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(err) => {
            let message = format!("codex app-server failed to start: {err}");
            gate.record_launch_failure(message.clone());
            return Err(message);
        }
    };

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "codex app-server stdin unavailable".to_owned())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "codex app-server stdout unavailable".to_owned())?;
    let (tx, rx) = mpsc::channel();
    let reader = thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });

    let result: Result<CodexRpcUsage, String> = (|| {
        drop(codex_rpc_request(
            &mut stdin,
            &rx,
            1,
            "initialize",
            serde_json::json!({
                "clientInfo": {
                    "name": "jackin-capsule",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
            CODEX_RPC_INIT_TIMEOUT,
        )?);
        codex_rpc_notification(&mut stdin, "initialized")?;
        let limits_value = codex_rpc_request(
            &mut stdin,
            &rx,
            2,
            "account/rateLimits/read",
            serde_json::json!({}),
            CODEX_RPC_REQUEST_TIMEOUT,
        )?;
        // The account label is non-essential (rate limits already succeeded), so
        // an RPC failure here degrades to no label rather than failing the whole
        // snapshot. Logged at the firehose tier (visible under JACKIN_DEBUG): an
        // absent account is usually a legitimate plan shape, not a fault, so this
        // does not warrant always-on `clog!` noise on every refresh.
        let account_value = codex_rpc_request(
            &mut stdin,
            &rx,
            3,
            "account/read",
            serde_json::json!({}),
            CODEX_RPC_REQUEST_TIMEOUT,
        )
        .inspect_err(|error| crate::cdebug!("codex account/read RPC failed: {error}"))
        .ok();
        let limits = serde_json::from_value::<CodexRpcRateLimitsResponse>(limits_value)
            .map_err(|err| format!("Codex app-server rate limit decode failed: {err}"))?;
        let account = account_value
            .map(serde_json::from_value::<CodexRpcAccountResponse>)
            .transpose()
            .map_err(|err| format!("Codex app-server account decode failed: {err}"))?;
        Ok(CodexRpcUsage::from_rpc(limits, account))
    })();

    drop(stdin);
    drop(child.kill());
    drop(child.wait());
    drop(reader.join());

    if result.is_ok() {
        gate.record_success();
    } else if let Err(message) = &result {
        gate.record_launch_failure(message.clone());
    }
    result
}

fn codex_rpc_request(
    stdin: &mut impl Write,
    rx: &mpsc::Receiver<String>,
    id: i64,
    method: &str,
    params: serde_json::Value,
    timeout: Duration,
) -> Result<serde_json::Value, String> {
    let payload = serde_json::json!({
        "id": id,
        "method": method,
        "params": params,
    });
    write_json_line(
        stdin,
        &payload,
        "Codex app-server request encode failed",
        "Codex app-server request write failed",
    )?;

    let started = Instant::now();
    loop {
        let remaining = timeout
            .checked_sub(started.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));
        if remaining.is_zero() {
            return Err(format!("Codex app-server timed out waiting for {method}"));
        }
        let line = rx
            .recv_timeout(remaining)
            .map_err(|_| format!("Codex app-server timed out waiting for {method}"))?;
        let value: serde_json::Value = serde_json::from_str(&line)
            .map_err(|err| format!("Codex app-server response decode failed: {err}"))?;
        if value.get("id").and_then(serde_json::Value::as_i64) != Some(id) {
            continue;
        }
        if let Some(error) = value.get("error") {
            let message = error
                .get("message")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown error");
            return Err(format!("Codex app-server {method} failed: {message}"));
        }
        return value
            .get("result")
            .cloned()
            .ok_or_else(|| format!("Codex app-server {method} response missing result"));
    }
}

fn codex_rpc_notification(stdin: &mut impl Write, method: &str) -> Result<(), String> {
    let payload = serde_json::json!({
        "method": method,
        "params": {},
    });
    write_json_line(
        stdin,
        &payload,
        "Codex app-server notification encode failed",
        "Codex app-server notification write failed",
    )
}

#[derive(Debug, Deserialize)]
struct GrokBillingResponse {
    #[serde(rename = "billingCycle")]
    billing_cycle: Option<GrokBillingCycle>,
    #[serde(rename = "monthlyLimit")]
    monthly_limit: Option<GrokCent>,
    #[serde(rename = "onDemandCap")]
    on_demand_cap: Option<GrokCent>,
    #[serde(rename = "on_demand_enabled")]
    on_demand_enabled: Option<bool>,
    usage: Option<GrokBillingUsage>,
}

#[derive(Debug, Deserialize)]
struct GrokBillingCycle {
    #[serde(rename = "billingPeriodStart")]
    billing_period_start: Option<String>,
    #[serde(rename = "billingPeriodEnd")]
    billing_period_end: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GrokBillingUsage {
    #[serde(rename = "includedUsed")]
    included_used: Option<GrokCent>,
    #[serde(rename = "onDemandUsed")]
    on_demand_used: Option<GrokCent>,
    #[serde(rename = "totalUsed")]
    total_used: Option<GrokCent>,
}

#[derive(Debug, Deserialize)]
struct GrokCent {
    val: Option<i64>,
}

#[derive(Debug)]
enum GrokBillingSnapshot {
    Rpc(GrokBillingResponse),
    Web(GrokWebBillingSnapshot),
}

#[derive(Debug)]
struct GrokWebBillingSnapshot {
    used_percent: f64,
    reset_at_epoch: Option<i64>,
}

impl GrokBillingSnapshot {
    fn buckets(&self, now: i64) -> Vec<QuotaBucketView> {
        match self {
            Self::Rpc(response) => response.buckets(now),
            Self::Web(snapshot) => snapshot.buckets(now),
        }
    }

    fn source(&self) -> UsageSource {
        match self {
            Self::Rpc(_) => UsageSource::Cli,
            Self::Web(_) => UsageSource::ProviderApi,
        }
    }
}

impl GrokWebBillingSnapshot {
    fn buckets(&self, now: i64) -> Vec<QuotaBucketView> {
        let label = self.reset_at_epoch.map_or("Credits", |reset_at| {
            grok_cycle_label_from_reset(reset_at, now)
        });
        // Grok exposes only a billing cycle (no session), so it fills the Weekly
        // headline slot.
        let mut view = timed_bucket(
            label,
            None,
            None,
            Some(100u8.saturating_sub(self.used_percent.round() as u8)),
            self.reset_at_epoch,
            now,
            None,
            UsageSnapshotStatus::Fresh,
        );
        view.status_slot = Some(StatusSlot::Weekly);
        vec![view]
    }
}

impl GrokBillingResponse {
    fn buckets(&self, now: i64) -> Vec<QuotaBucketView> {
        let mut buckets = Vec::new();
        if let Some(limit) = self.monthly_limit.as_ref().and_then(|amount| amount.val) {
            let reset_at = self.billing_period_end_epoch();
            let label = self
                .billing_period_minutes()
                .map_or("Credits", grok_cycle_label_from_minutes);
            let total_used = self
                .usage
                .as_ref()
                .and_then(|usage| usage.total_used.as_ref())
                .and_then(|amount| amount.val)
                .unwrap_or(0);
            let used_percent = if limit > 0 {
                Some(((total_used as f64 / limit as f64) * 100.0).clamp(0.0, 100.0))
            } else {
                None
            };
            // Billing cycle fills the Weekly slot; Grok has no session window.
            buckets.push(with_status_slot(
                timed_bucket(
                    label,
                    Some(format_cents(total_used)),
                    Some(format_cents(limit)),
                    used_percent.map(|used| 100u8.saturating_sub(used.round() as u8)),
                    reset_at,
                    now,
                    None,
                    UsageSnapshotStatus::Fresh,
                ),
                Some(StatusSlot::Weekly),
            ));
        }
        if let Some(usage) = &self.usage
            && let Some(included) = usage.included_used.as_ref().and_then(|amount| amount.val)
            && included > 0
        {
            buckets.push(bucket(
                "Included usage",
                Some(format_cents(included)),
                None,
                None,
                None,
                Some("used this cycle"),
                UsageSnapshotStatus::Fresh,
            ));
        }
        if let Some(usage) = &self.usage
            && let Some(on_demand) = usage.on_demand_used.as_ref().and_then(|amount| amount.val)
            && on_demand > 0
        {
            buckets.push(bucket(
                "On-demand usage",
                Some(format_cents(on_demand)),
                self.on_demand_cap
                    .as_ref()
                    .and_then(|amount| amount.val)
                    .map(format_cents),
                None,
                None,
                self.on_demand_enabled
                    .unwrap_or(false)
                    .then_some("enabled")
                    .or(Some("disabled")),
                UsageSnapshotStatus::Fresh,
            ));
        }
        buckets
    }

    fn billing_period_end_epoch(&self) -> Option<i64> {
        parse_iso_epoch(self.billing_cycle.as_ref()?.billing_period_end.as_deref()?)
    }

    fn billing_period_minutes(&self) -> Option<i64> {
        let cycle = self.billing_cycle.as_ref()?;
        let start = parse_iso_epoch(cycle.billing_period_start.as_deref()?)?;
        let end = parse_iso_epoch(cycle.billing_period_end.as_deref()?)?;
        (end > start).then_some((end - start) / 60)
    }
}

fn grok_cycle_label_from_minutes(minutes: i64) -> &'static str {
    let days = minutes / (24 * 60);
    if (6..=8).contains(&days) {
        "Weekly"
    } else if (28..=31).contains(&days) {
        "Monthly"
    } else {
        "Credits"
    }
}

fn grok_cycle_label_from_reset(reset_at: i64, now: i64) -> &'static str {
    let days = reset_at.saturating_sub(now) / 86_400;
    if days <= 8 {
        "Weekly"
    } else if days <= 35 {
        "Monthly"
    } else {
        "Credits"
    }
}

fn fetch_grok_billing(
    auth_path: &Path,
    now: i64,
    gate: &mut ManagedCliLaunchGate,
) -> Result<GrokBillingSnapshot, String> {
    match fetch_grok_rpc_billing(gate) {
        Ok(response) => Ok(GrokBillingSnapshot::Rpc(response)),
        Err(rpc_error) => match fetch_grok_web_billing(auth_path, now) {
            Ok(snapshot) => {
                gate.record_success();
                Ok(GrokBillingSnapshot::Web(snapshot))
            }
            Err(web_error) => Err(format!(
                "{rpc_error}; Grok bearer billing failed: {web_error}"
            )),
        },
    }
}

fn fetch_grok_rpc_billing(gate: &mut ManagedCliLaunchGate) -> Result<GrokBillingResponse, String> {
    gate.can_launch("Grok ACP billing", Instant::now())?;
    let executable = grok_binary_path();
    let mut child = match Command::new(&executable)
        .args(["agent", "stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(err) => {
            let message = format!(
                "{} agent stdio failed to start: {err}",
                executable.display()
            );
            gate.record_launch_failure(message.clone());
            return Err(message);
        }
    };

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "grok agent stdio stdin unavailable".to_owned())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "grok agent stdio stdout unavailable".to_owned())?;
    let (tx, rx) = mpsc::channel();
    let reader = thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });

    let result: Result<GrokBillingResponse, String> = (|| {
        drop(grok_rpc_request(
            &mut stdin,
            &rx,
            1,
            "initialize",
            serde_json::json!({
                "protocolVersion": "1",
                "clientCapabilities": {
                    "fs": {
                        "readTextFile": false,
                        "writeTextFile": false
                    },
                    "terminal": false
                }
            }),
            GROK_RPC_INIT_TIMEOUT,
        )?);
        let billing_value = grok_rpc_request(
            &mut stdin,
            &rx,
            2,
            "x.ai/billing",
            serde_json::json!({}),
            GROK_RPC_REQUEST_TIMEOUT,
        )?;
        serde_json::from_value::<GrokBillingResponse>(billing_value)
            .map_err(|err| format!("Grok billing decode failed: {err}"))
    })();

    drop(stdin);
    drop(child.kill());
    drop(child.wait());
    drop(reader.join());
    if result.is_ok() {
        gate.record_success();
    } else if let Err(message) = &result {
        gate.record_launch_failure(message.clone());
    }
    result
}

fn grok_binary_path() -> PathBuf {
    let home_bin = home_path(".grok/bin/grok");
    if home_bin.is_file() {
        home_bin
    } else {
        PathBuf::from("grok")
    }
}

fn fetch_grok_web_billing(auth_path: &Path, now: i64) -> Result<GrokWebBillingSnapshot, String> {
    let token = grok_bearer_token(auth_path, now)?;
    let client = provider_http_client()?;
    let response = client
        .post("https://grok.com/grok_api_v2.GrokBuildBilling/GetGrokCreditsConfig")
        .bearer_auth(token)
        .header(reqwest::header::ORIGIN, "https://grok.com")
        .header(reqwest::header::REFERER, "https://grok.com/?_s=usage")
        .header(reqwest::header::ACCEPT, "*/*")
        .header(reqwest::header::CONTENT_TYPE, "application/grpc-web+proto")
        .header("x-grpc-web", "1")
        .header("x-user-agent", "connect-es/2.1.1")
        .header(reqwest::header::USER_AGENT, "jackin-capsule")
        .body(vec![0, 0, 0, 0, 0])
        .send()
        .map_err(|err| format!("request failed: {err}"))?;
    let status = response.status();
    let grpc_status = response
        .headers()
        .get("grpc-status")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let grpc_message = response
        .headers()
        .get("grpc-message")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let body = response
        .bytes()
        .map_err(|err| format!("body read failed: {err}"))?;
    if !status.is_success() {
        return Err(format!("HTTP {status}"));
    }
    if let Some(grpc_status) = grpc_status
        && grpc_status != "0"
    {
        return Err(format!(
            "gRPC status {grpc_status}: {}",
            grpc_message.unwrap_or_else(|| "unknown".to_owned())
        ));
    }
    parse_grok_web_billing_response(&body, now)
}

fn grok_bearer_token(auth_path: &Path, now: i64) -> Result<String, String> {
    let text = fs::read_to_string(auth_path).map_err(|err| format!("auth read failed: {err}"))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|err| format!("auth decode failed: {err}"))?;
    let Some(entries) = value.as_object() else {
        return Err("auth.json root is not an object".to_owned());
    };
    let mut legacy: Option<(&str, &serde_json::Value)> = None;
    for (scope, entry) in entries {
        let is_oidc = scope.starts_with("https://auth.x.ai::");
        let is_legacy = scope == "https://accounts.x.ai/sign-in" || scope.contains("/sign-in");
        if is_legacy {
            legacy = Some((scope, entry));
        }
        if is_oidc && let Some(token) = grok_bearer_token_from_entry(entry, now)? {
            return Ok(token);
        }
    }
    if let Some((_, entry)) = legacy
        && let Some(token) = grok_bearer_token_from_entry(entry, now)?
    {
        return Ok(token);
    }
    Err("no fresh Grok bearer token in auth.json".to_owned())
}

fn grok_bearer_token_from_entry(
    entry: &serde_json::Value,
    now: i64,
) -> Result<Option<String>, String> {
    let Some(token) = entry.get("key").and_then(serde_json::Value::as_str) else {
        return Ok(None);
    };
    if token.is_empty() {
        return Ok(None);
    }
    if let Some(expires_at) = entry.get("expires_at").and_then(serde_json::Value::as_str)
        && let Some(epoch) = parse_iso_epoch(expires_at)
        && epoch <= now
    {
        return Err("Grok bearer token is expired".to_owned());
    }
    Ok(Some(token.to_owned()))
}

fn parse_grok_web_billing_response(
    data: &[u8],
    now: i64,
) -> Result<GrokWebBillingSnapshot, String> {
    let mut payloads = grpc_web_data_frames(data);
    if payloads.is_empty() && looks_like_protobuf_payload(data) {
        payloads.push(data.to_vec());
    }
    if payloads.is_empty() {
        return Err("empty gRPC-web payload".to_owned());
    }
    let mut scan = ProtobufScan::default();
    for payload in payloads {
        scan.merge(scan_protobuf(&payload, 0, Vec::new(), &mut 0));
    }
    let used_percent = scan
        .fixed32_fields
        .iter()
        .filter(|field| {
            field.path.last() == Some(&1)
                && field.value.is_finite()
                && field.value >= 0.0
                && field.value <= 100.0
        })
        .min_by(|left, right| {
            left.path
                .len()
                .cmp(&right.path.len())
                .then_with(|| left.order.cmp(&right.order))
        })
        .map(|field| f64::from(field.value))
        .ok_or_else(|| "usage percent not found in Grok billing protobuf".to_owned())?;
    let reset_at_epoch = scan
        .varint_fields
        .iter()
        .filter(|field| field.value >= 1_700_000_000 && field.value <= 2_100_000_000)
        .filter_map(|field| i64::try_from(field.value).ok().map(|epoch| (field, epoch)))
        .filter(|(_, epoch)| *epoch > now)
        .min_by_key(|(field, epoch)| {
            let preferred = i32::from(field.path != [1, 5, 1]);
            (preferred, *epoch)
        })
        .map(|(_, epoch)| epoch);
    Ok(GrokWebBillingSnapshot {
        used_percent,
        reset_at_epoch,
    })
}

#[derive(Debug, Default)]
struct ProtobufScan {
    fixed32_fields: Vec<Fixed32Field>,
    varint_fields: Vec<VarintField>,
}

#[derive(Debug)]
struct Fixed32Field {
    path: Vec<u64>,
    value: f32,
    order: usize,
}

#[derive(Debug)]
struct VarintField {
    path: Vec<u64>,
    value: u64,
}

impl ProtobufScan {
    fn merge(&mut self, other: Self) {
        self.fixed32_fields.extend(other.fixed32_fields);
        self.varint_fields.extend(other.varint_fields);
    }
}

fn grpc_web_data_frames(data: &[u8]) -> Vec<Vec<u8>> {
    let mut frames = Vec::new();
    let mut index = 0;
    while index < data.len() {
        if index + 5 > data.len() {
            break;
        }
        let flags = data[index];
        let length = (usize::from(data[index + 1]) << 24)
            | (usize::from(data[index + 2]) << 16)
            | (usize::from(data[index + 3]) << 8)
            | usize::from(data[index + 4]);
        let start = index + 5;
        let end = start.saturating_add(length);
        if end > data.len() {
            break;
        }
        if flags & 0x80 == 0 {
            frames.push(data[start..end].to_vec());
        }
        index = end;
    }
    frames
}

fn looks_like_protobuf_payload(data: &[u8]) -> bool {
    let Some(first) = data.first() else {
        return false;
    };
    let field_number = first >> 3;
    let wire_type = first & 0x07;
    field_number > 0 && matches!(wire_type, 0 | 1 | 2 | 5)
}

fn scan_protobuf(data: &[u8], depth: usize, path: Vec<u64>, order: &mut usize) -> ProtobufScan {
    let mut scan = ProtobufScan::default();
    let mut index = 0;
    while index < data.len() {
        let field_start = index;
        let Some(key) = read_varint(data, &mut index) else {
            index = field_start.saturating_add(1);
            continue;
        };
        if key == 0 {
            index = field_start.saturating_add(1);
            continue;
        }
        let field_number = key >> 3;
        let wire_type = key & 0x07;
        let field_path = {
            let mut next = path.clone();
            next.push(field_number);
            next
        };
        match wire_type {
            0 => {
                if let Some(value) = read_varint(data, &mut index) {
                    scan.varint_fields.push(VarintField {
                        path: field_path,
                        value,
                    });
                } else {
                    index = field_start.saturating_add(1);
                }
            }
            1 => {
                index = index.saturating_add(8).min(data.len());
            }
            2 => {
                let Some(length) =
                    read_varint(data, &mut index).and_then(|v| usize::try_from(v).ok())
                else {
                    index = field_start.saturating_add(1);
                    continue;
                };
                let start = index;
                let end = start.saturating_add(length);
                if end > data.len() {
                    break;
                }
                if depth < 4 {
                    scan.merge(scan_protobuf(
                        &data[start..end],
                        depth + 1,
                        field_path,
                        order,
                    ));
                }
                index = end;
            }
            5 => {
                if index + 4 > data.len() {
                    break;
                }
                let bytes = [
                    data[index],
                    data[index + 1],
                    data[index + 2],
                    data[index + 3],
                ];
                index += 4;
                let value = f32::from_le_bytes(bytes);
                let current_order = *order;
                *order = order.saturating_add(1);
                scan.fixed32_fields.push(Fixed32Field {
                    path: field_path,
                    value,
                    order: current_order,
                });
            }
            _ => {
                index = field_start.saturating_add(1);
            }
        }
    }
    scan
}

fn read_varint(data: &[u8], index: &mut usize) -> Option<u64> {
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

fn grok_rpc_request(
    stdin: &mut impl Write,
    rx: &mpsc::Receiver<String>,
    id: i64,
    method: &str,
    params: serde_json::Value,
    timeout: Duration,
) -> Result<serde_json::Value, String> {
    let payload = grok_rpc_request_payload(id, method, params);
    write_json_line(
        stdin,
        &payload,
        "Grok RPC request encode failed",
        "Grok RPC request write failed",
    )?;

    let started = Instant::now();
    loop {
        let remaining = timeout
            .checked_sub(started.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));
        if remaining.is_zero() {
            return Err(format!("Grok RPC timed out waiting for {method}"));
        }
        let line = rx
            .recv_timeout(remaining)
            .map_err(|_| format!("Grok RPC timed out waiting for {method}"))?;
        let value: serde_json::Value =
            serde_json::from_str(&line).map_err(|err| format!("Grok RPC decode failed: {err}"))?;
        if value.get("id").and_then(serde_json::Value::as_i64) != Some(id) {
            continue;
        }
        if let Some(error) = value.get("error") {
            let message = error
                .get("message")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown error");
            return Err(format!("Grok RPC {method} failed: {message}"));
        }
        return value
            .get("result")
            .cloned()
            .ok_or_else(|| format!("Grok RPC {method} response missing result"));
    }
}

fn grok_rpc_request_payload(id: i64, method: &str, params: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    })
}

fn write_json_line(
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

fn fetch_codex_oauth_usage(
    credentials: &CodexOAuthCredentials,
    codex_home: &Path,
) -> Result<CodexUsageResponse, String> {
    let mut headers = vec![(reqwest::header::USER_AGENT, "jackin-capsule/usage")];
    if let Some(account_id) = &credentials.account_id {
        headers.push((
            reqwest::header::HeaderName::from_static("chatgpt-account-id"),
            account_id.as_str(),
        ));
    }
    get_json_bearer(
        "Codex OAuth usage",
        &resolve_codex_usage_url(codex_home),
        &credentials.access_token,
        &headers,
    )
}

fn fetch_codex_oauth_reset_credits(
    credentials: &CodexOAuthCredentials,
    codex_home: &Path,
) -> Result<CodexResetCredits, String> {
    let mut headers = vec![
        (reqwest::header::USER_AGENT, "jackin-capsule/usage"),
        (
            reqwest::header::HeaderName::from_static("openai-beta"),
            "codex-1",
        ),
        (
            reqwest::header::HeaderName::from_static("originator"),
            "Codex Desktop",
        ),
    ];
    if let Some(account_id) = &credentials.account_id {
        headers.push((
            reqwest::header::HeaderName::from_static("chatgpt-account-id"),
            account_id.as_str(),
        ));
    }
    let credits: CodexResetCredits = get_json_bearer(
        "Codex reset credits",
        &resolve_codex_reset_credits_url(codex_home),
        &credentials.access_token,
        &headers,
    )?;
    if credits.available_count < 0 {
        return Err("Codex reset credits invalid available count".to_owned());
    }
    Ok(credits)
}

fn resolve_codex_usage_url(codex_home: &Path) -> String {
    let normalized = resolve_codex_base_url(codex_home);
    let path = if normalized.contains("/backend-api") {
        "/wham/usage"
    } else {
        "/api/codex/usage"
    };
    format!("{normalized}{path}")
}

fn resolve_codex_reset_credits_url(codex_home: &Path) -> String {
    format!(
        "{}/wham/rate-limit-reset-credits",
        resolve_codex_base_url(codex_home)
    )
}

fn resolve_codex_base_url(codex_home: &Path) -> String {
    let config_path = codex_home.join("config.toml");
    let contents = match fs::read_to_string(&config_path) {
        Ok(contents) => Some(contents),
        Err(error) => {
            // A config.toml that exists but is unreadable silently drops the
            // operator's custom base-URL override back to the public default.
            if error.kind() != std::io::ErrorKind::NotFound {
                crate::clog!(
                    "codex config.toml read failed for {}: {error}",
                    config_path.display()
                );
            }
            None
        }
    };
    let base = contents
        .and_then(|contents| parse_chatgpt_base_url(&contents))
        .unwrap_or_else(|| "https://chatgpt.com/backend-api".to_owned());
    let mut normalized = base.trim().trim_end_matches('/').to_owned();
    if normalized.is_empty() {
        normalized = "https://chatgpt.com/backend-api".to_owned();
    }
    if (normalized.starts_with("https://chatgpt.com")
        || normalized.starts_with("https://chat.openai.com"))
        && !normalized.contains("/backend-api")
    {
        normalized.push_str("/backend-api");
    }
    normalized
}

fn parse_chatgpt_base_url(contents: &str) -> Option<String> {
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

fn provider_http_client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(PROVIDER_HTTP_TIMEOUT)
        .connect_timeout(PROVIDER_HTTP_TIMEOUT)
        .build()
        .map_err(|err| format!("provider HTTP client unavailable: {err}"))
}

/// Shared GET → bearer-auth → JSON skeleton for provider quota endpoints. The
/// caller supplies the human label (used verbatim in every error string so the
/// per-provider wording is unchanged), the URL, the bearer token, and any extra
/// request headers beyond the always-sent `Accept: application/json`. Per-
/// provider response validation stays at the call site.
fn get_json_bearer<T: serde::de::DeserializeOwned>(
    label: &str,
    url: &str,
    token: &str,
    extra_headers: &[(reqwest::header::HeaderName, &str)],
) -> Result<T, String> {
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
}

#[derive(Debug, Deserialize)]
struct ZaiQuotaResponse {
    code: Option<i64>,
    msg: Option<String>,
    success: Option<bool>,
    data: Option<ZaiQuotaData>,
}

#[derive(Debug, Deserialize)]
struct ZaiQuotaData {
    #[serde(default)]
    limits: Vec<ZaiLimitRaw>,
    #[serde(
        rename = "planName",
        alias = "plan",
        alias = "plan_type",
        alias = "packageName"
    )]
    plan_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ZaiLimitRaw {
    #[serde(rename = "type")]
    limit_type: String,
    unit: Option<i64>,
    number: Option<i64>,
    usage: Option<i64>,
    #[serde(rename = "currentValue")]
    current_value: Option<i64>,
    remaining: Option<i64>,
    percentage: Option<f64>,
    #[serde(rename = "nextResetTime")]
    next_reset_time: Option<i64>,
}

impl ZaiQuotaResponse {
    fn buckets(&self, now: i64) -> Vec<QuotaBucketView> {
        let mut limits = self
            .data
            .as_ref()
            .map(|data| data.limits.clone())
            .unwrap_or_default();
        limits.sort_by_key(|limit| limit.window_minutes().unwrap_or(i64::MAX));

        let mut token_limits = limits
            .iter()
            .filter(|limit| limit.limit_type == "TOKENS_LIMIT")
            .collect::<Vec<_>>();
        let time_limit = limits.iter().find(|limit| limit.limit_type == "TIME_LIMIT");
        let mut buckets = Vec::new();
        let mut session_token_limit = None;
        let mut primary_token_limit = None;
        if token_limits.len() >= 2 {
            token_limits.sort_by_key(|limit| limit.window_minutes().unwrap_or(i64::MAX));
            session_token_limit = token_limits.first().copied();
            primary_token_limit = token_limits.last().copied();
        } else if let Some(limit) = token_limits.first() {
            primary_token_limit = Some(*limit);
        }
        // render order is 5-hour (short/active), then Tokens, then MCP — an
        // operator override of CodexBar's Tokens, MCP, 5-hour order.
        if let Some(limit) = session_token_limit {
            buckets.push(with_status_slot(
                zai_bucket("5-hour", limit, now),
                Some(StatusSlot::Session),
            ));
        }
        if let Some(limit) = primary_token_limit {
            buckets.push(with_status_slot(
                zai_bucket("Tokens", limit, now),
                Some(StatusSlot::Weekly),
            ));
        }
        if let Some(limit) = time_limit {
            buckets.push(zai_bucket("MCP", limit, now));
        }
        buckets
    }

    fn plan_name(&self) -> Option<String> {
        self.data
            .as_ref()
            .and_then(|data| data.plan_name.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    }
}

impl ZaiLimitRaw {
    fn used_percent(&self) -> Option<u8> {
        if let Some(limit) = self.usage.filter(|limit| *limit > 0) {
            let used = if let Some(remaining) = self.remaining {
                let from_remaining = limit.saturating_sub(remaining);
                self.current_value
                    .map_or(from_remaining, |current| from_remaining.max(current))
            } else {
                self.current_value?
            };
            let percent = ((used.clamp(0, limit) as f64 / limit as f64) * 100.0)
                .round()
                .clamp(0.0, 100.0) as u8;
            return Some(percent);
        }
        self.percentage
            .map(|percent| percent.round().clamp(0.0, 100.0) as u8)
    }

    fn window_minutes(&self) -> Option<i64> {
        let number = self.number?;
        if number <= 0 {
            return None;
        }
        match self.unit {
            Some(5) => Some(number),
            Some(3) => Some(number * 60),
            Some(1) => Some(number * 24 * 60),
            Some(6) => Some(number * 7 * 24 * 60),
            _ => None,
        }
    }
}

fn zai_bucket(label: &str, limit: &ZaiLimitRaw, now: i64) -> QuotaBucketView {
    let used_percent = limit.used_percent();
    let remaining = used_percent.map(|used| 100u8.saturating_sub(used));
    let reset_at = limit.next_reset_time.map(|epoch_ms| epoch_ms / 1000);
    let detail = if label == "MCP" {
        zai_count_line(limit)
    } else {
        None
    };
    timed_bucket(
        label,
        limit
            .current_value
            .map(|value| compact_count(value.max(0) as u64)),
        limit.usage.map(|value| compact_count(value.max(0) as u64)),
        remaining,
        reset_at,
        now,
        detail.as_deref(),
        UsageSnapshotStatus::Fresh,
    )
}

fn zai_count_line(limit: &ZaiLimitRaw) -> Option<String> {
    let total = limit.usage.filter(|value| *value > 0)?;
    let used = if let Some(remaining) = limit.remaining {
        let from_remaining = total.saturating_sub(remaining);
        limit
            .current_value
            .map_or(from_remaining, |current| from_remaining.max(current))
    } else {
        limit.current_value?
    }
    .clamp(0, total);
    let remaining = total.saturating_sub(used);
    Some(format!(
        "{} / {} ({} remaining)",
        compact_count(used as u64),
        compact_count(total as u64),
        compact_count(remaining as u64)
    ))
}

fn fetch_zai_usage(token: &str) -> Result<ZaiQuotaResponse, String> {
    let url = resolve_zai_quota_url();
    let quota: ZaiQuotaResponse = get_json_bearer("Z.AI quota", &url, token, &[])?;
    if quota.success == Some(false) || quota.code.is_some_and(|code| code != 200) {
        return Err(format!(
            "Z.AI quota rejected response: {}",
            quota.msg.unwrap_or_else(|| "unknown error".to_owned())
        ));
    }
    Ok(quota)
}

fn resolve_zai_quota_url() -> String {
    let override_url = env_value("ZAI_QUOTA_URL").or_else(|| env_value("Z_AI_QUOTA_URL"));
    let host = env_value("ZAI_API_HOST")
        .or_else(|| env_value("Z_AI_API_HOST"))
        .unwrap_or_else(|| "https://api.z.ai".to_owned());
    resolve_zai_quota_url_from(override_url.as_deref(), Some(&host))
}

fn resolve_zai_quota_url_from(override_url: Option<&str>, host: Option<&str>) -> String {
    if let Some(url) = override_url {
        return normalize_url_or_host(url, "");
    }
    let host = host.unwrap_or("https://api.z.ai");
    normalize_url_or_host(&zai_quota_host(host), "api/monitor/usage/quota/limit")
}

fn zai_quota_host(value: &str) -> String {
    let normalized = normalize_url_or_host(value, "");
    let Ok(mut url) = url::Url::parse(&normalized) else {
        return normalized;
    };
    url.set_path("");
    url.set_query(None);
    url.set_fragment(None);
    url.to_string().trim_end_matches('/').to_owned()
}

#[derive(Debug, Deserialize)]
struct KimiUsageResponse {
    #[serde(default)]
    usages: Vec<KimiUsageItem>,
    usage: Option<KimiUsageDetail>,
    #[serde(default)]
    limits: Vec<KimiRateLimit>,
}

#[derive(Debug, Deserialize)]
struct KimiUsageItem {
    scope: Option<String>,
    detail: KimiUsageDetail,
    #[serde(default)]
    limits: Vec<KimiRateLimit>,
}

#[derive(Debug, Clone, Deserialize)]
struct KimiUsageDetail {
    limit: String,
    used: Option<String>,
    remaining: Option<String>,
    #[serde(rename = "resetTime")]
    reset_time: Option<String>,
}

#[derive(Debug, Deserialize)]
struct KimiRateLimit {
    window: Option<KimiWindow>,
    detail: KimiUsageDetail,
}

#[derive(Debug, Deserialize)]
struct KimiWindow {
    duration: Option<i64>,
    #[serde(rename = "timeUnit")]
    time_unit: Option<String>,
}

impl KimiUsageResponse {
    fn buckets(&self, now: i64) -> Vec<QuotaBucketView> {
        let (detail, limits) = if let Some(detail) = &self.usage {
            (detail, self.limits.as_slice())
        } else if let Some(usage) = self
            .usages
            .iter()
            .find(|usage| usage.scope.as_deref() == Some("FEATURE_CODING"))
            .or_else(|| self.usages.first())
        {
            (&usage.detail, usage.limits.as_slice())
        } else {
            return Vec::new();
        };
        // rate (short/active) window on top, then Weekly — an operator
        // override of CodexBar's Weekly, Rate Limit order.
        let mut buckets = Vec::new();
        if let Some(rate_limit) = limits.first() {
            buckets.push(with_status_slot(
                kimi_bucket(
                    "Rate Limit",
                    &rate_limit.detail,
                    rate_limit.window.as_ref(),
                    now,
                ),
                Some(StatusSlot::Session),
            ));
        }
        buckets.push(with_status_slot(
            kimi_bucket("Weekly", detail, None, now),
            Some(StatusSlot::Weekly),
        ));
        buckets
    }
}

impl KimiUsageDetail {
    fn limit_value(&self) -> Option<i64> {
        self.limit.trim().parse().ok()
    }

    fn used_value(&self) -> Option<i64> {
        self.used
            .as_deref()
            .and_then(|value| value.trim().parse().ok())
    }

    fn remaining_value(&self) -> Option<i64> {
        self.remaining
            .as_deref()
            .and_then(|value| value.trim().parse().ok())
    }

    fn used_percent(&self) -> Option<u8> {
        let limit = self.limit_value()?.max(0);
        if limit == 0 {
            return None;
        }
        let used = self.used_value().or_else(|| {
            self.remaining_value()
                .map(|remaining| limit.saturating_sub(remaining))
        })?;
        Some(((used.clamp(0, limit) as f64 / limit as f64) * 100.0).round() as u8)
    }
}

impl KimiWindow {
    fn seconds(&self) -> Option<i64> {
        let duration = self.duration?;
        let unit = self
            .time_unit
            .as_deref()
            .unwrap_or("hour")
            .to_ascii_lowercase();
        if unit.contains("minute") {
            Some(duration * 60)
        } else if unit.contains("hour") {
            Some(duration * 60 * 60)
        } else if unit.contains("day") {
            Some(duration * 24 * 60 * 60)
        } else if unit.contains("week") {
            Some(duration * 7 * 24 * 60 * 60)
        } else {
            None
        }
    }
}

fn kimi_bucket(
    label: &str,
    detail: &KimiUsageDetail,
    window: Option<&KimiWindow>,
    now: i64,
) -> QuotaBucketView {
    let limit = detail.limit_value();
    let used = detail.used_value().or_else(|| {
        limit.and_then(|limit| {
            detail
                .remaining_value()
                .map(|remaining| limit.saturating_sub(remaining))
        })
    });
    let used_percent = detail.used_percent();
    let remaining = used_percent.map(|used| 100u8.saturating_sub(used));
    let reset_at = detail.reset_time.as_deref().and_then(parse_iso_epoch);
    let window_seconds = kimi_window_seconds(label, window);
    let pace = quota_pace_label(remaining, reset_at, window_seconds, now);
    timed_bucket(
        label,
        used.map(|value| compact_count(value.max(0) as u64)),
        limit.map(|value| compact_count(value.max(0) as u64)),
        remaining,
        reset_at,
        now,
        pace.as_deref(),
        UsageSnapshotStatus::Fresh,
    )
}

fn kimi_window_seconds(label: &str, window: Option<&KimiWindow>) -> Option<i64> {
    (label == "Rate Limit")
        .then(|| window.and_then(KimiWindow::seconds))
        .flatten()
}

fn fetch_kimi_usage(token: &str) -> Result<KimiUsageResponse, String> {
    get_json_bearer(
        "Kimi usage",
        "https://api.kimi.com/coding/v1/usages",
        token,
        &[(reqwest::header::USER_AGENT, "jackin-capsule/usage")],
    )
}

fn load_kimi_local_token(now: i64) -> Option<String> {
    load_kimi_local_token_from_home(&home_path(""), now)
}

fn load_kimi_local_token_from_home(home: &Path, now: i64) -> Option<String> {
    [
        home.join(".kimi-code/credentials/kimi-code.json"),
        home.join(".kimi/credentials/kimi-code.json"),
    ]
    .into_iter()
    .find_map(|path| {
        let value = read_json_file(&path)?;
        kimi_local_token_from_value(&value, now)
    })
}

fn kimi_local_token_from_value(value: &serde_json::Value, now: i64) -> Option<String> {
    if let Some(expires_at) = value.get("expires_at").and_then(json_epoch_seconds)
        && expires_at <= now
    {
        return None;
    }
    value
        .get("access_token")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn json_epoch_seconds(value: &serde_json::Value) -> Option<i64> {
    let number = json_number(value)?;
    if number > 1_000_000_000_000.0 {
        Some((number / 1000.0).floor() as i64)
    } else {
        Some(number.floor() as i64)
    }
}

#[derive(Debug, Deserialize)]
struct MiniMaxUsageResponse {
    #[serde(rename = "base_resp")]
    base_resp: Option<MiniMaxBaseResponse>,
    data: Option<MiniMaxUsageData>,
    #[serde(rename = "model_remains", default)]
    root_model_remains: Vec<MiniMaxModelRemain>,
}

#[derive(Debug, Deserialize)]
struct MiniMaxBaseResponse {
    #[serde(rename = "status_code")]
    status_code: Option<i64>,
    #[serde(rename = "status_msg")]
    status_msg: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MiniMaxUsageData {
    #[serde(rename = "base_resp")]
    base_resp: Option<MiniMaxBaseResponse>,
    #[serde(rename = "current_subscribe_title")]
    current_subscribe_title: Option<String>,
    #[serde(rename = "plan_name")]
    plan_name: Option<String>,
    #[serde(rename = "combo_title")]
    combo_title: Option<String>,
    #[serde(rename = "current_plan_title")]
    current_plan_title: Option<String>,
    #[serde(rename = "current_combo_card")]
    current_combo_card: Option<MiniMaxComboCard>,
    #[serde(rename = "model_remains", default)]
    model_remains: Vec<MiniMaxModelRemain>,
}

#[derive(Debug, Deserialize)]
struct MiniMaxComboCard {
    title: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct MiniMaxModelRemain {
    #[serde(rename = "model_name")]
    model_name: Option<String>,
    #[serde(rename = "current_interval_total_count")]
    current_interval_total_count: Option<i64>,
    #[serde(rename = "current_interval_usage_count")]
    current_interval_usage_count: Option<i64>,
    #[serde(rename = "current_interval_remaining_percent")]
    current_interval_remaining_percent: Option<f64>,
    #[serde(rename = "current_interval_status")]
    current_interval_status: Option<i64>,
    #[serde(rename = "end_time")]
    end_time: Option<i64>,
    #[serde(rename = "remains_time")]
    remains_time: Option<i64>,
    #[serde(rename = "current_weekly_total_count")]
    current_weekly_total_count: Option<i64>,
    #[serde(rename = "current_weekly_usage_count")]
    current_weekly_usage_count: Option<i64>,
    #[serde(rename = "current_weekly_remaining_percent")]
    current_weekly_remaining_percent: Option<f64>,
    #[serde(rename = "current_weekly_status")]
    current_weekly_status: Option<i64>,
    #[serde(rename = "weekly_end_time")]
    weekly_end_time: Option<i64>,
    #[serde(rename = "weekly_remains_time")]
    weekly_remains_time: Option<i64>,
}

impl MiniMaxUsageResponse {
    fn validate(&self) -> Result<(), String> {
        let base = self
            .data
            .as_ref()
            .and_then(|data| data.base_resp.as_ref())
            .or(self.base_resp.as_ref());
        if let Some(status) = base.and_then(|base| base.status_code)
            && status != 0
        {
            return Err(base
                .and_then(|base| base.status_msg.clone())
                .unwrap_or_else(|| format!("status_code {status}")));
        }
        if self.model_remains().is_empty() {
            return Err("missing MiniMax coding plan data".to_owned());
        }
        Ok(())
    }

    fn buckets(&self, now: i64) -> Vec<QuotaBucketView> {
        let mut buckets = Vec::new();
        for remain in self.model_remains() {
            if let Some(bucket) = minimax_bucket(
                remain.model_name.as_deref().unwrap_or("MiniMax model"),
                MiniMaxWindow::Interval,
                remain.current_interval_total_count,
                remain.current_interval_usage_count,
                remain.current_interval_remaining_percent,
                remain.current_interval_status,
                remain.end_time,
                remain.remains_time,
                now,
            ) {
                buckets.push(bucket);
            }
            if minimax_is_general_model(remain.model_name.as_deref())
                && let Some(bucket) = minimax_bucket(
                    remain.model_name.as_deref().unwrap_or("MiniMax model"),
                    MiniMaxWindow::Weekly,
                    remain.current_weekly_total_count,
                    remain.current_weekly_usage_count,
                    remain.current_weekly_remaining_percent,
                    remain.current_weekly_status,
                    remain.weekly_end_time,
                    remain.weekly_remains_time,
                    now,
                )
            {
                buckets.push(bucket);
            }
        }
        buckets
    }

    fn plan_name(&self) -> Option<String> {
        let data = self.data.as_ref()?;
        [
            data.current_subscribe_title.as_deref(),
            data.plan_name.as_deref(),
            data.combo_title.as_deref(),
            data.current_plan_title.as_deref(),
            data.current_combo_card
                .as_ref()
                .and_then(|card| card.title.as_deref()),
        ]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|value| !value.is_empty())
        .map(str::to_owned)
    }

    fn model_remains(&self) -> Vec<&MiniMaxModelRemain> {
        if let Some(data) = &self.data
            && !data.model_remains.is_empty()
        {
            return data.model_remains.iter().collect();
        }
        self.root_model_remains.iter().collect()
    }
}

#[derive(Debug, Clone, Copy)]
enum MiniMaxWindow {
    Interval,
    Weekly,
}

#[allow(clippy::too_many_arguments)]
fn minimax_bucket(
    model_name: &str,
    window: MiniMaxWindow,
    total: Option<i64>,
    usage: Option<i64>,
    remaining_percent: Option<f64>,
    status: Option<i64>,
    end: Option<i64>,
    remains_time: Option<i64>,
    now: i64,
) -> Option<QuotaBucketView> {
    if matches!(status, Some(value) if !matches!(value, 0 | 1)) {
        return None;
    }
    if total.is_none() && usage.is_none() && remaining_percent.is_none() {
        return None;
    }
    let remaining_percent = if let Some(remaining_percent) = remaining_percent {
        Some(remaining_percent.round().clamp(0.0, 100.0) as u8)
    } else {
        let total = total?;
        if total <= 0 {
            None
        } else {
            let usage = usage?;
            Some(100u8.saturating_sub(
                ((usage.clamp(0, total) as f64 / total as f64) * 100.0).round() as u8,
            ))
        }
    };
    let used_label = usage.map(|usage| compact_count(usage.max(0) as u64));
    let reset_epoch = minimax_reset_epoch(end, remains_time, now);
    let detail = minimax_usage_count_line(usage, total, remaining_percent);
    // Only the general model fills the status-bar slots; per-model windows are
    // detail rows the headline ignores.
    let status_slot = match (minimax_is_general_model(Some(model_name)), window) {
        (true, MiniMaxWindow::Interval) => Some(StatusSlot::Session),
        (true, MiniMaxWindow::Weekly) => Some(StatusSlot::Weekly),
        _ => None,
    };
    let mut view = timed_bucket(
        &minimax_bucket_label(model_name, window),
        used_label,
        total
            .filter(|value| *value > 0)
            .map(|value| compact_count(value.max(0) as u64)),
        remaining_percent,
        reset_epoch,
        now,
        detail.as_deref(),
        UsageSnapshotStatus::Fresh,
    );
    view.status_slot = status_slot;
    Some(view)
}

fn minimax_is_general_model(model_name: Option<&str>) -> bool {
    model_name.is_some_and(|value| value.eq_ignore_ascii_case("general"))
}

fn minimax_bucket_label(model_name: &str, window: MiniMaxWindow) -> String {
    let model = titlecase_ascii(model_name);
    match (minimax_is_general_model(Some(model_name)), window) {
        (true, MiniMaxWindow::Interval) => "General · 5h".to_owned(),
        (true, MiniMaxWindow::Weekly) => "General · Weekly".to_owned(),
        (false, MiniMaxWindow::Interval) => model,
        (false, MiniMaxWindow::Weekly) => format!("{model} · Weekly"),
    }
}

fn titlecase_ascii(value: &str) -> String {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let mut out = String::new();
    out.extend(first.to_uppercase());
    out.push_str(chars.as_str());
    out
}

fn minimax_usage_count_line(
    usage: Option<i64>,
    total: Option<i64>,
    remaining_percent: Option<u8>,
) -> Option<String> {
    let usage = usage?.max(0) as u64;
    let total = total.filter(|value| *value > 0).map_or_else(
        || remaining_percent.map(|_| 100),
        |value| Some(value.max(0) as u64),
    )?;
    Some(format!(
        "Usage: {} / {}",
        compact_count(usage),
        compact_count(total)
    ))
}

fn fetch_minimax_usage(token: &str) -> Result<MiniMaxUsageResponse, String> {
    let client = provider_http_client()?;
    let mut last_error = None;
    for url in resolve_minimax_remains_urls() {
        let response = match client
            .get(&url)
            .bearer_auth(token)
            .header(reqwest::header::ACCEPT, "application/json")
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header("MM-API-Source", "jackin-capsule")
            .send()
        {
            Ok(response) => response,
            Err(err) => {
                last_error = Some(format!("MiniMax usage request failed for {url}: {err}"));
                continue;
            }
        };
        let status = response.status();
        if !status.is_success() {
            last_error = Some(format!("MiniMax usage HTTP {status}"));
            continue;
        }
        let usage = response
            .json::<MiniMaxUsageResponse>()
            .map_err(|err| format!("MiniMax usage decode failed: {err}"))?;
        usage.validate()?;
        return Ok(usage);
    }
    Err(last_error.unwrap_or_else(|| "MiniMax usage endpoint unavailable".to_owned()))
}

fn resolve_minimax_remains_urls() -> Vec<String> {
    let override_url = env_value("MINIMAX_REMAINS_URL");
    let host = env_value("MINIMAX_API_HOST").or_else(|| env_value("MINIMAX_HOST"));
    resolve_minimax_remains_urls_from(override_url.as_deref(), host.as_deref())
}

fn resolve_minimax_remains_urls_from(
    override_url: Option<&str>,
    host: Option<&str>,
) -> Vec<String> {
    if let Some(url) = override_url {
        return vec![normalize_url_or_host(url, "")];
    }
    let mut urls = Vec::new();
    if let Some(host) = host {
        let host = minimax_remains_host(host);
        let host = host.trim_end_matches('/');
        urls.push(format!("{host}/v1/token_plan/remains"));
        urls.push(format!("{host}/v1/api/openplatform/coding_plan/remains"));
    } else {
        urls.push("https://api.minimax.io/v1/token_plan/remains".to_owned());
        urls.push("https://api.minimax.io/v1/api/openplatform/coding_plan/remains".to_owned());
        urls.push("https://api.minimaxi.com/v1/token_plan/remains".to_owned());
        urls.push("https://api.minimaxi.com/v1/api/openplatform/coding_plan/remains".to_owned());
    }
    urls
}

fn minimax_remains_host(value: &str) -> String {
    let normalized = normalize_url_or_host(value, "");
    let Ok(mut url) = url::Url::parse(&normalized) else {
        return normalized;
    };
    url.set_path("");
    url.set_query(None);
    url.set_fragment(None);
    url.to_string().trim_end_matches('/').to_owned()
}

fn minimax_reset_epoch(end: Option<i64>, remains_time: Option<i64>, now: i64) -> Option<i64> {
    end.map(epoch_seconds_from_maybe_ms)
        .or_else(|| remains_time.map(|seconds| now.saturating_add(seconds.max(0))))
}

fn epoch_seconds_from_maybe_ms(value: i64) -> i64 {
    if value > 1_000_000_000_000 {
        value / 1000
    } else {
        value
    }
}

fn normalize_url_or_host(value: &str, suffix: &str) -> String {
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

fn env_value(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

/// Clamp a raw provider utilization into a `0..=100` "used" percentage.
///
/// Accepts both fraction form (`0.0..=1.0`) and already-percent form (`>1.0`).
/// Returns `None` for non-finite or negative inputs: several providers use a
/// negative sentinel (e.g. `-1`) for "unknown/unlimited", which must be omitted,
/// never fabricated into a full meter (`remaining_from_fraction(-0.5)` would
/// otherwise yield `Some(100)` — a "100% left" row for data that is absent).
fn used_percent_from_fraction(value: f64) -> Option<u8> {
    if !value.is_finite() || value < 0.0 {
        return None;
    }
    let used = if value <= 1.0 { value * 100.0 } else { value }
        .round()
        .clamp(0.0, 100.0) as u8;
    Some(used)
}

fn remaining_from_fraction(value: f64) -> Option<u8> {
    used_percent_from_fraction(value).map(|used| 100u8.saturating_sub(used))
}

fn used_percent_label(value: f64) -> Option<String> {
    used_percent_from_fraction(value).map(|used| format!("{used}% used"))
}

fn parse_iso_epoch(value: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|date| date.with_timezone(&Utc).timestamp())
}

fn reset_label(reset_at: i64, now: i64) -> String {
    if reset_at <= now {
        return "Resets now".to_owned();
    }
    format!(
        "Resets in {} ({})",
        compact_duration_label(reset_at.saturating_sub(now).max(0)),
        local_timestamp_label(reset_at)
    )
}

fn expiry_label(expires_at: i64, now: i64) -> String {
    if expires_at <= now {
        return "now".to_owned();
    }
    format!(
        "in {} ({})",
        compact_duration_label(expires_at.saturating_sub(now).max(0)),
        local_timestamp_label(expires_at)
    )
}

fn local_timestamp_label(epoch: i64) -> String {
    Local.timestamp_opt(epoch, 0).single().map_or_else(
        || "local time unavailable".to_owned(),
        |timestamp| timestamp.format("%b %-d, %H:%M").to_string(),
    )
}

fn quota_pace_label(
    remaining_percent: Option<u8>,
    reset_at: Option<i64>,
    window_seconds: Option<i64>,
    now: i64,
) -> Option<String> {
    let remaining_percent = f64::from(remaining_percent?);
    let reset_in = reset_at?.saturating_sub(now).max(0);
    let window_seconds = window_seconds?.max(1);
    if reset_in > window_seconds {
        return None;
    }
    let time_left_percent = reset_in as f64 / window_seconds as f64 * 100.0;
    // CodexBar pace model: compare remaining quota against the fraction of the
    // window still left. `delta > 0` means more quota than time remains (ahead
    // of pace = reserve); `delta < 0` means burning faster than the clock
    // (behind = deficit); within 2 points is "On pace". The reset countdown is
    // carried separately in the bucket's reset label, so the pace token stays a
    // bare phrase exactly as the previews show.
    let delta = remaining_percent - time_left_percent;
    if delta.abs() <= 2.0 {
        Some("On pace".to_owned())
    } else if delta > 0.0 {
        Some(format!("{}% in reserve", delta.round() as i64))
    } else {
        Some(format!("{}% in deficit", (-delta).round() as i64))
    }
}

fn compact_duration_label(seconds: i64) -> String {
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;
    if days > 0 {
        if hours > 0 {
            format!("{days}d {hours}h")
        } else {
            format!("{days}d")
        }
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

fn window_minutes_label(minutes: i64) -> Option<String> {
    if minutes <= 0 {
        return None;
    }
    if minutes % (7 * 24 * 60) == 0 {
        let weeks = minutes / (7 * 24 * 60);
        return Some(format!(
            "{weeks} week{} window",
            if weeks == 1 { "" } else { "s" }
        ));
    }
    if minutes % (24 * 60) == 0 {
        let days = minutes / (24 * 60);
        return Some(format!(
            "{days} day{} window",
            if days == 1 { "" } else { "s" }
        ));
    }
    if minutes % 60 == 0 {
        let hours = minutes / 60;
        return Some(format!(
            "{hours} hour{} window",
            if hours == 1 { "" } else { "s" }
        ));
    }
    Some(format!("{minutes} minute window"))
}

/// Split a machine-style identifier on `_`/`-`/whitespace and join the per-word
/// transform with spaces. Shared by `humanize_plan_label` (plain title-case) and
/// `codex_plan_display_name` (acronym-aware words).
fn humanize_words_with(value: &str, word: impl Fn(&str) -> String) -> String {
    value
        .split(|c: char| c == '_' || c == '-' || c.is_whitespace())
        .filter(|part| !part.is_empty())
        .map(word)
        .collect::<Vec<_>>()
        .join(" ")
}

fn humanize_plan_label(value: &str) -> String {
    humanize_words_with(value, titlecase_ascii)
}

fn codex_limit_label(value: &str) -> String {
    let lower = value.to_ascii_lowercase();
    if lower.contains("spark") {
        "Codex Spark".to_owned()
    } else {
        humanize_plan_label(value)
    }
}

fn json_number(value: &serde_json::Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
}

fn format_amount_with_unit(value: f64, unit: &str) -> String {
    let amount = if value.fract().abs() < f64::EPSILON {
        format!("{}", value as i64)
    } else {
        format!("{value:.2}")
    };
    format!("{amount} {unit}")
}

fn format_extra_usage_amount(value: f64, unit: &str) -> String {
    if unit.len() == 3 && unit.chars().all(|ch| ch.is_ascii_uppercase()) {
        format!("{unit} {value:.2}")
    } else {
        format_amount_with_unit(value, unit)
    }
}

#[derive(Debug, Clone, Default)]
struct AmpApiUsage {
    account_label: Option<String>,
    free_remaining: Option<f64>,
    free_limit: Option<f64>,
    hourly_replenishment: Option<f64>,
    individual_credits: Option<f64>,
}

impl AmpApiUsage {
    fn from_value(value: serde_json::Value) -> Option<Self> {
        let root = value.get("result").unwrap_or(&value);
        if let Some(display_text) = root.get("displayText").and_then(serde_json::Value::as_str)
            && let Some(usage) = parse_amp_usage_output(display_text)
        {
            return Some(Self::from_cli_usage(usage));
        }
        let usage = Self {
            account_label: first_string_key(root, "email")
                .or_else(|| first_string_key(root, "accountEmail"))
                .or_else(|| first_string_key(root, "userEmail")),
            free_remaining: first_number_key(root, "ampFreeRemaining")
                .or_else(|| first_number_key(root, "freeRemaining"))
                .or_else(|| first_number_key(root, "remainingBalance")),
            free_limit: first_number_key(root, "ampFreeLimit")
                .or_else(|| first_number_key(root, "freeLimit"))
                .or_else(|| first_number_key(root, "limitBalance")),
            hourly_replenishment: first_number_key(root, "hourlyReplenishment")
                .or_else(|| first_number_key(root, "replenishmentRate")),
            individual_credits: first_number_key(root, "individualCredits")
                .or_else(|| first_number_key(root, "individualBalance")),
        };
        (usage.free_remaining.is_some()
            || usage.free_limit.is_some()
            || usage.individual_credits.is_some())
        .then_some(usage)
    }

    fn from_cli_usage(usage: AmpCliUsage) -> Self {
        Self {
            account_label: usage.account_label,
            free_remaining: usage.free_remaining,
            free_limit: usage.free_limit,
            hourly_replenishment: usage.hourly_replenishment,
            individual_credits: usage.individual_credits,
        }
    }

    fn buckets(&self, now: i64) -> Vec<QuotaBucketView> {
        let mut buckets = Vec::new();
        if let (Some(remaining), Some(limit)) = (self.free_remaining, self.free_limit) {
            let used = (limit - remaining).max(0.0);
            let remaining_percent = if limit > 0.0 {
                Some(((remaining / limit) * 100.0).round().clamp(0.0, 100.0) as u8)
            } else {
                None
            };
            buckets.push(bucket(
                "Amp Free",
                Some(format_currency(used)),
                Some(format_currency(limit)),
                remaining_percent,
                amp_free_reset_label(remaining, limit, self.hourly_replenishment, now),
                None,
                UsageSnapshotStatus::Fresh,
            ));
        }
        if let Some(credits) = self.individual_credits {
            buckets.push(bucket(
                "Individual credits",
                None,
                Some(format_currency(credits)),
                None,
                None,
                Some(&format!("Individual credits: {}", format_currency(credits))),
                UsageSnapshotStatus::Fresh,
            ));
        }
        buckets
    }
}

fn fetch_amp_api_usage(token: &str) -> Result<AmpApiUsage, String> {
    let client = provider_http_client()?;
    let response = client
        .post("https://ampcode.com/api/internal?userDisplayBalanceInfo")
        .bearer_auth(token)
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .json(&serde_json::json!({
            "method": "userDisplayBalanceInfo",
            "params": {}
        }))
        .send()
        .map_err(|err| format!("Amp usage request failed: {err}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("Amp usage HTTP {status}"));
    }
    let value = response
        .json::<serde_json::Value>()
        .map_err(|err| format!("Amp usage decode failed: {err}"))?;
    AmpApiUsage::from_value(value)
        .ok_or_else(|| "Amp usage response did not include balance info".to_owned())
}

fn load_amp_api_key(path: &Path) -> Option<String> {
    let value = read_json_file(path)?;
    value
        .as_object()?
        .iter()
        .find_map(|(key, value)| {
            key.starts_with("apiKey@")
                .then(|| value.as_str())
                .flatten()
                .map(str::trim)
                .filter(|token| !token.is_empty())
                .map(ToOwned::to_owned)
        })
        .or_else(|| {
            value
                .as_object()?
                .values()
                .filter_map(|value| value.as_str().map(str::trim))
                .find(|token| !token.is_empty())
                .map(ToOwned::to_owned)
        })
}

#[derive(Debug, Clone, Default)]
struct AmpCliUsage {
    account_label: Option<String>,
    free_remaining: Option<f64>,
    free_limit: Option<f64>,
    hourly_replenishment: Option<f64>,
    individual_credits: Option<f64>,
}

impl AmpCliUsage {
    fn buckets(&self, now: i64) -> Vec<QuotaBucketView> {
        let mut buckets = Vec::new();
        if let (Some(remaining), Some(limit)) = (self.free_remaining, self.free_limit) {
            let used = (limit - remaining).max(0.0);
            let remaining_percent = if limit > 0.0 {
                Some(((remaining / limit) * 100.0).round().clamp(0.0, 100.0) as u8)
            } else {
                None
            };
            buckets.push(bucket(
                "Amp Free",
                Some(format_currency(used)),
                Some(format_currency(limit)),
                remaining_percent,
                amp_free_reset_label(remaining, limit, self.hourly_replenishment, now),
                None,
                UsageSnapshotStatus::Fresh,
            ));
        }
        if let Some(credits) = self.individual_credits {
            buckets.push(bucket(
                "Individual credits",
                None,
                Some(format_currency(credits)),
                None,
                None,
                Some(&format!("Individual credits: {}", format_currency(credits))),
                UsageSnapshotStatus::Fresh,
            ));
        }
        buckets
    }
}

fn amp_free_reset_label(
    remaining: f64,
    limit: f64,
    hourly_replenishment: Option<f64>,
    now: i64,
) -> Option<String> {
    let hourly_replenishment = hourly_replenishment?;
    if !remaining.is_finite()
        || !limit.is_finite()
        || !hourly_replenishment.is_finite()
        || remaining >= limit
        || hourly_replenishment <= 0.0
    {
        return None;
    }
    let seconds = (((limit - remaining).max(0.0) / hourly_replenishment) * 3_600.0).ceil() as i64;
    let reset_at = now.saturating_add(seconds);
    Some(format!(
        "Resets in {} ({})",
        compact_duration_label(seconds),
        local_timestamp_label(reset_at)
    ))
}

fn fetch_amp_cli_usage() -> Result<AmpCliUsage, String> {
    let output = run_cli_with_timeout("amp", &["--no-color", "usage"], PROVIDER_CLI_TIMEOUT)?;
    parse_amp_usage_output(&output)
        .ok_or_else(|| "Amp CLI usage output was not recognized".to_owned())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ClaudeUsageDiagnostic {
    pub command: String,
    pub args: Vec<String>,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub fetched_at_epoch: i64,
}

pub(crate) fn run_claude_usage_diagnostic() -> Result<ClaudeUsageDiagnostic, String> {
    run_claude_usage_diagnostic_with(|command, args, timeout| {
        run_cli_with_timeout_full(command, args, timeout)
    })
}

fn fetch_claude_cli_usage() -> Result<ClaudeCliUsage, String> {
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

fn run_claude_usage_diagnostic_with<F>(mut runner: F) -> Result<ClaudeUsageDiagnostic, String>
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

fn parse_amp_usage_output(text: &str) -> Option<AmpCliUsage> {
    let mut usage = AmpCliUsage::default();
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if let Some(rest) = line.strip_prefix("Signed in as ") {
            usage.account_label = Some(rest.split(" - ").next().unwrap_or(rest).trim().to_owned());
            continue;
        }
        if let Some(rest) = line.strip_prefix("Amp Free:") {
            let amounts = dollar_amounts(rest);
            if amounts.len() >= 2 {
                usage.free_remaining = Some(amounts[0]);
                usage.free_limit = Some(amounts[1]);
            }
            if let Some(replenishment) = rest
                .split("replenishes")
                .nth(1)
                .and_then(|value| dollar_amounts(value).first().copied())
            {
                usage.hourly_replenishment = Some(replenishment);
            }
            continue;
        }
        if line.starts_with("Individual credits:") {
            usage.individual_credits = dollar_amounts(line).first().copied();
        }
    }
    (usage.free_remaining.is_some() || usage.individual_credits.is_some()).then_some(usage)
}

fn parse_claude_usage_output(text: &str) -> Option<ClaudeCliUsage> {
    let mut usage = ClaudeCliUsage::default();
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if line.starts_with("Current session:") {
            usage.session_used = percent_before_used(line);
        } else if line.starts_with("Current week (all models):") {
            usage.weekly_used = percent_before_used(line);
        } else if line.starts_with("Current week (Sonnet only):") {
            usage.sonnet_used = percent_before_used(line);
        }
    }
    (usage.session_used.is_some() || usage.weekly_used.is_some() || usage.sonnet_used.is_some())
        .then_some(usage)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CliOutput {
    success: bool,
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
}

fn run_cli_with_timeout(command: &str, args: &[&str], timeout: Duration) -> Result<String, String> {
    let output = run_cli_with_timeout_full(command, args, timeout)?;
    if !output.success {
        return Err(format!(
            "{command} exited with status {:?}",
            output.exit_code
        ));
    }
    Ok(output.stdout)
}

#[allow(clippy::disallowed_methods)]
fn run_cli_with_timeout_full(
    command: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<CliOutput, String> {
    let mut child = Command::new(command)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("{command} failed to start: {err}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| format!("{command} stdout unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| format!("{command} stderr unavailable"))?;
    let stdout_reader = thread::spawn(move || read_process_pipe(stdout));
    let stderr_reader = thread::spawn(move || read_process_pipe(stderr));
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return collect_cli_output(command, Some(status), stdout_reader, stderr_reader);
            }
            Ok(None) if started.elapsed() >= timeout => {
                drop(child.kill());
                drop(child.wait());
                return Err(format!("{command} timed out after {}s", timeout.as_secs()));
            }
            Ok(None) => thread::sleep(Duration::from_millis(50)),
            Err(err) if err.raw_os_error() == Some(nix::errno::Errno::ECHILD as i32) => {
                return collect_cli_output(command, None, stdout_reader, stderr_reader);
            }
            Err(err) => {
                drop(child.kill());
                drop(child.wait());
                return Err(format!("{command} status failed: {err}"));
            }
        }
    }
}

fn collect_cli_output(
    command: &str,
    status: Option<ExitStatus>,
    stdout_reader: thread::JoinHandle<Result<String, String>>,
    stderr_reader: thread::JoinHandle<Result<String, String>>,
) -> Result<CliOutput, String> {
    let stdout = stdout_reader
        .join()
        .map_err(|_| format!("{command} stdout reader panicked"))?;
    let stderr = stderr_reader
        .join()
        .map_err(|_| format!("{command} stderr reader panicked"))?;
    Ok(CliOutput {
        success: status.is_none_or(|status| status.success()),
        exit_code: status.and_then(|status| status.code()),
        stdout: stdout?,
        stderr: stderr?,
    })
}

fn read_process_pipe(mut pipe: impl Read) -> Result<String, String> {
    let mut bytes = Vec::new();
    pipe.read_to_end(&mut bytes)
        .map_err(|err| format!("process output read failed: {err}"))?;
    String::from_utf8(bytes).map_err(|err| format!("process output was not UTF-8: {err}"))
}

fn dollar_amounts(text: &str) -> Vec<f64> {
    let mut values = Vec::new();
    let mut rest = text;
    while let Some(index) = rest.find('$') {
        rest = &rest[index + 1..];
        let amount: String = rest
            .chars()
            .take_while(|ch| ch.is_ascii_digit() || matches!(ch, '.' | ','))
            .filter(|ch| *ch != ',')
            .collect();
        if let Ok(value) = amount.parse() {
            values.push(value);
        }
    }
    values
}

fn percent_before_used(text: &str) -> Option<f64> {
    let before_used = text.split("% used").next()?;
    let percent = before_used
        .rsplit(|ch: char| !(ch.is_ascii_digit() || ch == '.'))
        .find(|part| !part.is_empty())?;
    percent.parse().ok()
}

fn format_currency(value: f64) -> String {
    if value.fract().abs() < f64::EPSILON {
        format!("${value:.0}")
    } else {
        format!("${value:.2}")
    }
}

fn format_cents(value: i64) -> String {
    format_currency(value as f64 / 100.0)
}

fn codex_account_from_value(value: &serde_json::Value) -> Option<String> {
    value
        .pointer("/tokens/email")
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            value
                .pointer("/tokens/account_id")
                .and_then(serde_json::Value::as_str)
        })
        .or_else(|| value.get("auth_mode").and_then(serde_json::Value::as_str))
        .map(str::to_owned)
}

fn grok_account_label(path: &Path) -> Option<String> {
    let value = read_json_file(path)?;
    first_string_key(&value, "email")
        .or_else(|| first_string_key(&value, "user_id"))
        .or_else(|| first_string_key(&value, "team_id"))
}

fn grok_plan_label(path: &Path) -> Option<String> {
    let value = read_json_file(path)?;
    first_string_key(&value, "auth_mode").map(|mode| {
        if mode.eq_ignore_ascii_case("oidc") {
            "SuperGrok".to_owned()
        } else {
            mode
        }
    })
}

fn grok_account_label_or_presence(
    auth_path: &Path,
    has_auth: bool,
    has_xai_api_key: bool,
    has_deployment_key: bool,
) -> String {
    grok_account_label(auth_path).unwrap_or_else(|| {
        if has_auth {
            "local Grok auth".to_owned()
        } else if has_xai_api_key {
            "XAI_API_KEY present".to_owned()
        } else if has_deployment_key {
            "GROK_DEPLOYMENT_KEY present".to_owned()
        } else {
            "needs Grok login".to_owned()
        }
    })
}

fn first_string_key(value: &serde_json::Value, needle: &str) -> Option<String> {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(found) = map.get(needle).and_then(serde_json::Value::as_str) {
                return Some(found.to_owned());
            }
            map.values().find_map(|v| first_string_key(v, needle))
        }
        serde_json::Value::Array(values) => values.iter().find_map(|v| first_string_key(v, needle)),
        _ => None,
    }
}

fn first_number_key(value: &serde_json::Value, needle: &str) -> Option<f64> {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(found) = map.get(needle).and_then(json_number) {
                return Some(found);
            }
            map.values().find_map(|v| first_number_key(v, needle))
        }
        serde_json::Value::Array(values) => values.iter().find_map(|v| first_number_key(v, needle)),
        _ => None,
    }
}

fn home_path(rel: &str) -> PathBuf {
    let rel = rel.trim_start_matches('/');
    std::env::var("HOME")
        .map_or_else(|_| PathBuf::from("/home/agent"), PathBuf::from)
        .join(rel)
}

/// `Auth:` origin label for an OAuth credential resolved from `path`, with the
/// home dir collapsed to `~` (so it reads `~/.codex/auth.json`, not an absolute
/// container path). Shared by the Claude and Codex snapshots.
fn oauth_origin(path: &Path) -> String {
    // `to_string_lossy` borrows (no alloc) for the common UTF-8 path and only
    // allocates for non-UTF-8 container paths; `&Cow<str>` coerces to `&str`.
    format!(
        "OAuth · {}",
        jackin_tui::shorten_home(&path.to_string_lossy())
    )
}

fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

pub(crate) fn relative_updated_label(fetched_at: i64, now_epoch: i64) -> String {
    let age = now_epoch.saturating_sub(fetched_at).max(0);
    if age < 60 {
        "Updated just now".to_owned()
    } else if age < 3_600 {
        format!("Updated {}m ago", age / 60)
    } else {
        format!("Updated {}h ago", age / 3_600)
    }
}

fn refresh_cached_updated_label(view: &mut FocusedUsageView, now_epoch: i64) {
    if matches!(
        view.status,
        UsageSnapshotStatus::Fresh | UsageSnapshotStatus::Stale
    ) || view.updated_label.trim().is_empty()
    {
        view.updated_label = relative_updated_label(view.fetched_at_epoch, now_epoch);
    }
}

fn compact_count(value: u64) -> String {
    if value >= 1_000_000_000 {
        format!("{:.1}B", value as f64 / 1_000_000_000.0)
    } else if value >= 1_000_000 {
        format!("{:.1}M", value as f64 / 1_000_000.0)
    } else if value >= 1_000 {
        format!("{:.1}K", value as f64 / 1_000.0)
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests;
