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
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use chrono::{DateTime, Local, TimeZone, Utc};
use jackin_protocol::control::{
    AccountUsageSnapshotView, FocusedAccountHeader, FocusedUsageView, QuotaBucketView,
    UsageConfidence, UsageProviderTab, UsageSnapshotStatus, UsageSource,
};
use serde::{Deserialize, Serialize};

const PROVIDER_HTTP_TIMEOUT: Duration = Duration::from_secs(10);
const PROVIDER_CLI_TIMEOUT: Duration = Duration::from_secs(10);
const CODEX_RPC_INIT_TIMEOUT: Duration = Duration::from_secs(8);
const CODEX_RPC_REQUEST_TIMEOUT: Duration = Duration::from_secs(3);
const CODEX_RPC_LAUNCH_COOLDOWN: Duration = Duration::from_mins(30);
const CLAUDE_VERSION_TIMEOUT: Duration = Duration::from_secs(2);
const CLAUDE_CODE_USER_AGENT_FALLBACK: &str = "claude-code/2.1.0";
const GROK_RPC_INIT_TIMEOUT: Duration = Duration::from_secs(8);
const GROK_RPC_REQUEST_TIMEOUT: Duration = Duration::from_secs(12);
const MATERIALIZED_USAGE_ACCOUNTS_PATH: &str = "/jackin/run/usage/accounts.json";
pub(crate) const TELEMETRY_STORE_PATH: &str = "/jackin/state/usage/telemetry.db";

static MATERIALIZED_TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone)]
pub(crate) struct UsageCache {
    snapshots: HashMap<String, CachedUsage>,
    codex_rpc_gate: ManagedCliLaunchGate,
    grok_rpc_gate: ManagedCliLaunchGate,
    refresh_schedule: UsageRefreshSchedule,
    telemetry_store_path: PathBuf,
}

#[derive(Debug, Clone)]
struct CachedUsage {
    view: FocusedUsageView,
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
}

impl UsageCache {
    #[cfg(test)]
    pub(crate) fn set_telemetry_store_path(&mut self, path: PathBuf) {
        self.telemetry_store_path = path;
    }

    pub(crate) fn focused_status_bar_label(
        &self,
        focused_agent: Option<&str>,
        focused_provider: Option<&str>,
    ) -> Option<String> {
        let agent = focused_agent?;
        if let Some(view) = self.cached_focused_usage_view(agent, focused_provider) {
            return Some(view.status_bar_label);
        }
        if let Ok(Some(view)) = crate::telemetry_store::focused_usage_view(
            &self.telemetry_store_path,
            Some(agent),
            focused_provider,
            now_epoch(),
        ) {
            return Some(view.status_bar_label);
        }
        None
    }

    pub(crate) fn focused_snapshot(
        &mut self,
        focused_agent: Option<&str>,
        focused_provider: Option<&str>,
        provider_keys: &BTreeMap<jackin_protocol::Provider, String>,
        force_refresh: bool,
    ) -> FocusedUsageView {
        let Some(agent) = focused_agent else {
            if let Some(provider) = focused_provider {
                return cached_unavailable_view("usage", Some(provider), now_epoch());
            }
            return FocusedUsageView::unavailable("no focused agent session", now_epoch());
        };
        if !force_refresh {
            let now = now_epoch();
            if let Some(view) = self.cached_focused_usage_view(agent, focused_provider) {
                return view;
            }
            return match crate::telemetry_store::focused_usage_view(
                &self.telemetry_store_path,
                Some(agent),
                focused_provider,
                now,
            ) {
                Ok(Some(mut view)) => {
                    if view.focused_provider.is_none() {
                        view.focused_provider = focused_provider.map(str::to_owned);
                    }
                    mark_active_tab(&mut view);
                    view
                }
                Ok(None) => cached_unavailable_view(agent, focused_provider, now),
                Err(error) => {
                    let mut view = cached_unavailable_view(agent, focused_provider, now);
                    view.last_error = Some(format!(
                        "usage unavailable: telemetry store read failed: {error}"
                    ));
                    view
                }
            };
        }
        let cache_key = canonical_usage_cache_key(agent, focused_provider);
        let mut view = build_snapshot(
            agent,
            focused_provider,
            provider_keys,
            &mut self.codex_rpc_gate,
            &mut self.grok_rpc_gate,
        );
        if let Some(cached) = self.snapshots.get(&cache_key) {
            preserve_cached_quota_on_failed_refresh(&mut view, &cached.view);
        }
        enrich_provider_tabs(&mut view, &self.snapshots);
        self.snapshots
            .insert(cache_key, CachedUsage { view: view.clone() });
        if let Err(error) = self.materialize_accounts(now_epoch()) {
            crate::cdebug!("usage accounts materialization failed: {error}");
        }
        if let Err(error) =
            crate::telemetry_store::store_usage_snapshot(&self.telemetry_store_path, &view)
        {
            crate::cdebug!("usage telemetry store write failed: {error}");
        }
        if let Ok(Some(mut stored)) = crate::telemetry_store::focused_usage_view(
            &self.telemetry_store_path,
            Some(agent),
            focused_provider,
            now_epoch(),
        ) {
            mark_active_tab(&mut stored);
            return stored;
        }
        view
    }

    fn cached_focused_usage_view(
        &self,
        agent: &str,
        focused_provider: Option<&str>,
    ) -> Option<FocusedUsageView> {
        let cache_key = canonical_usage_cache_key(agent, focused_provider);
        let mut view = self.snapshots.get(&cache_key)?.view.clone();
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
        for target in targets {
            if !self.refresh_schedule.should_refresh(&target, now) {
                continue;
            }
            let view = self.focused_snapshot(
                Some(&target.agent),
                target.provider.as_deref(),
                provider_keys,
                true,
            );
            self.refresh_schedule.mark_refreshed(&target, now, &view);
        }
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

impl Default for UsageCache {
    fn default() -> Self {
        Self {
            snapshots: HashMap::new(),
            codex_rpc_gate: ManagedCliLaunchGate::default(),
            grok_rpc_gate: ManagedCliLaunchGate::default(),
            refresh_schedule: UsageRefreshSchedule::default(),
            telemetry_store_path: PathBuf::from(TELEMETRY_STORE_PATH),
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
    if fs::create_dir_all(cooldown_dir).is_err() {
        return;
    }
    let path = shared_usage_cooldown_marker_path(cooldown_dir, key);
    let reason = reason.replace('\n', " ");
    drop(fs::write(path, format!("{until_epoch}\n{reason}\n")));
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

pub(crate) fn resolved_usage_provider_label(
    agent: &str,
    focused_provider: Option<&str>,
) -> Option<&'static str> {
    let surface = resolve_surface(agent, focused_provider);
    (surface != UsageSurface::Unsupported).then_some(surface.label())
}

fn cached_unavailable_view(
    agent: &str,
    focused_provider: Option<&str>,
    now: i64,
) -> FocusedUsageView {
    let surface = resolve_surface(agent, focused_provider);
    let mut view =
        FocusedUsageView::unavailable("usage unavailable: no cached provider snapshot", now);
    view.focused_agent = Some(agent.to_owned());
    view.focused_provider = focused_provider
        .map(str::to_owned)
        .or_else(|| Some(surface.label().to_owned()));
    view.account.provider_label = surface.account_label().to_owned();
    view.tabs = provider_tabs(surface);
    view
}

fn mark_active_tab(view: &mut FocusedUsageView) {
    let provider = view.focused_provider.as_deref().unwrap_or_default();
    for tab in &mut view.tabs {
        tab.active = provider_matches_usage_label(&tab.label, provider)
            || provider_matches_usage_label(&tab.label, &view.account.provider_label);
    }
}

pub(crate) fn cached_account_snapshots() -> Vec<AccountUsageSnapshotView> {
    crate::telemetry_store::account_snapshot_views(Path::new(TELEMETRY_STORE_PATH)).unwrap_or_else(
        |error| {
            crate::cdebug!("usage account snapshot read failed: {error}");
            Vec::new()
        },
    )
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

fn claude_snapshot(agent: &str, provider: Option<&str>, now: i64) -> FocusedUsageView {
    let config =
        std::env::var("CLAUDE_CONFIG_DIR").map_or_else(|_| home_path(".claude"), PathBuf::from);
    // Claude Code stores the OAuth token in `<config>/.credentials.json`
    // (the file the runtime forwards from the host Keychain or a
    // workspace-pinned config dir). The legacy `~/.claude.json` only
    // carries `oauthAccount` metadata, never the access token, so it is a
    // last-resort read. Matches CodexBar's credential source order.
    let oauth = load_claude_oauth_credentials(&config.join(".credentials.json"))
        .or_else(|| load_claude_oauth_credentials(&home_path(".claude/.credentials.json")))
        .or_else(|| load_claude_oauth_credentials(&home_path(".claude.json")));
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .ok()
        .filter(|v| !v.is_empty());
    let auth_token = std::env::var("ANTHROPIC_AUTH_TOKEN")
        .ok()
        .filter(|v| !v.is_empty());
    let account = if api_key.is_some() {
        "ANTHROPIC_API_KEY".to_owned()
    } else if auth_token.is_some() {
        "ANTHROPIC_AUTH_TOKEN".to_owned()
    } else if oauth.is_some() {
        "Claude OAuth".to_owned()
    } else if config.join(".credentials.json").exists() {
        "local Claude credentials".to_owned()
    } else {
        "needs Claude login".to_owned()
    };
    let (oauth_quota, oauth_error) = match oauth.as_ref() {
        Some(credentials) => match fetch_claude_oauth_usage(&credentials.access_token) {
            Ok(usage) => (Some(usage), None),
            Err(error) => (None, Some(error)),
        },
        None => (None, None),
    };
    let (cli_usage, cli_error) = if oauth_quota.is_none() && oauth.is_some() {
        match fetch_claude_cli_usage() {
            Ok(usage) => (Some(usage), None),
            Err(error) => (None, Some(error)),
        }
    } else {
        (None, None)
    };
    let provider_error = if oauth_quota.is_some() || cli_usage.is_some() {
        None
    } else {
        oauth_error.as_ref().or(cli_error.as_ref()).cloned()
    };
    let status = if account == "needs Claude login" {
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
        plan_label: oauth.and_then(|credentials| credentials.subscription_type),
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
            UsageConfidence::Authoritative
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
            _ => None,
        },
    })
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
    let credentials = load_codex_oauth_credentials(&auth_path);
    let auth_account = credentials
        .as_ref()
        .and_then(|credentials| credentials.account_label.clone())
        .or_else(|| codex_account_label(&auth_path))
        .unwrap_or_else(|| {
            if std::env::var("OPENAI_API_KEY").is_ok_and(|v| !v.is_empty()) {
                "OPENAI_API_KEY".to_owned()
            } else {
                "needs Codex login".to_owned()
            }
        });
    let (rpc_usage, rpc_error) = match fetch_codex_rpc_usage(rpc_gate) {
        Ok(usage) => (Some(usage), None),
        Err(error) => (None, Some(error)),
    };
    let rpc_quota = rpc_usage.as_ref().map(|usage| &usage.response);
    let (oauth_quota, oauth_error) = match credentials.as_ref() {
        Some(credentials) => match fetch_codex_oauth_usage(credentials, &codex_home) {
            Ok(mut usage) => {
                usage.reset_credits =
                    fetch_codex_oauth_reset_credits(credentials, &codex_home).ok();
                (Some(usage), None)
            }
            Err(error) => (None, Some(error)),
        },
        None => (None, None),
    };
    let provider_error = rpc_error.as_ref().or(oauth_error.as_ref()).cloned();
    let quota = rpc_quota.or(oauth_quota.as_ref());
    let account = rpc_usage
        .as_ref()
        .and_then(|usage| usage.account_label.clone())
        .unwrap_or(auth_account);
    let status = if account == "needs Codex login" {
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
        plan_label: quota.and_then(|usage| usage.plan_type.clone()),
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
    let amp_api_key = env_value("AMP_API_KEY");
    let (api_usage, api_error) = match amp_api_key.as_deref() {
        Some(token) => match fetch_amp_api_usage(token) {
            Ok(usage) => (Some(usage), None),
            Err(error) => (None, Some(error)),
        },
        None => (None, None),
    };
    let (cli_usage, cli_error) = if api_usage.is_none() {
        match fetch_amp_cli_usage() {
            Ok(usage) => (Some(usage), None),
            Err(error) => (None, Some(error)),
        }
    } else {
        (None, None)
    };
    let provider_error = api_error.as_ref().or(cli_error.as_ref()).cloned();
    let has_auth = amp_api_key.is_some() || data.join("secrets.json").exists();
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
        plan_label: (api_usage.is_some() || cli_usage.is_some()).then_some("Amp Free".to_owned()),
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
    let auth = data.join("auth.json");
    let has_auth = auth.exists();
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
        plan_label: grok_plan_label(auth),
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
    let (provider_usage, provider_error) = match token {
        Some(token) => match fetch_kimi_usage(token) {
            Ok(usage) => (Some(usage), None),
            Err(error) => (None, Some(error)),
        },
        None => (None, None),
    };
    let status = if provider_usage.is_some() {
        UsageSnapshotStatus::Fresh
    } else if has_token || has_local {
        UsageSnapshotStatus::Unsupported
    } else {
        UsageSnapshotStatus::NeedsSecret
    };
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
        account_label: if has_token {
            "Kimi auth token"
        } else if has_local {
            "local Kimi config"
        } else {
            "needs Kimi auth"
        }
        .to_owned(),
        plan_label: None,
        buckets,
        status,
        source: if provider_usage.is_some() {
            UsageSource::ProviderApi
        } else {
            UsageSource::None
        },
        confidence: if provider_usage.is_some() {
            UsageConfidence::Authoritative
        } else if has_token || has_local {
            UsageConfidence::PresenceOnly
        } else {
            UsageConfidence::None
        },
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
    let (provider_usage, provider_error) = match token {
        Some(token) => match fetch_minimax_usage(token) {
            Ok(usage) => (Some(usage), None),
            Err(error) => (None, Some(error)),
        },
        None => (None, None),
    };
    let status = if provider_usage.is_some() {
        UsageSnapshotStatus::Fresh
    } else if has_token {
        UsageSnapshotStatus::Unsupported
    } else {
        UsageSnapshotStatus::NeedsSecret
    };
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
        account_label: if has_token {
            "MiniMax API token"
        } else {
            "needs MINIMAX_CODING_API_KEY"
        }
        .to_owned(),
        plan_label: provider_usage
            .as_ref()
            .and_then(MiniMaxUsageResponse::plan_name),
        buckets,
        status,
        source: if provider_usage.is_some() {
            UsageSource::ProviderApi
        } else {
            UsageSource::None
        },
        confidence: if provider_usage.is_some() {
            UsageConfidence::Authoritative
        } else if has_token {
            UsageConfidence::PresenceOnly
        } else {
            UsageConfidence::None
        },
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
    let (provider_quota, provider_error) = match (surface, key) {
        (UsageSurface::Zai, Some(token)) => match fetch_zai_usage(token) {
            Ok(quota) => (Some(quota), None),
            Err(error) => (None, Some(error)),
        },
        _ => (None, None),
    };
    let status = if provider_quota.is_some() {
        UsageSnapshotStatus::Fresh
    } else if has_key {
        UsageSnapshotStatus::Unsupported
    } else {
        UsageSnapshotStatus::NeedsSecret
    };
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
        account_label: if has_key {
            format!("{key_name} present")
        } else {
            format!("needs {key_name}")
        },
        plan_label: provider_quota
            .as_ref()
            .and_then(ZaiQuotaResponse::plan_name),
        buckets,
        status,
        source: if provider_quota.is_some() {
            UsageSource::ProviderApi
        } else {
            UsageSource::None
        },
        confidence: if provider_quota.is_some() {
            UsageConfidence::Authoritative
        } else if has_key {
            UsageConfidence::PresenceOnly
        } else {
            UsageConfidence::None
        },
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
        plan_label: None,
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
        plan_label: None,
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
    plan_label: Option<String>,
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
            plan_label: input.plan_label,
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
    match surface {
        UsageSurface::Amp => amp_status_bar_headline(buckets),
        _ => {
            let labels = status_bar_quota_labels(buckets);
            (!labels.is_empty()).then(|| labels.join(" · "))
        }
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
    ["Session", "Weekly"]
        .into_iter()
        .filter_map(|target| {
            buckets
                .iter()
                .find(|bucket| status_bar_bucket_matches(target, bucket))
                .and_then(|bucket| {
                    bucket
                        .remaining_percent
                        .map(|remaining| format!("{target} {remaining}%"))
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

fn status_bar_bucket_matches(target: &str, bucket: &QuotaBucketView) -> bool {
    if !status_bar_fresh_or_stale(bucket) {
        return false;
    }
    let label = bucket.label.to_ascii_lowercase();
    match target {
        "Session" => {
            label.contains("session") || label.contains("5-hour") || label.contains("5 hour")
        }
        "Weekly" => label.contains("weekly") || label.contains("week"),
        _ => false,
    }
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

fn provider_matches_usage_label(provider: &str, account_provider: &str) -> bool {
    let provider = provider.to_ascii_lowercase();
    let account_provider = account_provider.to_ascii_lowercase();
    provider == account_provider
        || provider.contains(&account_provider)
        || account_provider.contains(&provider)
        || (provider.contains("openai") && account_provider.contains("codex"))
        || (provider.contains("codex") && account_provider.contains("codex"))
        || (provider.contains("anthropic") && account_provider.contains("claude"))
        || (provider.contains("claude") && account_provider.contains("claude"))
        || (provider.contains("z.ai") && account_provider.contains("glm"))
        || (provider.contains("zai") && account_provider.contains("glm"))
        || (provider.contains("glm") && account_provider.contains("z.ai"))
        || (provider.contains("xai") && account_provider.contains("grok"))
        || (provider.contains("grok") && account_provider.contains("grok"))
        || (provider.contains("minimax") && account_provider.contains("minimax"))
        || (provider.contains("kimi") && account_provider.contains("kimi"))
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
        pace_label: pace_label.map(str::to_owned),
        status,
    }
}

#[derive(Debug, Clone)]
struct ClaudeOAuthCredentials {
    access_token: String,
    subscription_type: Option<String>,
}

fn load_claude_oauth_credentials(path: &Path) -> Option<ClaudeOAuthCredentials> {
    let text = fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
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

#[derive(Debug, Deserialize)]
struct ClaudeOAuthUsageResponse {
    #[serde(rename = "five_hour")]
    five_hour: Option<ClaudeOAuthUsageWindow>,
    #[serde(alias = "seven_day_oauth_apps")]
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
        push_claude_cli_bucket(&mut buckets, "Session", self.session_used);
        push_claude_cli_bucket(&mut buckets, "Weekly", self.weekly_used);
        push_claude_cli_bucket(&mut buckets, "Sonnet", self.sonnet_used);
        buckets
    }
}

fn push_claude_cli_bucket(buckets: &mut Vec<QuotaBucketView>, label: &str, used: Option<f64>) {
    let Some(used) = used else {
        return;
    };
    buckets.push(bucket(
        label,
        Some(used_percent_label(used)),
        Some("100%".to_owned()),
        remaining_from_fraction(used),
        None,
        None,
        UsageSnapshotStatus::Fresh,
    ));
}

impl ClaudeOAuthUsageResponse {
    fn into_buckets(self, now: i64) -> Vec<QuotaBucketView> {
        let mut buckets = Vec::new();
        push_claude_window(&mut buckets, "Session", self.five_hour, now);
        push_claude_window(&mut buckets, "Weekly", self.seven_day, now);
        push_claude_window(&mut buckets, "Sonnet", self.seven_day_sonnet, now);
        push_claude_window(&mut buckets, "Opus", self.seven_day_opus, now);
        push_claude_window(&mut buckets, "Daily Routines", self.seven_day_routines, now);
        if let Some(extra) = self.extra_usage
            && extra.is_enabled.unwrap_or(true)
        {
            let remaining_percent = extra.utilization.and_then(remaining_from_fraction);
            let currency = extra.currency.unwrap_or_else(|| "credits".to_owned());
            let used = extra
                .used_credits
                .map(|used| format_extra_usage_amount(used, &currency));
            let limit = extra
                .monthly_limit
                .map(|limit| format_extra_usage_amount(limit, &currency));
            buckets.push(bucket(
                "Extra usage",
                used,
                limit,
                remaining_percent,
                None,
                None,
                UsageSnapshotStatus::Fresh,
            ));
        }
        buckets
    }
}

fn push_claude_window(
    buckets: &mut Vec<QuotaBucketView>,
    label: &str,
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
    buckets.push(bucket(
        label,
        window.utilization.map(used_percent_label),
        Some("100%".to_owned()),
        remaining,
        reset_at.map(|epoch| reset_label(epoch, now)),
        pace.as_deref(),
        UsageSnapshotStatus::Fresh,
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
    let client = provider_http_client()?;
    let user_agent = claude_code_user_agent();
    let response = client
        .get("https://api.anthropic.com/api/oauth/usage")
        .bearer_auth(access_token)
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header("anthropic-beta", "oauth-2025-04-20")
        // The OAuth usage endpoint is gated to the Claude Code client UA;
        // a generic UA is rejected.
        .header(reqwest::header::USER_AGENT, user_agent)
        .send()
        .map_err(|err| format!("Claude OAuth usage request failed: {err}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("Claude OAuth usage HTTP {status}"));
    }
    response
        .json()
        .map_err(|err| format!("Claude OAuth usage decode failed: {err}"))
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

fn load_codex_oauth_credentials(path: &Path) -> Option<CodexOAuthCredentials> {
    let text = fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
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
        let rate_limits = limits.rate_limits;
        let response = CodexUsageResponse {
            plan_type: account_plan.or(rate_limits.plan_type),
            rate_limit: Some(CodexRateLimitDetails {
                primary_window: rate_limits.primary.map(CodexWindowSnapshot::from_rpc),
                secondary_window: rate_limits.secondary.map(CodexWindowSnapshot::from_rpc),
            }),
            credits: rate_limits.credits.map(CodexCreditDetails::from_rpc),
            additional_rate_limits: None,
            reset_credits: None,
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
                rate_limit.primary_window.as_ref(),
                now,
            );
            push_codex_window(
                &mut buckets,
                "Weekly",
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
                push_codex_window(
                    &mut buckets,
                    &format!("{label} 5-hour"),
                    rate_limit.primary_window.as_ref(),
                    now,
                );
                push_codex_window(
                    &mut buckets,
                    &format!("{label} Weekly"),
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
    buckets.push(bucket(
        label,
        used.map(|value| format!("{value}% used")),
        Some("100%".to_owned()),
        remaining,
        window.reset_at.map(|epoch| reset_label(epoch, now)),
        pace.as_deref(),
        UsageSnapshotStatus::Fresh,
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
        let account_value = codex_rpc_request(
            &mut stdin,
            &rx,
            3,
            "account/read",
            serde_json::json!({}),
            CODEX_RPC_REQUEST_TIMEOUT,
        )
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
        vec![bucket(
            label,
            None,
            None,
            Some(100u8.saturating_sub(self.used_percent.round() as u8)),
            self.reset_at_epoch
                .map(|reset_at| reset_label(reset_at, now)),
            None,
            UsageSnapshotStatus::Fresh,
        )]
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
            buckets.push(bucket(
                label,
                Some(format_cents(total_used)),
                Some(format_cents(limit)),
                used_percent.map(|used| 100u8.saturating_sub(used.round() as u8)),
                reset_at.map(|reset_at| reset_label(reset_at, now)),
                None,
                UsageSnapshotStatus::Fresh,
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
    let client = provider_http_client()?;
    let mut request = client
        .get(resolve_codex_usage_url(codex_home))
        .bearer_auth(&credentials.access_token)
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::USER_AGENT, "jackin-capsule/usage");
    if let Some(account_id) = &credentials.account_id {
        request = request.header("ChatGPT-Account-Id", account_id);
    }
    let response = request
        .send()
        .map_err(|err| format!("Codex OAuth usage request failed: {err}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("Codex OAuth usage HTTP {status}"));
    }
    response
        .json()
        .map_err(|err| format!("Codex OAuth usage decode failed: {err}"))
}

fn fetch_codex_oauth_reset_credits(
    credentials: &CodexOAuthCredentials,
    codex_home: &Path,
) -> Result<CodexResetCredits, String> {
    let client = provider_http_client()?;
    let mut request = client
        .get(resolve_codex_reset_credits_url(codex_home))
        .bearer_auth(&credentials.access_token)
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::USER_AGENT, "jackin-capsule/usage")
        .header("OpenAI-Beta", "codex-1")
        .header("originator", "Codex Desktop");
    if let Some(account_id) = &credentials.account_id {
        request = request.header("ChatGPT-Account-Id", account_id);
    }
    let response = request
        .send()
        .map_err(|err| format!("Codex reset credits request failed: {err}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("Codex reset credits HTTP {status}"));
    }
    let credits = response
        .json::<CodexResetCredits>()
        .map_err(|err| format!("Codex reset credits decode failed: {err}"))?;
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
    let base = fs::read_to_string(codex_home.join("config.toml"))
        .ok()
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
        if let Some(limit) = primary_token_limit {
            buckets.push(zai_bucket("Tokens", limit, now));
        }
        if let Some(limit) = time_limit {
            buckets.push(zai_bucket("MCP", limit, now));
        }
        if let Some(limit) = session_token_limit {
            buckets.push(zai_bucket("5-hour", limit, now));
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
    bucket(
        label,
        limit
            .current_value
            .map(|value| compact_count(value.max(0) as u64)),
        limit.usage.map(|value| compact_count(value.max(0) as u64)),
        remaining,
        reset_at.map(|epoch| reset_label(epoch, now)),
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
    let client = provider_http_client()?;
    let response = client
        .get(&url)
        .bearer_auth(token)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .map_err(|err| format!("Z.AI quota request failed: {err}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("Z.AI quota HTTP {status}"));
    }
    let quota = response
        .json::<ZaiQuotaResponse>()
        .map_err(|err| format!("Z.AI quota decode failed: {err}"))?;
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
        let mut buckets = vec![kimi_bucket("Weekly", detail, None, now)];
        if let Some(rate_limit) = limits.first() {
            buckets.push(kimi_bucket(
                "Rate Limit",
                &rate_limit.detail,
                rate_limit.window.as_ref(),
                now,
            ));
        }
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
    bucket(
        label,
        used.map(|value| compact_count(value.max(0) as u64)),
        limit.map(|value| compact_count(value.max(0) as u64)),
        remaining,
        reset_at.map(|epoch| reset_label(epoch, now)),
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
    let client = provider_http_client()?;
    let response = client
        .get("https://api.kimi.com/coding/v1/usages")
        .bearer_auth(token)
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::USER_AGENT, "jackin-capsule/usage")
        .send()
        .map_err(|err| format!("Kimi usage request failed: {err}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("Kimi usage HTTP {status}"));
    }
    response
        .json()
        .map_err(|err| format!("Kimi usage decode failed: {err}"))
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
        let text = fs::read_to_string(path).ok()?;
        let value: serde_json::Value = serde_json::from_str(&text).ok()?;
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
    Some(bucket(
        &minimax_bucket_label(model_name, window),
        used_label,
        total
            .filter(|value| *value > 0)
            .map(|value| compact_count(value.max(0) as u64)),
        remaining_percent,
        reset_epoch.map(|epoch| reset_label(epoch, now)),
        detail.as_deref(),
        UsageSnapshotStatus::Fresh,
    ))
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

fn remaining_from_fraction(value: f64) -> Option<u8> {
    if !value.is_finite() {
        return None;
    }
    let used = if value <= 1.0 { value * 100.0 } else { value }
        .round()
        .clamp(0.0, 100.0) as u8;
    Some(100u8.saturating_sub(used))
}

fn used_percent_label(value: f64) -> String {
    let used = if value <= 1.0 { value * 100.0 } else { value }
        .round()
        .clamp(0.0, 100.0) as u8;
    format!("{used}% used")
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

fn humanize_plan_label(value: &str) -> String {
    value
        .split(['_', '-', ' '])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
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

fn codex_account_label(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
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
    let text = fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    first_string_key(&value, "email")
        .or_else(|| first_string_key(&value, "user_id"))
        .or_else(|| first_string_key(&value, "team_id"))
}

fn grok_plan_label(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
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

fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
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
mod tests {
    use super::*;

    #[test]
    fn compact_count_uses_token_suffixes() {
        assert_eq!(compact_count(999), "999");
        assert_eq!(compact_count(1_500), "1.5K");
        assert_eq!(compact_count(2_000_000), "2.0M");
    }

    #[test]
    fn provider_labels_resolve_all_account_refresh_surfaces() {
        assert_eq!(
            resolve_surface("codex", Some("Claude")),
            UsageSurface::Claude
        );
        assert_eq!(
            resolve_surface("claude", Some("Codex")),
            UsageSurface::Codex
        );
        assert_eq!(resolve_surface("codex", Some("Amp")), UsageSurface::Amp);
        assert_eq!(
            resolve_surface("claude", Some("Grok Build")),
            UsageSurface::Grok
        );
        assert_eq!(
            resolve_surface("codex", Some("GLM / Z.AI")),
            UsageSurface::Zai
        );
        assert_eq!(resolve_surface("codex", Some("Kimi")), UsageSurface::Kimi);
        assert_eq!(
            resolve_surface("codex", Some("MiniMax")),
            UsageSurface::Minimax
        );
    }

    #[test]
    fn provider_tabs_follow_usage_overlay_display_order() {
        let labels = provider_tabs(UsageSurface::Codex)
            .into_iter()
            .map(|tab| tab.label)
            .collect::<Vec<_>>();

        assert_eq!(
            labels,
            vec![
                "Codex".to_owned(),
                "Claude".to_owned(),
                "Amp".to_owned(),
                "Grok Build".to_owned(),
                "GLM / Z.AI".to_owned(),
                "Kimi".to_owned(),
                "MiniMax".to_owned(),
            ]
        );
    }

    #[test]
    fn provider_tabs_include_cached_account_identity() {
        let mut view = FocusedUsageView::unavailable("none", 123);
        view.account = FocusedAccountHeader {
            provider_label: "OpenAI / Codex".to_owned(),
            account_label: "codex@example.com".to_owned(),
            plan_label: Some("Pro 20x".to_owned()),
        };
        view.status = UsageSnapshotStatus::Fresh;
        view.tabs = provider_tabs(UsageSurface::Codex);

        let mut claude = FocusedUsageView::unavailable("none", 120);
        claude.account = FocusedAccountHeader {
            provider_label: "Anthropic / Claude".to_owned(),
            account_label: "claude@example.com".to_owned(),
            plan_label: Some("Max".to_owned()),
        };
        claude.status = UsageSnapshotStatus::Stale;

        let mut snapshots = HashMap::new();
        snapshots.insert("claude:Claude".to_owned(), CachedUsage { view: claude });

        enrich_provider_tabs(&mut view, &snapshots);

        let codex = view
            .tabs
            .iter()
            .find(|tab| tab.label == "Codex")
            .expect("codex tab");
        assert_eq!(codex.account_label, "codex@example.com");
        assert_eq!(codex.plan_label.as_deref(), Some("Pro 20x"));

        let claude = view
            .tabs
            .iter()
            .find(|tab| tab.label == "Claude")
            .expect("claude tab");
        assert_eq!(claude.account_label, "claude@example.com");
        assert_eq!(claude.plan_label.as_deref(), Some("Max"));
        assert_eq!(claude.status_label, "stale");
    }

    #[test]
    fn usage_status_label_prefers_in_memory_cache_before_store() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut cache = UsageCache::default();
        cache.set_telemetry_store_path(dir.path().join("missing").join("usage.sqlite3"));
        let view = codex_cached_usage_view();
        let expected = view.status_bar_label.clone();
        cache.snapshots.insert(
            canonical_usage_cache_key("codex", Some("OpenAI")),
            CachedUsage { view },
        );

        assert_eq!(
            cache.focused_status_bar_label(Some("codex"), Some("OpenAI")),
            Some(expected)
        );
    }

    #[test]
    fn usage_snapshot_prefers_in_memory_cache_before_store() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut cache = UsageCache::default();
        cache.set_telemetry_store_path(dir.path().join("missing").join("usage.sqlite3"));
        let view = codex_cached_usage_view();
        let expected_label = view.status_bar_label.clone();
        cache.snapshots.insert(
            canonical_usage_cache_key("codex", Some("OpenAI")),
            CachedUsage { view },
        );

        let snapshot =
            cache.focused_snapshot(Some("codex"), Some("OpenAI"), &BTreeMap::new(), false);

        assert_eq!(snapshot.status_bar_label, expected_label);
        assert_eq!(snapshot.account.account_label, "codex@example.com");
        assert!(
            snapshot
                .tabs
                .iter()
                .any(|tab| tab.label == "Codex" && tab.active)
        );
    }

    fn codex_cached_usage_view() -> FocusedUsageView {
        usage_view(UsageViewInput {
            agent: "codex",
            provider: Some("OpenAI"),
            surface: UsageSurface::Codex,
            account_label: "codex@example.com".to_owned(),
            plan_label: Some("Pro 20x".to_owned()),
            buckets: vec![QuotaBucketView {
                label: "Session".to_owned(),
                used_label: Some("63% used".to_owned()),
                limit_label: Some("100%".to_owned()),
                remaining_percent: Some(37),
                reset_label: Some("Resets in 2h".to_owned()),
                pace_label: None,
                status: UsageSnapshotStatus::Fresh,
            }],
            status: UsageSnapshotStatus::Fresh,
            source: UsageSource::ProviderApi,
            confidence: UsageConfidence::Authoritative,
            now: 123,
            last_error: None,
        })
    }

    #[test]
    fn materialized_usage_accounts_write_normalized_snapshots() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("usage").join("accounts.json");
        let mut view = FocusedUsageView::unavailable("none", 123);
        view.focused_agent = Some("codex".to_owned());
        view.status_bar_label = "Codex Session: 63% used · 37% left".to_owned();

        write_materialized_usage_accounts(&path, 456, vec![view]).expect("write accounts");

        let body = fs::read_to_string(&path).expect("accounts json");
        let decoded: MaterializedUsageAccounts =
            serde_json::from_str(&body).expect("decode accounts");
        assert_eq!(decoded.generated_at_epoch, 456);
        assert_eq!(decoded.snapshots.len(), 1);
        assert_eq!(decoded.snapshots[0].focused_agent.as_deref(), Some("codex"));
        assert_eq!(
            decoded.snapshots[0].status_bar_label,
            "Codex Session: 63% used · 37% left"
        );
        let leftovers = fs::read_dir(path.parent().expect("parent"))
            .expect("read usage dir")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp."))
            .count();
        assert_eq!(leftovers, 0);
    }

    #[test]
    fn status_bar_label_uses_session_and_weekly_remaining() {
        let buckets = vec![
            QuotaBucketView {
                label: "Session".to_owned(),
                used_label: Some("63% used".to_owned()),
                limit_label: Some("100%".to_owned()),
                remaining_percent: Some(37),
                reset_label: None,
                pace_label: None,
                status: UsageSnapshotStatus::Fresh,
            },
            QuotaBucketView {
                label: "Weekly".to_owned(),
                used_label: Some("90% used".to_owned()),
                limit_label: Some("100%".to_owned()),
                remaining_percent: Some(10),
                reset_label: Some("Resets in 3h 52m".to_owned()),
                pace_label: None,
                status: UsageSnapshotStatus::Fresh,
            },
        ];

        assert_eq!(
            status_bar_label(
                UsageSurface::Codex,
                "alexey@example.com",
                UsageSnapshotStatus::Fresh,
                &buckets
            ),
            "Session 37% · Weekly 10%"
        );
    }

    #[test]
    fn status_bar_label_uses_stale_cached_percentages() {
        let buckets = vec![QuotaBucketView {
            label: "Session".to_owned(),
            used_label: Some("99% used".to_owned()),
            limit_label: Some("100%".to_owned()),
            remaining_percent: Some(1),
            reset_label: None,
            pace_label: None,
            status: UsageSnapshotStatus::Stale,
        }];

        assert_eq!(
            status_bar_label(
                UsageSurface::Claude,
                "alexey@example.com",
                UsageSnapshotStatus::Stale,
                &buckets
            ),
            "Session 1%"
        );
    }

    #[test]
    fn status_bar_label_uses_amp_free_and_credits() {
        let buckets = vec![
            QuotaBucketView {
                label: "Amp Free".to_owned(),
                used_label: Some("$5.24".to_owned()),
                limit_label: Some("$10".to_owned()),
                remaining_percent: Some(48),
                reset_label: None,
                pace_label: None,
                status: UsageSnapshotStatus::Fresh,
            },
            QuotaBucketView {
                label: "Individual credits".to_owned(),
                used_label: None,
                limit_label: Some("$4.76".to_owned()),
                remaining_percent: None,
                reset_label: None,
                pace_label: Some("Individual credits: $4.76".to_owned()),
                status: UsageSnapshotStatus::Fresh,
            },
        ];

        assert_eq!(
            status_bar_label(
                UsageSurface::Amp,
                "alexey@example.com",
                UsageSnapshotStatus::Fresh,
                &buckets
            ),
            "Free 48% · $4.76"
        );
    }

    #[test]
    fn status_bar_label_uses_stale_amp_cache() {
        let buckets = vec![QuotaBucketView {
            label: "Amp Free".to_owned(),
            used_label: Some("$9.10".to_owned()),
            limit_label: Some("$10".to_owned()),
            remaining_percent: Some(9),
            reset_label: None,
            pace_label: None,
            status: UsageSnapshotStatus::Stale,
        }];

        assert_eq!(
            status_bar_label(
                UsageSurface::Amp,
                "alexey@example.com",
                UsageSnapshotStatus::Stale,
                &buckets
            ),
            "Free 9%"
        );
    }

    #[test]
    fn usage_refresh_targets_are_focused_first_and_deduplicated() {
        let active = vec![
            UsageRefreshTarget {
                agent: "claude".to_owned(),
                provider: Some("Anthropic".to_owned()),
            },
            UsageRefreshTarget {
                agent: "codex".to_owned(),
                provider: Some("OpenAI".to_owned()),
            },
            UsageRefreshTarget {
                agent: "claude".to_owned(),
                provider: Some("Anthropic / Claude".to_owned()),
            },
        ];
        let focused = UsageRefreshTarget {
            agent: "codex".to_owned(),
            provider: Some("OpenAI".to_owned()),
        };

        let ordered = ordered_refresh_targets(&active, Some(focused));

        assert_eq!(
            ordered,
            vec![
                UsageRefreshTarget {
                    agent: "codex".to_owned(),
                    provider: Some("OpenAI".to_owned()),
                },
                UsageRefreshTarget {
                    agent: "claude".to_owned(),
                    provider: Some("Anthropic".to_owned()),
                },
            ]
        );
    }

    #[test]
    fn usage_cache_key_canonicalizes_provider_aliases() {
        assert_eq!(
            canonical_usage_cache_key("claude", Some("Anthropic")),
            canonical_usage_cache_key("claude", Some("Anthropic / Claude"))
        );
        assert_eq!(
            canonical_usage_cache_key("codex", Some("OpenAI")),
            canonical_usage_cache_key("codex", Some("OpenAI / Codex"))
        );
        assert_eq!(
            canonical_usage_cache_key("claude", Some("Z.AI")),
            canonical_usage_cache_key("glm", Some("GLM / Z.AI"))
        );
        assert_ne!(
            canonical_usage_cache_key("claude", Some("Anthropic")),
            canonical_usage_cache_key("claude", Some("Z.AI"))
        );
    }

    #[test]
    fn usage_refresh_interval_stays_within_jitter_bounds() {
        for key in ["Codex", "Claude", "GLM / Z.AI"] {
            let interval = refresh_interval_for_key(key);
            assert!(
                interval >= USAGE_REFRESH_BASE_INTERVAL.saturating_sub(USAGE_REFRESH_JITTER),
                "{key}: {interval:?}"
            );
            assert!(
                interval <= USAGE_REFRESH_BASE_INTERVAL + USAGE_REFRESH_JITTER,
                "{key}: {interval:?}"
            );
        }
    }

    #[test]
    fn usage_rate_limit_delay_honors_retry_after_and_caps_backoff() {
        assert_eq!(
            usage_rate_limit_delay("provider HTTP 429 retry-after: 17", 1),
            Duration::from_secs(17)
        );
        assert_eq!(
            usage_rate_limit_delay("provider HTTP 429", 1),
            USAGE_REFRESH_BASE_INTERVAL
        );
        assert_eq!(
            usage_rate_limit_delay("provider HTTP 429", 20),
            USAGE_REFRESH_BACKOFF_CAP
        );
        assert!(!usage_error_is_rate_limited("provider HTTP 500"));
    }

    #[test]
    fn usage_refresh_schedule_skips_until_ttl_or_manual_refresh() {
        let dir = tempfile::tempdir().expect("tempdir");
        let target = UsageRefreshTarget {
            agent: "codex".to_owned(),
            provider: Some("OpenAI".to_owned()),
        };
        let mut schedule = UsageRefreshSchedule::default();
        let now = Instant::now();
        let view = FocusedUsageView::unavailable("fresh", now_epoch());

        assert!(schedule.should_refresh_with_cooldown_dir(&target, now, dir.path()));
        schedule.mark_refreshed_with_cooldown_dir(&target, now, &view, dir.path());
        assert!(!schedule.should_refresh_with_cooldown_dir(
            &target,
            now + Duration::from_secs(30),
            dir.path()
        ));

        schedule.mark_due(&target, now + Duration::from_secs(31));
        assert!(schedule.should_refresh_with_cooldown_dir(
            &target,
            now + Duration::from_secs(31),
            dir.path()
        ));
    }

    #[test]
    fn usage_refresh_schedule_writes_and_honors_shared_rate_limit_cooldown() {
        let dir = tempfile::tempdir().expect("tempdir");
        let target = UsageRefreshTarget {
            agent: "codex".to_owned(),
            provider: Some("OpenAI".to_owned()),
        };
        let mut schedule = UsageRefreshSchedule::default();
        let now = Instant::now();

        assert!(schedule.should_refresh_with_cooldown_dir(&target, now, dir.path()));

        let mut view = FocusedUsageView::unavailable("rate limited", now_epoch());
        view.last_error = Some("Codex usage HTTP 429 retry-after: 60".to_owned());
        schedule.mark_refreshed_with_cooldown_dir(&target, now, &view, dir.path());

        let key = target.cache_key();
        assert!(shared_usage_cooldown_active(dir.path(), &key, now_epoch()));
        assert!(!schedule.should_refresh_with_cooldown_dir(
            &target,
            now + Duration::from_secs(61),
            dir.path()
        ));
    }

    #[test]
    fn failed_refresh_preserves_last_fresh_quota_rows_as_stale_cache() {
        let mut cached = FocusedUsageView::unavailable("seed", 123);
        cached.status = UsageSnapshotStatus::Fresh;
        cached.confidence = UsageConfidence::Authoritative;
        cached.account = FocusedAccountHeader {
            provider_label: "OpenAI / Codex".to_owned(),
            account_label: "alexey@example.com".to_owned(),
            plan_label: Some("Pro 20x".to_owned()),
        };
        cached.buckets = vec![QuotaBucketView {
            label: "Weekly".to_owned(),
            used_label: Some("90% used".to_owned()),
            limit_label: Some("100%".to_owned()),
            remaining_percent: Some(10),
            reset_label: Some("Resets in 3h 52m".to_owned()),
            pace_label: None,
            status: UsageSnapshotStatus::Fresh,
        }];

        for failed_status in [
            UsageSnapshotStatus::Stale,
            UsageSnapshotStatus::NeedsLogin,
            UsageSnapshotStatus::Error,
        ] {
            let mut view = FocusedUsageView::unavailable("seed", 124);
            view.focused_agent = Some("codex".to_owned());
            view.focused_provider = Some("Codex".to_owned());
            view.status = failed_status;
            view.account = FocusedAccountHeader {
                provider_label: "OpenAI / Codex".to_owned(),
                account_label: "alexey@example.com".to_owned(),
                plan_label: None,
            };
            view.last_error = Some("Codex provider usage unavailable".to_owned());

            preserve_cached_quota_on_failed_refresh(&mut view, &cached);

            assert_eq!(view.status, UsageSnapshotStatus::Stale);
            assert_eq!(view.source, UsageSource::Cache);
            assert_eq!(view.confidence, UsageConfidence::Authoritative);
            assert_eq!(view.buckets.len(), 1);
            assert_eq!(view.buckets[0].status, UsageSnapshotStatus::Stale);
            assert_eq!(view.account.plan_label.as_deref(), Some("Pro 20x"));
            assert_eq!(view.status_bar_label, "Weekly 10%");
            assert!(
                view.last_error
                    .as_deref()
                    .is_some_and(|error| error.contains("showing last cached quota"))
            );
        }
    }

    #[test]
    fn claude_oauth_response_maps_windows_to_buckets() {
        let usage: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
            "five_hour": { "utilization": 0.84, "resets_at": "2026-06-11T15:12:00Z" },
            "seven_day": { "utilization": 0.78, "resets_at": "2026-06-12T14:26:00Z" },
            "seven_day_sonnet": { "utilization": 0.02, "resets_at": "2026-06-12T14:26:00Z" },
            "seven_day_routines": { "utilization": 0.0 },
            "extra_usage": {
                "is_enabled": true,
                "monthly_limit": 260.0,
                "used_credits": 78.49,
                "utilization": 0.30,
                "currency": "SGD"
            }
        }))
        .expect("valid Claude OAuth usage");

        let buckets = usage.into_buckets(1_781_185_560);

        assert_eq!(buckets[0].label, "Session");
        assert_eq!(buckets[0].remaining_percent, Some(16));
        assert_eq!(
            buckets[0].reset_label.as_deref(),
            Some(
                reset_label(
                    parse_iso_epoch("2026-06-11T15:12:00Z").expect("session reset"),
                    1_781_185_560,
                )
                .as_str()
            )
        );
        assert_eq!(buckets[1].label, "Weekly");
        assert_eq!(buckets[1].remaining_percent, Some(22));
        assert!(buckets.iter().any(|bucket| bucket.label == "Sonnet"));
        assert!(
            buckets
                .iter()
                .find(|bucket| bucket.label == "Sonnet")
                .is_some_and(|bucket| bucket.pace_label.is_none())
        );
        assert!(buckets.iter().any(|bucket| bucket.label == "Daily Routines"
            && bucket.remaining_percent == Some(100)
            && bucket.pace_label.is_none()));
        let extra = buckets
            .iter()
            .find(|bucket| bucket.label == "Extra usage")
            .expect("extra usage bucket");
        assert_eq!(extra.remaining_percent, Some(70));
        assert_eq!(extra.used_label.as_deref(), Some("SGD 78.49"));
        assert_eq!(extra.limit_label.as_deref(), Some("SGD 260.00"));
    }

    #[test]
    fn claude_oauth_response_accepts_window_aliases() {
        let usage: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
            "five_hour": { "utilization": 0.10 },
            "seven_day_oauth_apps": { "utilization": 0.45 },
            "seven_day_cowork": { "utilization": 0.25 }
        }))
        .expect("valid Claude OAuth usage aliases");

        let buckets = usage.into_buckets(1_781_185_560);

        assert!(
            buckets
                .iter()
                .any(|bucket| bucket.label == "Weekly" && bucket.remaining_percent == Some(55))
        );
        assert!(
            buckets
                .iter()
                .any(|bucket| bucket.label == "Daily Routines"
                    && bucket.remaining_percent == Some(75))
        );
    }

    #[test]
    fn codex_oauth_response_maps_primary_weekly_spark_and_credits() {
        let mut usage: CodexUsageResponse = serde_json::from_value(serde_json::json!({
            "plan_type": "pro",
            "rate_limit": {
                "primary_window": {
                    "used_percent": 63,
                    "reset_at": 1781189520,
                    "limit_window_seconds": 18000
                },
                "secondary_window": {
                    "used_percent": 90,
                    "reset_at": 1781197200,
                    "limit_window_seconds": 604800
                }
            },
            "additional_rate_limits": [{
                "limit_name": "gpt-5.3-codex-spark",
                "rate_limit": {
                    "primary_window": {
                        "used_percent": 0,
                        "reset_at": 1781200800,
                        "limit_window_seconds": 18000
                    },
                    "secondary_window": {
                        "used_percent": 0,
                        "reset_at": 1781798400,
                        "limit_window_seconds": 604800
                    }
                }
            }],
            "credits": {
                "has_credits": true,
                "unlimited": false,
                "balance": "12.5"
            }
        }))
        .expect("valid Codex usage");
        usage.reset_credits = Some(CodexResetCredits {
            available_count: 2,
            credits: vec![
                CodexResetCredit {
                    status: Some("available".to_owned()),
                    expires_at: Some("2026-06-10T00:00:00Z".to_owned()),
                },
                CodexResetCredit {
                    status: Some("available".to_owned()),
                    expires_at: Some("2026-06-18T00:00:00Z".to_owned()),
                },
                CodexResetCredit {
                    status: Some("redeemed".to_owned()),
                    expires_at: Some("2026-06-17T00:00:00Z".to_owned()),
                },
            ],
        });

        let buckets = usage.buckets(1_781_185_560);

        assert_eq!(buckets[0].label, "Session");
        assert_eq!(buckets[0].remaining_percent, Some(37));
        assert_eq!(buckets[1].label, "Weekly");
        assert_eq!(buckets[1].remaining_percent, Some(10));
        assert!(
            buckets
                .iter()
                .any(|bucket| bucket.label == "Codex Spark 5-hour"
                    && bucket.remaining_percent == Some(100))
        );
        let reset_credits = buckets
            .iter()
            .position(|bucket| bucket.label == "Limit Reset Credits")
            .expect("reset credits bucket");
        let reset_credit_label = format!(
            "2 manual resets available · Next expires {}",
            expiry_label(
                parse_iso_epoch("2026-06-18T00:00:00Z").expect("expiry epoch"),
                1_781_185_560
            )
        );
        assert_eq!(
            buckets[reset_credits].pace_label.as_deref(),
            Some(reset_credit_label.as_str())
        );
        let credits = buckets
            .iter()
            .enumerate()
            .find(|(_, bucket)| bucket.label == "Credits")
            .expect("credits bucket");
        assert!(reset_credits < credits.0);
        assert_eq!(credits.1.limit_label.as_deref(), Some("12.50 credits"));
    }

    #[test]
    fn codex_rpc_response_maps_account_windows_and_credits() {
        let limits: CodexRpcRateLimitsResponse = serde_json::from_value(serde_json::json!({
            "rateLimits": {
                "primary": {
                    "usedPercent": 63.0,
                    "windowDurationMins": 300,
                    "resetsAt": 1781189520
                },
                "secondary": {
                    "usedPercent": 90.0,
                    "windowDurationMins": 10080,
                    "resetsAt": 1781798400
                },
                "credits": {
                    "hasCredits": true,
                    "unlimited": false,
                    "balance": "12.5"
                },
                "planType": "pro"
            }
        }))
        .expect("valid Codex RPC rate limits");
        let account: CodexRpcAccountResponse = serde_json::from_value(serde_json::json!({
            "account": {
                "type": "chatgpt",
                "email": "person@example.com",
                "planType": "pro"
            }
        }))
        .expect("valid Codex RPC account");

        let usage = CodexRpcUsage::from_rpc(limits, Some(account));
        let buckets = usage.response.buckets(1_781_185_560);

        assert_eq!(usage.account_label.as_deref(), Some("person@example.com"));
        assert_eq!(usage.response.plan_type.as_deref(), Some("pro"));
        assert_eq!(buckets[0].label, "Session");
        assert_eq!(buckets[0].remaining_percent, Some(37));
        assert_eq!(buckets[0].pace_label.as_deref(), Some("15% in reserve"));
        assert_eq!(buckets[1].label, "Weekly");
        assert_eq!(buckets[1].remaining_percent, Some(10));
        assert_eq!(buckets[1].pace_label.as_deref(), Some("1 week window"));
        let credits = buckets
            .iter()
            .find(|bucket| bucket.label == "Credits")
            .expect("credits bucket");
        assert_eq!(credits.limit_label.as_deref(), Some("12.50 credits"));
    }

    #[test]
    fn managed_cli_launch_gate_cools_down_after_launch_failure() {
        let mut gate = ManagedCliLaunchGate::default();
        assert!(gate.can_launch("probe", Instant::now()).is_ok());

        gate.record_launch_failure("blocked".to_owned());

        let error = gate
            .can_launch("probe", Instant::now())
            .expect_err("cooldown should block launch");
        assert!(error.contains("cooldown active"));
        assert!(error.contains("blocked"));

        gate.record_success();
        assert!(gate.can_launch("probe", Instant::now()).is_ok());
    }

    #[test]
    fn claude_usage_diagnostic_invokes_explicit_usage_command() {
        let diagnostic = run_claude_usage_diagnostic_with(|command, args, timeout| {
            assert_eq!(command, "claude");
            assert_eq!(args, ["-p", "/usage"]);
            assert_eq!(timeout, PROVIDER_CLI_TIMEOUT);
            Ok(CliOutput {
                success: true,
                exit_code: Some(0),
                stdout: "usage output".to_owned(),
                stderr: String::new(),
            })
        })
        .expect("diagnostic");

        assert_eq!(diagnostic.command, "claude");
        assert_eq!(diagnostic.args, vec!["-p", "/usage"]);
        assert!(diagnostic.success);
        assert_eq!(diagnostic.stdout, "usage output");
    }

    #[test]
    fn claude_usage_diagnostic_preserves_cli_failure_output() {
        let diagnostic = run_claude_usage_diagnostic_with(|_, _, _| {
            Ok(CliOutput {
                success: false,
                exit_code: Some(1),
                stdout: String::new(),
                stderr: "not logged in".to_owned(),
            })
        })
        .expect("diagnostic");

        assert!(!diagnostic.success);
        assert_eq!(diagnostic.exit_code, Some(1));
        assert_eq!(diagnostic.stderr, "not logged in");
    }

    #[test]
    fn claude_cli_usage_output_maps_current_windows() {
        let usage = parse_claude_usage_output(
            "You are currently using your subscription to power your Claude Code usage\n\
             \n\
             Current session: 0% used\n\
             Current week (all models): 46% used · resets Jun 26, 6:59am (UTC)\n\
             Current week (Sonnet only): 15% used · resets Jun 26, 6:59am (UTC)\n",
        )
        .expect("usage output");

        let buckets = usage.buckets();

        assert_eq!(buckets[0].label, "Session");
        assert_eq!(buckets[0].remaining_percent, Some(100));
        assert_eq!(buckets[1].label, "Weekly");
        assert_eq!(buckets[1].remaining_percent, Some(54));
        assert_eq!(buckets[2].label, "Sonnet");
        assert_eq!(buckets[2].remaining_percent, Some(85));
    }

    #[test]
    fn grok_billing_response_maps_monthly_credits() {
        let usage: GrokBillingResponse = serde_json::from_value(serde_json::json!({
            "billingCycle": {
                "billingPeriodStart": "2026-06-01T00:00:00Z",
                "billingPeriodEnd": "2026-07-01T00:00:00Z"
            },
            "monthlyLimit": { "val": 5000 },
            "onDemandCap": { "val": 2500 },
            "on_demand_enabled": true,
            "usage": {
                "includedUsed": { "val": 1500 },
                "onDemandUsed": { "val": 300 },
                "totalUsed": { "val": 1800 }
            }
        }))
        .expect("valid Grok billing response");

        let buckets = usage.buckets(1_780_315_200);

        assert_eq!(buckets[0].label, "Monthly");
        assert_eq!(buckets[0].used_label.as_deref(), Some("$18"));
        assert_eq!(buckets[0].limit_label.as_deref(), Some("$50"));
        assert_eq!(buckets[0].remaining_percent, Some(64));
        assert_eq!(
            buckets[0].reset_label.as_deref(),
            Some(
                reset_label(
                    parse_iso_epoch("2026-07-01T00:00:00Z").expect("billing reset"),
                    1_780_315_200,
                )
                .as_str()
            )
        );
        assert_eq!(buckets[0].pace_label, None);
        assert!(buckets.iter().any(|bucket| bucket.label == "Included usage"
            && bucket.used_label.as_deref() == Some("$15")));
        assert!(
            buckets
                .iter()
                .any(|bucket| bucket.label == "On-demand usage"
                    && bucket.used_label.as_deref() == Some("$3")
                    && bucket.limit_label.as_deref() == Some("$25"))
        );
    }

    #[test]
    fn grok_rpc_payload_keeps_billing_method_unescaped() {
        let payload = grok_rpc_request_payload(2, "x.ai/billing", serde_json::json!({}));
        let encoded = serde_json::to_string(&payload).expect("encode payload");

        assert!(encoded.contains("\"method\":\"x.ai/billing\""));
        assert!(!encoded.contains("x.ai\\/billing"));
    }

    #[test]
    fn grok_account_label_prefers_auth_identity_over_env_presence() {
        let dir = tempfile::tempdir().expect("tempdir");
        let auth = dir.path().join("auth.json");
        fs::write(
            &auth,
            r#"{"account":{"email":"operator@example.com"},"token":"redacted"}"#,
        )
        .expect("write auth");

        let label = grok_account_label_or_presence(&auth, true, true, true);

        assert_eq!(label, "operator@example.com");
    }

    #[test]
    fn grok_account_label_reports_safe_credential_presence() {
        let missing = Path::new("/tmp/nonexistent-grok-auth-for-test.json");

        assert_eq!(
            grok_account_label_or_presence(missing, false, true, true),
            "XAI_API_KEY present"
        );
        assert_eq!(
            grok_account_label_or_presence(missing, false, false, true),
            "GROK_DEPLOYMENT_KEY present"
        );
        assert_eq!(
            grok_account_label_or_presence(missing, false, false, false),
            "needs Grok login"
        );
    }

    #[test]
    fn grok_snapshot_uses_probe_success_without_local_credential_marker() {
        let missing = Path::new("/tmp/nonexistent-grok-auth-for-test.json");
        let billing: GrokBillingResponse = serde_json::from_value(serde_json::json!({
            "billingCycle": {
                "billingPeriodStart": "2026-06-01T00:00:00Z",
                "billingPeriodEnd": "2026-07-01T00:00:00Z"
            },
            "monthlyLimit": { "val": 5000 },
            "usage": { "totalUsed": { "val": 1000 } }
        }))
        .expect("valid Grok billing response");

        let view = grok_snapshot_from_rpc_result(
            "grok",
            1_780_315_200,
            missing,
            false,
            false,
            false,
            Ok(GrokBillingSnapshot::Rpc(billing)),
        );

        assert_eq!(view.status, UsageSnapshotStatus::Fresh);
        assert_eq!(view.source, UsageSource::Cli);
        assert_eq!(view.confidence, UsageConfidence::Authoritative);
        assert_eq!(view.account.account_label, "needs Grok login");
        assert_eq!(view.buckets[0].label, "Monthly");
        assert_eq!(view.buckets[0].remaining_percent, Some(80));
        assert_eq!(view.last_error, None);
    }

    #[test]
    fn grok_web_billing_response_maps_weekly_usage() {
        let data = [
            0x00, 0x00, 0x00, 0x00, 0x3c, 0x0a, 0x3a, 0x0d, 0x9c, 0x7d, 0xac, 0x42, 0x12, 0x00,
            0x1a, 0x00, 0x22, 0x06, 0x08, 0x80, 0x97, 0xf3, 0xd0, 0x06, 0x2a, 0x06, 0x08, 0x80,
            0xb1, 0x91, 0xd2, 0x06, 0x3a, 0x07, 0x08, 0x02, 0x15, 0x12, 0x03, 0xa5, 0x42, 0x42,
            0x12, 0x08, 0x01, 0x12, 0x06, 0x08, 0x80, 0x97, 0xf3, 0xd0, 0x06, 0x1a, 0x06, 0x08,
            0x80, 0xb1, 0x91, 0xd2, 0x06, 0x62, 0x00, 0x68, 0x01, 0x72, 0x00, 0x7a, 0x00, 0x82,
            0x01, 0x00, 0x8a, 0x01, 0x00, 0x92, 0x01, 0x00, 0x9a, 0x01, 0x00, 0xa2, 0x01, 0x00,
            0xaa, 0x01, 0x00,
        ];

        let snapshot =
            parse_grok_web_billing_response(&data, 1_782_318_000).expect("parse grok billing");
        let buckets = snapshot.buckets(1_782_318_000);

        assert_eq!(buckets[0].label, "Weekly");
        assert_eq!(buckets[0].remaining_percent, Some(14));
        assert_eq!(
            buckets[0].reset_label.as_deref(),
            Some(
                reset_label(
                    parse_iso_epoch("2026-07-01T00:00:00Z").expect("billing reset"),
                    1_782_318_000,
                )
                .as_str()
            )
        );
    }

    #[test]
    fn grok_cycle_label_falls_back_to_credits_for_irregular_cycles() {
        assert_eq!(grok_cycle_label_from_minutes(7 * 24 * 60), "Weekly");
        assert_eq!(grok_cycle_label_from_minutes(30 * 24 * 60), "Monthly");
        assert_eq!(grok_cycle_label_from_minutes(13 * 24 * 60), "Credits");
    }

    #[test]
    fn grok_snapshot_reports_probe_error_instead_of_presence_gate() {
        let missing = Path::new("/tmp/nonexistent-grok-auth-for-test.json");

        let view = grok_snapshot_from_rpc_result(
            "grok",
            1_780_315_200,
            missing,
            false,
            false,
            false,
            Err("grok agent stdio failed to start: not found".to_owned()),
        );

        assert_eq!(view.status, UsageSnapshotStatus::NeedsLogin);
        assert_eq!(view.source, UsageSource::None);
        assert_eq!(view.confidence, UsageConfidence::None);
        assert_eq!(
            view.last_error.as_deref(),
            Some("grok agent stdio failed to start: not found")
        );
    }

    #[test]
    fn codex_oauth_credentials_parse_nested_tokens() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("auth.json");
        let id_token = test_jwt(serde_json::json!({
            "email": "person@example.com",
            "sub": "acct-sub"
        }));
        fs::write(
            &path,
            serde_json::json!({
                "tokens": {
                    "access_token": "access",
                    "refresh_token": "refresh",
                    "account_id": "acct",
                    "id_token": id_token
                }
            })
            .to_string(),
        )
        .expect("write auth");

        let credentials = load_codex_oauth_credentials(&path).expect("credentials");

        assert_eq!(credentials.access_token, "access");
        assert_eq!(credentials.account_id.as_deref(), Some("acct"));
        assert_eq!(
            credentials.account_label.as_deref(),
            Some("person@example.com")
        );
    }

    #[test]
    fn codex_id_token_identity_falls_back_to_subject() {
        let id_token = test_jwt(serde_json::json!({
            "sub": "user-123"
        }));

        assert_eq!(
            codex_account_label_from_id_token(&id_token).as_deref(),
            Some("ChatGPT account user-123")
        );
    }

    #[test]
    fn credential_file_loaders_reread_updated_container_files() {
        let dir = tempfile::tempdir().expect("tempdir");

        let claude_path = dir.path().join(".credentials.json");
        fs::write(
            &claude_path,
            serde_json::json!({
                "claudeAiOauth": {
                    "accessToken": "old-claude",
                    "subscriptionType": "max"
                }
            })
            .to_string(),
        )
        .expect("write Claude auth");
        assert_eq!(
            load_claude_oauth_credentials(&claude_path)
                .expect("Claude credentials")
                .access_token,
            "old-claude"
        );
        fs::write(
            &claude_path,
            serde_json::json!({
                "claudeAiOauth": {
                    "accessToken": "new-claude",
                    "subscriptionType": "max"
                }
            })
            .to_string(),
        )
        .expect("refresh Claude auth");
        assert_eq!(
            load_claude_oauth_credentials(&claude_path)
                .expect("updated Claude credentials")
                .access_token,
            "new-claude"
        );

        let codex_path = dir.path().join("auth.json");
        fs::write(
            &codex_path,
            serde_json::json!({
                "tokens": {
                    "access_token": "old-codex",
                    "id_token": test_jwt(serde_json::json!({"email": "old@example.com"}))
                }
            })
            .to_string(),
        )
        .expect("write Codex auth");
        assert_eq!(
            load_codex_oauth_credentials(&codex_path)
                .expect("Codex credentials")
                .access_token,
            "old-codex"
        );
        fs::write(
            &codex_path,
            serde_json::json!({
                "tokens": {
                    "access_token": "new-codex",
                    "id_token": test_jwt(serde_json::json!({"email": "new@example.com"}))
                }
            })
            .to_string(),
        )
        .expect("refresh Codex auth");
        let codex = load_codex_oauth_credentials(&codex_path).expect("updated Codex credentials");
        assert_eq!(codex.access_token, "new-codex");
        assert_eq!(codex.account_label.as_deref(), Some("new@example.com"));

        let kimi_path = dir.path().join(".kimi-code/credentials/kimi-code.json");
        fs::create_dir_all(kimi_path.parent().expect("Kimi credentials parent"))
            .expect("create Kimi credentials dir");
        fs::write(
            &kimi_path,
            serde_json::json!({
                "access_token": "old-kimi",
                "expires_at": 1_781_300_000
            })
            .to_string(),
        )
        .expect("write Kimi auth");
        assert_eq!(
            load_kimi_local_token_from_home(dir.path(), 1_781_200_000).as_deref(),
            Some("old-kimi")
        );
        fs::write(
            &kimi_path,
            serde_json::json!({
                "access_token": "new-kimi",
                "expires_at": 1_781_300_000
            })
            .to_string(),
        )
        .expect("refresh Kimi auth");
        assert_eq!(
            load_kimi_local_token_from_home(dir.path(), 1_781_200_000).as_deref(),
            Some("new-kimi")
        );
        fs::write(
            &kimi_path,
            serde_json::json!({
                "access_token": "expired-kimi",
                "expires_at": 1_781_100_000
            })
            .to_string(),
        )
        .expect("expire Kimi auth");
        assert_eq!(
            load_kimi_local_token_from_home(dir.path(), 1_781_200_000),
            None
        );
    }

    fn test_jwt(payload: serde_json::Value) -> String {
        let header = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode("{}");
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload.to_string());
        format!("{header}.{payload}.signature")
    }

    #[test]
    fn quota_pace_label_uses_codexbar_reserve_deficit_onpace() {
        // Behind pace (burning faster than the clock): 60% quota left with 90%
        // of the window still remaining -> 30 points of deficit.
        let deficit = quota_pace_label(Some(60), Some(900), Some(1_000), 0).expect("pace label");
        assert_eq!(deficit, "30% in deficit");

        // Ahead of pace (quota outlasting the clock): 90% left, 60% of window
        // remaining -> 30 points in reserve.
        let reserve = quota_pace_label(Some(90), Some(600), Some(1_000), 0).expect("pace label");
        assert_eq!(reserve, "30% in reserve");

        // Within 2 points of the clock -> On pace.
        let on_pace = quota_pace_label(Some(50), Some(500), Some(1_000), 0).expect("pace label");
        assert_eq!(on_pace, "On pace");
    }

    #[test]
    fn reset_label_uses_relative_and_local_timestamp() {
        let now = parse_iso_epoch("2026-06-11T13:46:00Z").expect("now");
        let same_day = parse_iso_epoch("2026-06-11T15:12:00Z").expect("same day");
        assert_eq!(
            reset_label(same_day, now),
            format!("Resets in 1h 26m ({})", local_timestamp_label(same_day))
        );
        let tomorrow = parse_iso_epoch("2026-06-12T04:18:00Z").expect("tomorrow");
        assert_eq!(
            reset_label(tomorrow, now),
            format!("Resets in 14h 32m ({})", local_timestamp_label(tomorrow))
        );
        let future = parse_iso_epoch("2026-07-01T16:31:00Z").expect("future");
        assert_eq!(
            reset_label(future, now),
            format!("Resets in 20d 2h ({})", local_timestamp_label(future))
        );
        assert_eq!(reset_label(now, now), "Resets now");
    }

    #[test]
    fn claude_oauth_credentials_parse_subscription_label() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("claude.json");
        fs::write(
            &path,
            serde_json::json!({
                "claudeAiOauth": {
                    "accessToken": "access",
                    "subscriptionType": "claude_max"
                }
            })
            .to_string(),
        )
        .expect("write auth");

        let credentials = load_claude_oauth_credentials(&path).expect("credentials");

        assert_eq!(credentials.access_token, "access");
        assert_eq!(credentials.subscription_type.as_deref(), Some("Claude Max"));
    }

    #[test]
    fn claude_oauth_credentials_fall_back_to_rate_limit_tier() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("claude.json");
        fs::write(
            &path,
            serde_json::json!({
                "claudeAiOauth": {
                    "accessToken": "access",
                    "rateLimitTier": "max"
                }
            })
            .to_string(),
        )
        .expect("write auth");

        let credentials = load_claude_oauth_credentials(&path).expect("credentials");

        assert_eq!(credentials.access_token, "access");
        assert_eq!(credentials.subscription_type.as_deref(), Some("Max"));
    }

    #[test]
    fn claude_code_user_agent_parses_cli_version() {
        assert_eq!(
            claude_code_version_from_text("Claude Code 2.1.7\n").as_deref(),
            Some("2.1.7")
        );
        assert_eq!(
            claude_code_user_agent_with(|command, args, timeout| {
                assert_eq!(command, "claude");
                assert_eq!(args, ["--version"]);
                assert_eq!(timeout, CLAUDE_VERSION_TIMEOUT);
                Ok(CliOutput {
                    success: true,
                    exit_code: Some(0),
                    stdout: "Claude Code 2.2.0".to_owned(),
                    stderr: String::new(),
                })
            })
            .as_deref(),
            Some("claude-code/2.2.0")
        );
    }

    #[test]
    fn amp_cli_usage_parser_maps_free_and_credit_rows() {
        let usage = parse_amp_usage_output(
            "Signed in as person@example.com (handle)\n\
             Amp Free: $2.42/$10 remaining (replenishes +$0.42/hour) - https://ampcode.com/settings#amp-free\n\
             Individual credits: $0.33 remaining (set up automatic top-up to avoid running out)\n",
        )
        .expect("Amp usage");

        assert_eq!(
            usage.account_label.as_deref(),
            Some("person@example.com (handle)")
        );
        let now = 1_781_185_560;
        let buckets = usage.buckets(now);
        assert_eq!(buckets[0].label, "Amp Free");
        assert_eq!(buckets[0].used_label.as_deref(), Some("$7.58"));
        assert_eq!(buckets[0].limit_label.as_deref(), Some("$10"));
        assert_eq!(buckets[0].remaining_percent, Some(24));
        assert_eq!(
            buckets[0].reset_label.as_deref(),
            amp_free_reset_label(2.42, 10.0, Some(0.42), now).as_deref()
        );
        assert_eq!(buckets[0].pace_label, None);
        assert_eq!(buckets[1].label, "Individual credits");
        assert_eq!(buckets[1].limit_label.as_deref(), Some("$0.33"));
        assert_eq!(
            buckets[1].pace_label.as_deref(),
            Some("Individual credits: $0.33")
        );
    }

    #[test]
    fn cli_output_collector_treats_reaped_child_as_success() {
        let output = collect_cli_output(
            "amp",
            None,
            thread::spawn(|| Ok("usage rows".to_owned())),
            thread::spawn(|| Ok(String::new())),
        )
        .expect("cli output");

        assert!(output.success);
        assert_eq!(output.exit_code, None);
        assert_eq!(output.stdout, "usage rows");
    }

    #[test]
    fn amp_api_usage_maps_display_balance_info() {
        let usage = AmpApiUsage::from_value(serde_json::json!({
            "result": {
                "user": { "email": "person@example.com" },
                "ampFree": {
                    "ampFreeRemaining": 4.94,
                    "ampFreeLimit": 10.0,
                    "hourlyReplenishment": 0.42
                },
                "credits": {
                    "individualCredits": 1.25
                }
            }
        }))
        .expect("Amp API usage");

        assert_eq!(usage.account_label.as_deref(), Some("person@example.com"));
        let now = 1_781_185_560;
        let buckets = usage.buckets(now);
        assert_eq!(buckets[0].label, "Amp Free");
        assert_eq!(buckets[0].used_label.as_deref(), Some("$5.06"));
        assert_eq!(buckets[0].limit_label.as_deref(), Some("$10"));
        assert_eq!(buckets[0].remaining_percent, Some(49));
        assert_eq!(
            buckets[0].reset_label.as_deref(),
            amp_free_reset_label(4.94, 10.0, Some(0.42), now).as_deref()
        );
        assert_eq!(buckets[0].pace_label, None);
        assert_eq!(buckets[1].label, "Individual credits");
        assert_eq!(buckets[1].limit_label.as_deref(), Some("$1.25"));
        assert_eq!(
            buckets[1].pace_label.as_deref(),
            Some("Individual credits: $1.25")
        );
    }

    #[test]
    fn zai_quota_response_maps_token_session_and_time_limits() {
        let quota: ZaiQuotaResponse = serde_json::from_value(serde_json::json!({
            "code": 200,
            "success": true,
            "msg": "ok",
            "data": {
                "planName": "Coding Pro",
                "limits": [
                    {
                        "type": "TOKENS_LIMIT",
                        "unit": 5,
                        "number": 300,
                        "usage": 1000,
                        "currentValue": 250,
                        "remaining": 750,
                        "percentage": 25,
                        "nextResetTime": 1_781_189_520_000_i64
                    },
                    {
                        "type": "TOKENS_LIMIT",
                        "unit": 6,
                        "number": 1,
                        "usage": 10000,
                        "currentValue": 9000,
                        "remaining": 1000,
                        "percentage": 90,
                        "nextResetTime": 1_781_798_400_000_i64
                    },
                    {
                        "type": "TIME_LIMIT",
                        "unit": 5,
                        "number": 1,
                        "usage": 120,
                        "currentValue": 30,
                        "remaining": 90,
                        "percentage": 25
                    }
                ]
            }
        }))
        .expect("valid Z.AI quota");

        let buckets = quota.buckets(1_781_185_560);

        assert_eq!(quota.plan_name().as_deref(), Some("Coding Pro"));
        assert_eq!(buckets[0].label, "Tokens");
        assert_eq!(buckets[0].remaining_percent, Some(10));
        assert_eq!(buckets[0].pace_label, None);
        assert_eq!(buckets[1].label, "MCP");
        assert_eq!(buckets[1].remaining_percent, Some(75));
        assert_eq!(
            buckets[1].pace_label.as_deref(),
            Some("30 / 120 (90 remaining)")
        );
        assert_eq!(buckets[2].label, "5-hour");
        assert_eq!(buckets[2].remaining_percent, Some(75));
        assert_eq!(buckets[2].pace_label, None);
    }

    #[test]
    fn zai_url_normalization_accepts_hosts_and_full_urls() {
        assert_eq!(
            normalize_url_or_host("open.bigmodel.cn", "api/monitor/usage/quota/limit"),
            "https://open.bigmodel.cn/api/monitor/usage/quota/limit"
        );
        assert_eq!(
            normalize_url_or_host("https://example.test/custom", ""),
            "https://example.test/custom"
        );
        assert_eq!(
            normalize_url_or_host(
                &zai_quota_host("https://api.z.ai/api/anthropic"),
                "api/monitor/usage/quota/limit"
            ),
            "https://api.z.ai/api/monitor/usage/quota/limit"
        );
        assert_eq!(
            resolve_zai_quota_url_from(Some("https://example.test/quota"), None),
            "https://example.test/quota"
        );
    }

    #[test]
    fn kimi_usage_response_maps_weekly_and_rate_limit() {
        let usage: KimiUsageResponse = serde_json::from_value(serde_json::json!({
            "usages": [{
                "scope": "FEATURE_CODING",
                "detail": {
                    "limit": "1000",
                    "used": "220",
                    "remaining": "780",
                    "resetTime": "2026-06-18T12:00:00Z"
                },
                "limits": [{
                    "window": { "duration": 300, "timeUnit": "TIME_UNIT_MINUTE" },
                    "detail": {
                        "limit": "200",
                        "remaining": "150",
                        "resetTime": "2026-06-11T16:00:00Z"
                    }
                }]
            }]
        }))
        .expect("valid Kimi usage");

        let buckets = usage.buckets(1_781_185_560);

        assert_eq!(buckets[0].label, "Weekly");
        assert_eq!(buckets[0].used_label.as_deref(), Some("220"));
        assert_eq!(buckets[0].limit_label.as_deref(), Some("1.0K"));
        assert_eq!(buckets[0].remaining_percent, Some(78));
        assert_eq!(buckets[0].pace_label, None);
        assert_eq!(buckets[1].label, "Rate Limit");
        assert_eq!(buckets[1].used_label.as_deref(), Some("50"));
        assert_eq!(buckets[1].remaining_percent, Some(75));
        assert_eq!(buckets[1].pace_label.as_deref(), Some("30% in reserve"));
    }

    #[test]
    fn kimi_local_token_loader_skips_expired_tokens() {
        let value = serde_json::json!({
            "access_token": "expired-token",
            "expires_at": 1_781_000_000.0
        });

        assert_eq!(kimi_local_token_from_value(&value, 1_781_200_000), None);
    }

    #[test]
    fn kimi_local_token_loader_accepts_unexpired_tokens() {
        let value = serde_json::json!({
            "access_token": "fresh-token",
            "expires_at": 1_781_300_000
        });

        assert_eq!(
            kimi_local_token_from_value(&value, 1_781_200_000).as_deref(),
            Some("fresh-token")
        );
    }

    #[test]
    fn kimi_local_token_loader_normalizes_millisecond_expiry() {
        let value = serde_json::json!({
            "access_token": "fresh-ms-token",
            "expires_at": 1_781_300_000_000_i64
        });

        assert_eq!(
            kimi_local_token_from_value(&value, 1_781_200_000).as_deref(),
            Some("fresh-ms-token")
        );
    }

    #[test]
    fn minimax_usage_response_maps_model_remains() {
        let usage: MiniMaxUsageResponse = serde_json::from_value(serde_json::json!({
            "base_resp": { "status_code": 0 },
            "data": {
                "current_subscribe_title": "MiniMax Pro",
                "model_remains": [{
                    "model_name": "MiniMax Text",
                    "current_interval_total_count": 100,
                    "current_interval_usage_count": 60,
                    "current_interval_status": 0,
                    "start_time": 1781172000,
                    "end_time": 1781186400,
                    "current_weekly_total_count": 700,
                    "current_weekly_usage_count": 630,
                    "current_weekly_remaining_percent": 90,
                    "weekly_start_time": 1780761600,
                    "weekly_end_time": 1781366400
                }]
            }
        }))
        .expect("valid MiniMax usage");

        usage.validate().expect("valid quota response");
        let buckets = usage.buckets(1_781_185_560);

        assert_eq!(usage.plan_name().as_deref(), Some("MiniMax Pro"));
        assert_eq!(buckets[0].label, "MiniMax Text");
        assert_eq!(buckets[0].used_label.as_deref(), Some("60"));
        assert_eq!(buckets[0].limit_label.as_deref(), Some("100"));
        assert_eq!(buckets[0].remaining_percent, Some(40));
        assert_eq!(buckets[0].pace_label.as_deref(), Some("Usage: 60 / 100"));
        assert_eq!(buckets.len(), 1);
    }

    #[test]
    fn minimax_usage_response_maps_live_root_model_remains() {
        let usage: MiniMaxUsageResponse = serde_json::from_value(serde_json::json!({
            "model_remains": [
                {
                    "model_name": "general",
                    "current_interval_total_count": 0,
                    "current_interval_usage_count": 0,
                    "current_interval_remaining_percent": 100,
                    "current_interval_status": 1,
                    "remains_time": 14_400_000,
                    "current_weekly_total_count": 0,
                    "current_weekly_usage_count": 1,
                    "current_weekly_remaining_percent": 99,
                    "current_weekly_status": 1,
                    "weekly_remains_time": 345_600_000
                },
                {
                    "model_name": "video",
                    "current_interval_total_count": 5,
                    "current_interval_usage_count": 0,
                    "current_interval_remaining_percent": 100,
                    "current_interval_status": 1,
                    "remains_time": 28_800_000,
                    "current_weekly_total_count": 35,
                    "current_weekly_usage_count": 0,
                    "current_weekly_remaining_percent": 100,
                    "current_weekly_status": 1,
                    "weekly_remains_time": 345_600_000
                }
            ],
            "base_resp": { "status_code": 0, "status_msg": "success" }
        }))
        .expect("valid MiniMax usage");

        usage.validate().expect("valid quota response");
        let buckets = usage.buckets(1_782_315_600);

        assert_eq!(
            buckets
                .iter()
                .map(|bucket| {
                    (
                        bucket.label.as_str(),
                        bucket.remaining_percent,
                        bucket.pace_label.as_deref(),
                    )
                })
                .collect::<Vec<_>>(),
            vec![
                ("General · 5h", Some(100), Some("Usage: 0 / 100")),
                ("General · Weekly", Some(99), Some("Usage: 1 / 100")),
                ("Video", Some(100), Some("Usage: 0 / 5")),
            ]
        );
    }

    #[test]
    fn minimax_remains_urls_accept_override_and_api_host_alias() {
        assert_eq!(
            resolve_minimax_remains_urls_from(Some("https://example.test/custom"), None),
            vec!["https://example.test/custom"]
        );

        assert_eq!(
            resolve_minimax_remains_urls_from(None, Some("https://api.minimax.io/anthropic")),
            vec![
                "https://api.minimax.io/v1/token_plan/remains",
                "https://api.minimax.io/v1/api/openplatform/coding_plan/remains"
            ]
        );
    }
}
