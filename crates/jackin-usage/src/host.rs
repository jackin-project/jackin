// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Capsule-free host usage orchestration for the macOS menu-bar app and CLI.
//!
//! Reuses [`crate::usage::UsageCache`] probes, cache, cooldown, and
//! `FocusedUsageView` shaping. State roots live under the operator jackin
//! data dir (not container `/jackin/...` paths).

mod accounts;

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use jackin_core::Agent;
use jackin_protocol::Provider;
use jackin_protocol::control::{FocusedUsageView, UsageSeverity};

use crate::usage::{
    UsageCache, UsageFormatPrefs, UsageRefreshTarget, compact_duration_label, estimate_caption,
    exact_reset_parenthetical, percent_headline, provider_display_label, reset_label_with_prefs,
    usage_display_status_label, usage_status_storage_label,
};

pub use accounts::{
    HostAccountDescriptor, account_key_for_view, min_remaining, short_account_identity,
};

/// Relative data-dir subtree for menu-bar durable state.
pub const HOST_USAGE_STATE_REL: &str = "usage-menu-bar";

/// Surfaces the host menu bar may show (excludes `Unsupported`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum HostSurfaceId {
    /// Anthropic / `Claude`.
    Claude,
    /// `OpenAI` / `Codex`.
    Codex,
    /// Amp.
    Amp,
    /// xAI / Grok Build.
    Grok,
    /// GLM / Z.AI routed provider.
    Zai,
    /// Kimi.
    Kimi,
    /// `MiniMax` routed provider.
    Minimax,
    /// `OpenCode`.
    OpenCode,
}

impl HostSurfaceId {
    /// Every host surface in stable UI order.
    pub const ALL: &'static [Self] = &[
        Self::Claude,
        Self::Codex,
        Self::Amp,
        Self::Grok,
        Self::Zai,
        Self::Kimi,
        Self::Minimax,
        Self::OpenCode,
    ];

    /// The canonical seven-provider Desktop glance order (Capsule tab order).
    /// `OpenCode` is intentionally excluded from the Desktop item contract.
    pub const DESKTOP_PROVIDER_ORDER: &'static [Self] = &[
        Self::Codex,
        Self::Claude,
        Self::Amp,
        Self::Grok,
        Self::Zai,
        Self::Kimi,
        Self::Minimax,
    ];

    /// Stable machine id (`claude`, `codex`, `zai`, …).
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Amp => "amp",
            Self::Grok => "grok",
            Self::Zai => "zai",
            Self::Kimi => "kimi",
            Self::Minimax => "minimax",
            Self::OpenCode => "opencode",
        }
    }

    /// Human label matching Capsule usage tabs.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::Codex => "Codex",
            Self::Amp => "Amp",
            Self::Grok => "Grok Build",
            Self::Zai => "GLM / Z.AI",
            Self::Kimi => "Kimi",
            Self::Minimax => "MiniMax",
            Self::OpenCode => "OpenCode",
        }
    }

    /// Two-character menu-bar prefix for the compact status item (HIG width).
    #[must_use]
    pub const fn compact_prefix(self) -> &'static str {
        match self {
            Self::Claude => "Cl",
            Self::Codex => "Cx",
            Self::Amp => "Am",
            Self::Grok => "Gr",
            Self::Zai => "ZA",
            Self::Kimi => "Ki",
            Self::Minimax => "MM",
            Self::OpenCode => "OC",
        }
    }

    /// Agent slug for `UsageRefreshTarget` (Z.AI/MiniMax route via a dummy agent
    /// + provider label — `resolve_surface` keys on the provider first).
    #[must_use]
    pub const fn agent_slug(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Amp => "amp",
            Self::Grok => "grok",
            Self::Zai | Self::Minimax => "codex",
            Self::Kimi => "kimi",
            Self::OpenCode => "opencode",
        }
    }

    /// Optional provider label for surface resolution.
    #[must_use]
    pub const fn provider_label(self) -> Option<&'static str> {
        match self {
            Self::Claude => Some("Claude"),
            Self::Codex => Some("Codex"),
            Self::Amp => Some("Amp"),
            Self::Grok => Some("Grok Build"),
            Self::Zai => Some("GLM / Z.AI"),
            Self::Kimi => Some("Kimi"),
            Self::Minimax => Some("MiniMax"),
            Self::OpenCode => Some("OpenCode"),
        }
    }

    /// Parse a stable id; unknown → `None`.
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|surface| surface.id() == id)
    }

    /// Map jackin agent runtimes to their primary surface (not Z.AI/MiniMax).
    #[must_use]
    pub const fn from_agent(agent: Agent) -> Self {
        match agent {
            Agent::Claude => Self::Claude,
            Agent::Codex => Self::Codex,
            Agent::Amp => Self::Amp,
            Agent::Kimi => Self::Kimi,
            Agent::Opencode => Self::OpenCode,
            Agent::Grok => Self::Grok,
        }
    }

    fn refresh_target(self) -> UsageRefreshTarget {
        UsageRefreshTarget {
            agent: self.agent_slug().to_owned(),
            provider: self.provider_label().map(str::to_owned),
        }
    }
}

/// Descriptor returned to `UniFFI` / CLI (no secrets).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostSurfaceDescriptor {
    /// Stable id (`claude`).
    pub id: String,
    /// Display label (for example `Claude`).
    pub label: String,
    /// Agent slug used for probes.
    pub agent: String,
    /// Provider label when set.
    pub provider: Option<String>,
    /// Whether the surface is currently enabled for refresh/bar.
    pub enabled: bool,
}

/// Coarse host event for the presentation poll loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostUsageEvent {
    /// Monotonic sequence.
    pub sequence: u64,
    /// `snapshot_updated` | `probe_failed` | `enabled_changed` | `runtime_ready`.
    pub kind: String,
    /// Surface id when relevant.
    pub surface_id: Option<String>,
    /// Optional detail (error message, never credentials).
    pub detail: Option<String>,
}

/// Bounded event batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostEventBatch {
    /// Next cursor for the client.
    pub next_cursor: u64,
    /// Events in `(cursor, cursor+max]`.
    pub events: Vec<HostUsageEvent>,
    /// Client must resync when true.
    pub resync_required: bool,
}

/// Open configuration for the host runtime.
#[derive(Debug, Clone)]
pub struct HostRuntimeConfig {
    /// jackin data dir (`~/.jackin/data` or test root).
    pub data_dir: PathBuf,
    /// Minimum refresh interval floor (seconds). Clamped to ≥ 60.
    pub refresh_floor_secs: u64,
    /// Initially enabled surface ids; empty → all host surfaces.
    pub enabled_surface_ids: Vec<String>,
    /// Whether this runtime may dispatch live provider probes. `Disabled` is
    /// used by the isolated launch smoke test so an accidental refresh cannot
    /// reach any credential/file/env/CLI/network/Keychain resolution.
    pub probe_policy: HostProbePolicy,
}

/// Whether a host runtime may dispatch live provider probes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HostProbePolicy {
    /// Normal operation: refreshes dispatch provider probes.
    #[default]
    Live,
    /// Smoke/defense-in-depth: refresh is a no-probe no-op and never due.
    Disabled,
}

impl HostRuntimeConfig {
    /// Default host layout under `data_dir` (live probes).
    #[must_use]
    pub fn under_data_dir(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
            refresh_floor_secs: 300,
            enabled_surface_ids: Vec::new(),
            probe_policy: HostProbePolicy::Live,
        }
    }
}

/// Snapshot store path under the host data dir.
#[must_use]
pub fn host_snapshot_store_path(data_dir: &Path) -> PathBuf {
    data_dir.join(HOST_USAGE_STATE_REL).join("snapshots.db")
}

/// Materialized accounts JSON path under the host data dir.
#[must_use]
pub fn host_accounts_path(data_dir: &Path) -> PathBuf {
    data_dir.join(HOST_USAGE_STATE_REL).join("accounts.json")
}

const MAX_EVENT_LOG: usize = 4_096;
const MAX_EVENT_BATCH: u32 = 256;

/// One enabled-surface overview row for jackin❯ Desktop (popover + Usage window).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostOverviewRow {
    /// Machine surface id (`claude`, `codex`, …).
    pub surface_id: String,
    /// Remapped display label (`OpenAI`, `Anthropic`, …).
    pub display_label: String,
    /// Percent headline or empty when only a status word applies.
    pub headline: String,
    /// Countdown-form reset line when known.
    pub reset_label: Option<String>,
    /// Exact clock parenthetical when `resets_at` is known, e.g. `(Jul 28, 17:02)`.
    pub exact_reset: Option<String>,
    /// Storage status word (`fresh`, `stale`, `needs_login`, …).
    pub status_word: String,
    /// Worst bucket severity: `normal` | `warn` | `danger`.
    pub severity: String,
}

/// One selected-account-aware provider projection for native usage surfaces
/// (the Desktop status bar, popover, and Usage window all consume this same
/// Rust-owned row rather than choosing providers or formatting quota in Swift).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostProviderGlanceRow {
    /// Stable provider machine identifier (`codex`, `claude`, …).
    pub surface_id: String,
    /// Stable provider icon key (closed domain, equals `surface_id`).
    pub icon_key: String,
    /// Rust-owned provider display name (`OpenAI`, `Anthropic`, …).
    pub display_label: String,
    /// Rust-owned selected-account label (empty when none).
    pub account_label: String,
    /// Provider plan label when known.
    pub plan_label: Option<String>,
    /// Selected semantic glance percentage (Weekly for six, Daily for Amp),
    /// when the required bucket exists.
    pub glance_remaining_percent: Option<u8>,
    /// Verbatim menu-bar value (`57%` or `–`).
    pub bar_label: String,
    /// Verbatim detail headline (`57% left` or `–`).
    pub headline: String,
    /// Relative reset label when the glance bucket carries a reset.
    pub reset_label: Option<String>,
    /// Exact-clock reset parenthetical when the glance bucket carries a reset.
    pub exact_reset: Option<String>,
    /// Stable machine status word.
    pub status_word: String,
    /// Whether this provider is the cold refreshing placeholder.
    pub is_refreshing: bool,
    /// Rust-owned human status label.
    pub status_label: String,
    /// Stable presentation-severity key (`normal` | `warn` | `danger`).
    pub severity: String,
    /// Rust-owned freshness label.
    pub updated_label: String,
    /// Rust-owned last error, when present.
    pub last_error: Option<String>,
    /// Whether the native bar value is visually dimmed (stale/error).
    pub dimmed: bool,
}

/// Driving bucket for compact/overview labels: min remaining + its reset epoch.
#[derive(Debug, Clone, Copy)]
struct DrivingBucket {
    remaining: u8,
    resets_at: Option<i64>,
}

/// Capsule-free host usage runtime.
#[derive(Debug)]
pub struct HostUsageRuntime {
    cache: UsageCache,
    enabled: HashSet<String>,
    provider_keys: BTreeMap<Provider, String>,
    events: VecDeque<HostUsageEvent>,
    next_seq: u64,
    refresh_floor_secs: u64,
    /// Last time a network-bearing refresh completed (floor gate).
    last_refresh: Option<Instant>,
    /// Presentation-time format prefs (not persisted).
    format_prefs: UsageFormatPrefs,
    open: bool,
    /// Absolute jackin data dir (for snapshot store + selected-accounts prefs).
    data_dir: Option<PathBuf>,
    /// Selected account key per surface id (persisted).
    selected_accounts: HashMap<String, String>,
    /// Whether live probes may dispatch (smoke mode disables them).
    probe_policy: HostProbePolicy,
    /// Provider ids currently auto-detected for the Desktop glance list.
    /// Runtime-only (never persisted); holds ids, never display strings.
    desktop_detected_surfaces: HashSet<String>,
}

impl HostUsageRuntime {
    /// Construct a closed runtime (call [`Self::open`] before use).
    #[must_use]
    pub fn new() -> Self {
        Self {
            cache: UsageCache::default(),
            enabled: HashSet::new(),
            provider_keys: BTreeMap::new(),
            events: VecDeque::new(),
            next_seq: 0,
            refresh_floor_secs: 300,
            last_refresh: None,
            format_prefs: UsageFormatPrefs::default(),
            open: false,
            data_dir: None,
            selected_accounts: HashMap::new(),
            probe_policy: HostProbePolicy::Live,
            desktop_detected_surfaces: HashSet::new(),
        }
    }

    /// Open with host paths; enables all surfaces when config list empty.
    pub fn open(&mut self, config: HostRuntimeConfig) -> Result<(), String> {
        let snapshot_path = host_snapshot_store_path(&config.data_dir);
        let accounts_path = host_accounts_path(&config.data_dir);
        if let Some(parent) = snapshot_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| format!("create host usage state dir: {err}"))?;
        }
        self.cache.set_usage_snapshot_store_path(snapshot_path);
        self.cache.set_accounts_materialize_path(accounts_path);
        self.refresh_floor_secs = config.refresh_floor_secs.max(60);
        self.last_refresh = None;
        self.enabled.clear();
        if config.enabled_surface_ids.is_empty() {
            for surface in HostSurfaceId::ALL {
                self.enabled.insert(surface.id().to_owned());
            }
        } else {
            for id in config.enabled_surface_ids {
                if HostSurfaceId::from_id(&id).is_some() {
                    self.enabled.insert(id);
                }
            }
        }
        // Prove Agent::ALL is covered by primary surfaces.
        for agent in Agent::ALL {
            let surface = HostSurfaceId::from_agent(*agent);
            debug_assert!(
                HostSurfaceId::ALL.contains(&surface),
                "agent {} missing host surface",
                agent.slug()
            );
        }
        let selected_path = accounts::selected_accounts_path(&config.data_dir);
        self.selected_accounts = accounts::load_selected_accounts(&selected_path);
        self.probe_policy = config.probe_policy;
        self.desktop_detected_surfaces.clear();
        self.data_dir = Some(config.data_dir);
        self.open = true;
        self.push_event("runtime_ready", None, None);
        Ok(())
    }

    /// Whether the runtime accepted [`Self::open`].
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// List surfaces with enable flags.
    pub fn list_surfaces(&self) -> Result<Vec<HostSurfaceDescriptor>, String> {
        self.require_open()?;
        Ok(HostSurfaceId::ALL
            .iter()
            .copied()
            .map(|surface| HostSurfaceDescriptor {
                id: surface.id().to_owned(),
                label: surface.label().to_owned(),
                agent: surface.agent_slug().to_owned(),
                provider: surface.provider_label().map(str::to_owned),
                enabled: self.enabled.contains(surface.id()),
            })
            .collect())
    }

    /// Enable or disable a surface for bar + refresh set.
    pub fn set_enabled(&mut self, surface_id: &str, enabled: bool) -> Result<(), String> {
        self.require_open()?;
        let surface = HostSurfaceId::from_id(surface_id)
            .ok_or_else(|| format!("unknown surface: {surface_id}"))?;
        if enabled {
            self.enabled.insert(surface.id().to_owned());
        } else {
            self.enabled.remove(surface.id());
        }
        self.push_event(
            "enabled_changed",
            Some(surface.id()),
            Some(if enabled { "enabled" } else { "disabled" }.to_owned()),
        );
        Ok(())
    }

    /// Inject optional provider API keys (Z.AI / `MiniMax` / Kimi) without env.
    pub fn set_provider_key(&mut self, provider: Provider, key: String) {
        if key.trim().is_empty() {
            self.provider_keys.remove(&provider);
        } else {
            self.provider_keys.insert(provider, key);
        }
    }

    /// Seed a fixture view (tests / offline QA). Does not hit the network.
    pub fn inject_snapshot(
        &mut self,
        surface_id: &str,
        view: FocusedUsageView,
    ) -> Result<(), String> {
        self.require_open()?;
        let surface = HostSurfaceId::from_id(surface_id)
            .ok_or_else(|| format!("unknown surface: {surface_id}"))?;
        self.cache
            .insert_snapshot_for_test(surface.agent_slug(), surface.provider_label(), view);
        self.push_event(
            "snapshot_updated",
            Some(surface.id()),
            Some("injected".to_owned()),
        );
        Ok(())
    }

    /// Refresh enabled surfaces (blocking probes when due).
    ///
    /// When `force` is false, a call within [`Self::refresh_floor_secs`] of the
    /// last network refresh is a no-op (poll-safe). When `force` is true (manual
    /// Refresh / Settings), the floor is bypassed and targets are marked due.
    pub fn refresh(&mut self, surface_id: Option<&str>, force: bool) -> Result<(), String> {
        self.require_open()?;
        // Defense in depth: a `Disabled` runtime never dispatches probes, so an
        // accidental refresh cannot reach any credential/file/env/CLI/network/
        // Keychain resolution. Return a successful no-probe event.
        if self.probe_policy == HostProbePolicy::Disabled {
            self.push_event(
                "refresh_skipped",
                surface_id,
                Some("probes disabled".to_owned()),
            );
            return Ok(());
        }
        if !force && let Some(last) = self.last_refresh {
            let floor = Duration::from_secs(self.refresh_floor_secs);
            if last.elapsed() < floor {
                return Ok(());
            }
        }
        let now = Instant::now();
        let targets = self.refresh_targets(surface_id)?;
        for target in &targets {
            self.cache.request_account_refresh(target, now);
        }
        self.cache
            .refresh_active_account_snapshots(&targets, None, &self.provider_keys, now);
        self.last_refresh = Some(now);
        for target in &targets {
            let surface = surface_for_target(target);
            let view = self
                .cache
                .focused_snapshot(Some(&target.agent), target.provider.as_deref());
            let kind = if view.last_error.is_some()
                && matches!(
                    view.status,
                    jackin_protocol::control::UsageSnapshotStatus::Error
                        | jackin_protocol::control::UsageSnapshotStatus::Unavailable
                        | jackin_protocol::control::UsageSnapshotStatus::NeedsLogin
                        | jackin_protocol::control::UsageSnapshotStatus::NeedsSecret
                ) {
                "probe_failed"
            } else {
                "snapshot_updated"
            };
            self.push_event(
                kind,
                surface.map(HostSurfaceId::id),
                view.last_error.clone(),
            );
        }
        Ok(())
    }

    /// Update the refresh floor (seconds). Clamped to ≥ 60.
    pub fn set_refresh_floor_secs(&mut self, secs: u64) -> Result<(), String> {
        self.require_open()?;
        let clamped = secs.max(60);
        self.refresh_floor_secs = clamped;
        self.push_event(
            "config_changed",
            None,
            Some(format!("refresh_floor_secs={clamped}")),
        );
        Ok(())
    }

    /// Whether a non-forced refresh would hit the network (floor elapsed or never).
    #[must_use]
    pub fn refresh_due(&self) -> bool {
        if self.probe_policy == HostProbePolicy::Disabled {
            return false;
        }
        match self.last_refresh {
            None => true,
            Some(last) => last.elapsed() >= Duration::from_secs(self.refresh_floor_secs),
        }
    }

    /// Cached snapshot for one surface (honest refreshing/unavailable).
    ///
    /// When a non-live account is selected, returns that account's durable view
    /// (multi-account Desktop); otherwise the live host-login snapshot.
    pub fn snapshot(&mut self, surface_id: &str) -> Result<FocusedUsageView, String> {
        self.require_open()?;
        let surface = HostSurfaceId::from_id(surface_id)
            .ok_or_else(|| format!("unknown surface: {surface_id}"))?;
        if !self.enabled.contains(surface.id()) {
            return Err(format!("surface disabled: {surface_id}"));
        }
        let live = self
            .cache
            .focused_snapshot(Some(surface.agent_slug()), surface.provider_label());
        let store_path = self
            .data_dir
            .as_ref()
            .map(|d| host_snapshot_store_path(d))
            .unwrap_or_default();
        // A local-only Claude resolution (Keychain denial, missing credential,
        // or an anonymous credential) never restores a durable/shared account
        // view over the live local result.
        if self
            .cache
            .active_snapshot_policy(surface.agent_slug(), surface.provider_label())
            .is_local_only()
        {
            return Ok(live);
        }
        let selected = self.selected_accounts.get(surface.id()).map(String::as_str);
        Ok(accounts::resolve_account_view(
            surface,
            selected,
            live,
            &store_path,
        ))
    }

    /// List known accounts for one surface (or all surfaces when `None`).
    ///
    /// Sources: live host login, durable menu-bar store, shared container snapshots.
    pub fn list_accounts(
        &mut self,
        surface_id: Option<&str>,
    ) -> Result<Vec<HostAccountDescriptor>, String> {
        self.require_open()?;
        let store_path = self
            .data_dir
            .as_ref()
            .map(|d| host_snapshot_store_path(d))
            .unwrap_or_default();
        let surfaces: Vec<HostSurfaceId> = match surface_id {
            Some(id) => {
                let surface =
                    HostSurfaceId::from_id(id).ok_or_else(|| format!("unknown surface: {id}"))?;
                vec![surface]
            }
            None => HostSurfaceId::ALL.to_vec(),
        };
        let mut out = Vec::new();
        for surface in surfaces {
            let live = self
                .cache
                .focused_snapshot(Some(surface.agent_slug()), surface.provider_label());
            let live_key = account_key_for_view(&live);
            // Local-only Claude scopes read no durable/shared history: only the
            // live row (when its identity is non-placeholder) is returned.
            let mut account_map = if self
                .cache
                .active_snapshot_policy(surface.agent_slug(), surface.provider_label())
                .is_local_only()
            {
                HashMap::new()
            } else {
                accounts::collect_account_views(surface, Some(&live), &store_path)
            };
            if !live_key.is_empty() {
                account_map
                    .entry(live_key.clone())
                    .or_insert_with(|| live.clone());
            }
            let mut keys: Vec<String> = account_map.keys().cloned().collect();
            keys.sort();
            let selected = self
                .selected_accounts
                .get(surface.id())
                .cloned()
                .filter(|k| keys.contains(k))
                .unwrap_or_else(|| live_key.clone());
            if !selected.is_empty() {
                self.selected_accounts
                    .entry(surface.id().to_owned())
                    .or_insert_with(|| selected.clone());
            }
            for key in keys {
                let view = account_map
                    .get(&key)
                    .cloned()
                    .unwrap_or_else(|| live.clone());
                let label = view.account.account_label.clone();
                let placeholder =
                    label.trim().is_empty() || label.eq_ignore_ascii_case("account unavailable");
                if placeholder && account_map.len() > 1 && key != live_key {
                    continue;
                }
                out.push(HostAccountDescriptor {
                    surface_id: surface.id().to_owned(),
                    account_key: key.clone(),
                    account_label: if placeholder {
                        "Current host login".to_owned()
                    } else {
                        label
                    },
                    plan_label: view.account.plan_label.clone(),
                    selected: key == selected,
                    remaining_percent: min_remaining(&view),
                    status_word: usage_status_storage_label(view.status).to_owned(),
                });
            }
        }
        Ok(out)
    }

    /// Select which account drives detail/snapshot for a surface (persisted).
    pub fn set_selected_account(
        &mut self,
        surface_id: &str,
        account_key: &str,
    ) -> Result<(), String> {
        self.require_open()?;
        let surface = HostSurfaceId::from_id(surface_id)
            .ok_or_else(|| format!("unknown surface: {surface_id}"))?;
        if account_key.is_empty() {
            self.selected_accounts.remove(surface.id());
        } else {
            self.selected_accounts
                .insert(surface.id().to_owned(), account_key.to_owned());
        }
        if let Some(dir) = &self.data_dir {
            let path = accounts::selected_accounts_path(dir);
            accounts::save_selected_accounts(&path, &self.selected_accounts)?;
        }
        self.push_event(
            "account_selected",
            Some(surface.id()),
            Some(account_key.to_owned()),
        );
        Ok(())
    }

    /// Compact bar label for one enabled surface, if known.
    pub fn status_bar_label(&mut self, surface_id: &str) -> Result<Option<String>, String> {
        self.require_open()?;
        let surface = HostSurfaceId::from_id(surface_id)
            .ok_or_else(|| format!("unknown surface: {surface_id}"))?;
        if !self.enabled.contains(surface.id()) {
            return Ok(None);
        }
        Ok(self
            .cache
            .focused_status_bar_label(Some(surface.agent_slug()), surface.provider_label()))
    }

    /// Merged compact bar text from enabled surfaces that have labels.
    pub fn merged_status_bar_label(&mut self) -> Result<String, String> {
        self.require_open()?;
        let mut parts = Vec::new();
        for surface in HostSurfaceId::ALL {
            if !self.enabled.contains(surface.id()) {
                continue;
            }
            if let Some(label) = self
                .cache
                .focused_status_bar_label(Some(surface.agent_slug()), surface.provider_label())
            {
                // Skip pure loading noise when other surfaces already contribute.
                if label == "refreshing" && !parts.is_empty() {
                    continue;
                }
                parts.push(format!("{}: {label}", surface.label()));
            }
        }
        if parts.is_empty() {
            Ok("jackin❯ usage".to_owned())
        } else {
            Ok(parts.join(" · "))
        }
    }

    /// Presentation-time format prefs (defaults match shipped Capsule strings).
    pub fn set_format_prefs(&mut self, prefs: UsageFormatPrefs) -> Result<(), String> {
        self.require_open()?;
        self.format_prefs = prefs;
        Ok(())
    }

    /// Current presentation-time format prefs.
    #[must_use]
    pub fn format_prefs(&self) -> UsageFormatPrefs {
        self.format_prefs
    }

    /// Short status-item label: enabled surface with the **least remaining**
    /// (lowest `remaining_percent` across its buckets). Default
    /// [`PercentStyle::Left`] shows remaining (e.g. `Cl 37%`);
    /// [`PercentStyle::Used`] shows used percent (e.g. `Cl 63%`).
    ///
    /// Never invents percentages — only uses Rust-provided `remaining_percent`.
    /// Empty when no enabled surface has a numeric remaining value (all
    /// unavailable / disabled / still refreshing without last-good data).
    /// Ties keep the earlier surface in [`HostSurfaceId::ALL`] order.
    /// Depleted (`remaining == 0`) with `resets_at` renders `Cl resets 1h 21m`.
    pub fn compact_status_bar_label(&mut self) -> Result<String, String> {
        self.require_open()?;
        let mut best: Option<(u8, HostSurfaceId, Option<i64>)> = None;
        for surface in HostSurfaceId::ALL.iter().copied() {
            if !self.enabled.contains(surface.id()) {
                continue;
            }
            let Some(drive) = self.driving_bucket_for(surface) else {
                continue;
            };
            match best {
                Some((best_remaining, _, _)) if drive.remaining >= best_remaining => {}
                _ => best = Some((drive.remaining, surface, drive.resets_at)),
            }
        }
        let prefs = self.format_prefs;
        Ok(match best {
            Some((remaining, surface, resets_at)) => {
                Self::format_compact_entry(surface, remaining, resets_at, prefs)
            }
            None => String::new(),
        })
    }

    /// Pinned-surface compact label (e.g. `Cx 59%` remaining / depleted form).
    /// `None` when disabled or no numeric remaining.
    pub fn compact_status_bar_label_for(
        &mut self,
        surface_id: &str,
    ) -> Result<Option<String>, String> {
        self.require_open()?;
        let surface = HostSurfaceId::from_id(surface_id)
            .ok_or_else(|| format!("unknown surface: {surface_id}"))?;
        if !self.enabled.contains(surface.id()) {
            return Ok(None);
        }
        let Some(drive) = self.driving_bucket_for(surface) else {
            return Ok(None);
        };
        Ok(Some(Self::format_compact_entry(
            surface,
            drive.remaining,
            drive.resets_at,
            self.format_prefs,
        )))
    }

    /// Worst-first multi-surface strip, capped, joined with ` · `.
    pub fn compact_status_bar_strip(&mut self, max: u32) -> Result<String, String> {
        self.require_open()?;
        let cap = max.clamp(1, 8) as usize;
        let prefs = self.format_prefs;
        let mut rows: Vec<(u8, HostSurfaceId, Option<i64>)> = Vec::new();
        for surface in HostSurfaceId::ALL.iter().copied() {
            if !self.enabled.contains(surface.id()) {
                continue;
            }
            if let Some(drive) = self.driving_bucket_for(surface) {
                rows.push((drive.remaining, surface, drive.resets_at));
            }
        }
        // Ascending remaining (worst first); ties keep ALL order (stable sort).
        rows.sort_by_key(|(remaining, surface, _)| {
            (
                *remaining,
                HostSurfaceId::ALL
                    .iter()
                    .position(|s| *s == *surface)
                    .unwrap_or(usize::MAX),
            )
        });
        let parts: Vec<String> = rows
            .into_iter()
            .take(cap)
            .map(|(remaining, surface, resets_at)| {
                Self::format_compact_entry(surface, remaining, resets_at, prefs)
            })
            .collect();
        Ok(parts.join(" · "))
    }

    /// Next network refresh relative to the floor (`Next update in …` / due).
    #[must_use]
    pub fn next_refresh_label(&self) -> String {
        match self.last_refresh {
            None => "Next update due".to_owned(),
            Some(last) => {
                let floor = Duration::from_secs(self.refresh_floor_secs);
                let elapsed = last.elapsed();
                if elapsed >= floor {
                    "Next update due".to_owned()
                } else {
                    let remain = floor.saturating_sub(elapsed);
                    let secs = i64::try_from(remain.as_secs()).unwrap_or(i64::MAX);
                    format!("Next update in {}", compact_duration_label(secs.max(0)))
                }
            }
        }
    }

    /// Overview rows for every **enabled** surface in `ALL` order.
    pub fn overview_rows(&mut self) -> Result<Vec<HostOverviewRow>, String> {
        self.require_open()?;
        let prefs = self.format_prefs;
        let now = chrono::Utc::now().timestamp();
        let mut rows = Vec::new();
        for surface in HostSurfaceId::ALL.iter().copied() {
            if !self.enabled.contains(surface.id()) {
                continue;
            }
            let view = self
                .cache
                .focused_snapshot(Some(surface.agent_slug()), surface.provider_label());
            let status_word = usage_status_storage_label(view.status).to_owned();
            let severity = worst_severity_label(&view);
            // Prefer remapping the account provider_label when present (OpenAI / Codex).
            let display_label = if view.account.provider_label.is_empty() {
                provider_display_label(surface.label()).to_owned()
            } else {
                provider_display_label(&view.account.provider_label).to_owned()
            };

            let mut headline = String::new();
            let mut reset_label = None;
            let mut exact_reset = None;
            if let Some(drive) = driving_bucket_from_view(&view) {
                // Optional model-scoped bucket name prefix (Fable, Sonnet, …).
                if let Some(prefix) = drive_label_prefix(&view, drive.remaining) {
                    headline.push_str(prefix);
                    headline.push(' ');
                }
                headline.push_str(&percent_headline(drive.remaining, prefs));
                if let Some(at) = drive.resets_at {
                    reset_label = Some(reset_label_with_prefs(at, now, prefs));
                    exact_reset = Some(exact_reset_parenthetical(at));
                }
            }

            rows.push(HostOverviewRow {
                surface_id: surface.id().to_owned(),
                display_label,
                headline,
                reset_label,
                exact_reset,
                status_word,
                severity,
            });
        }
        Ok(rows)
    }

    /// Detected providers in the canonical Desktop model order, each a
    /// selected-account-aware glance row. Iterates only
    /// [`HostSurfaceId::DESKTOP_PROVIDER_ORDER`], reads every row through the
    /// selected-account-aware [`Self::snapshot`], and re-evaluates detection on
    /// every call: affirmative evidence inserts membership, a non-refreshing
    /// view without evidence removes it, and the cold refreshing placeholder
    /// alone reuses prior membership so a refresh cannot drop a detected item.
    /// Returns an empty vector for zero detected providers.
    #[must_use = "the glance rows are the Desktop surface source"]
    pub fn provider_glance_rows(&mut self) -> Result<Vec<HostProviderGlanceRow>, String> {
        self.require_open()?;
        let prefs = self.format_prefs;
        let now = chrono::Utc::now().timestamp();
        let mut rows = Vec::new();
        for surface in HostSurfaceId::DESKTOP_PROVIDER_ORDER.iter().copied() {
            // A disabled surface has no glance row (snapshot rejects it).
            let Ok(view) = self.snapshot(surface.id()) else {
                self.desktop_detected_surfaces.remove(surface.id());
                continue;
            };
            let detected = if view_is_auto_detected(&view) {
                self.desktop_detected_surfaces
                    .insert(surface.id().to_owned());
                true
            } else if view.is_refreshing_placeholder() {
                self.desktop_detected_surfaces.contains(surface.id())
            } else {
                self.desktop_detected_surfaces.remove(surface.id());
                false
            };
            if detected {
                rows.push(build_provider_glance_row(surface, &view, now, prefs));
            }
        }
        Ok(rows)
    }

    /// Estimate honesty caption for one surface snapshot (presentation-time).
    pub fn estimate_caption_for(&mut self, surface_id: &str) -> Result<Option<String>, String> {
        let view = self.snapshot(surface_id)?;
        Ok(estimate_caption(&view))
    }

    fn driving_bucket_for(&mut self, surface: HostSurfaceId) -> Option<DrivingBucket> {
        let view = self
            .cache
            .focused_snapshot(Some(surface.agent_slug()), surface.provider_label());
        driving_bucket_from_view(&view)
    }

    /// Compact status token: prefix + percent matching format prefs.
    ///
    /// Default [`PercentStyle::Left`] uses **remaining** (OpenUsage/CodexBar
    /// dual-bucket stack semantics). [`PercentStyle::Used`] flips to used %.
    /// Depleted with `resets_at` keeps the countdown form; depleted without
    /// reset is `Cl 0%` (remaining) or `Cl 100%` (used).
    fn format_compact_entry(
        surface: HostSurfaceId,
        remaining: u8,
        resets_at: Option<i64>,
        prefs: UsageFormatPrefs,
    ) -> String {
        if remaining == 0 {
            if let Some(at) = resets_at {
                let now = chrono::Utc::now().timestamp();
                let secs = at.saturating_sub(now).max(0);
                return format!(
                    "{} resets {}",
                    surface.compact_prefix(),
                    compact_duration_label(secs)
                );
            }
            return match prefs.percent_style {
                crate::usage::PercentStyle::Left => {
                    format!("{} 0%", surface.compact_prefix())
                }
                crate::usage::PercentStyle::Used => {
                    format!("{} 100%", surface.compact_prefix())
                }
            };
        }
        let pct = match prefs.percent_style {
            crate::usage::PercentStyle::Left => remaining,
            crate::usage::PercentStyle::Used => 100u8.saturating_sub(remaining),
        };
        format!("{} {pct}%", surface.compact_prefix())
    }

    /// Poll events after `cursor` (exclusive), up to `max`.
    pub fn next_events(&mut self, cursor: u64, max: u32) -> Result<HostEventBatch, String> {
        self.require_open()?;
        let max = max.clamp(1, MAX_EVENT_BATCH) as usize;
        if self.events.is_empty() {
            return Ok(HostEventBatch {
                next_cursor: self.next_seq,
                events: Vec::new(),
                resync_required: false,
            });
        }
        let first = self.events.front().map_or(0, |e| e.sequence);
        if cursor + 1 < first {
            return Ok(HostEventBatch {
                next_cursor: self.next_seq,
                events: Vec::new(),
                resync_required: true,
            });
        }
        let events: Vec<HostUsageEvent> = self
            .events
            .iter()
            .filter(|event| event.sequence > cursor)
            .take(max)
            .cloned()
            .collect();
        let next_cursor = events.last().map_or(cursor, |event| event.sequence);
        Ok(HostEventBatch {
            next_cursor,
            events,
            resync_required: false,
        })
    }

    /// Refresh floor in seconds (clamped).
    #[must_use]
    pub fn refresh_floor_secs(&self) -> u64 {
        self.refresh_floor_secs
    }

    /// Shutdown; idempotent.
    pub fn shutdown(&mut self) {
        self.open = false;
        self.last_refresh = None;
        self.events.clear();
    }

    fn require_open(&self) -> Result<(), String> {
        if self.open {
            Ok(())
        } else {
            Err("runtime not open".to_owned())
        }
    }

    fn refresh_targets(&self, surface_id: Option<&str>) -> Result<Vec<UsageRefreshTarget>, String> {
        if let Some(id) = surface_id {
            let surface =
                HostSurfaceId::from_id(id).ok_or_else(|| format!("unknown surface: {id}"))?;
            if !self.enabled.contains(surface.id()) {
                return Err(format!("surface disabled: {id}"));
            }
            return Ok(vec![surface.refresh_target()]);
        }
        Ok(HostSurfaceId::ALL
            .iter()
            .copied()
            .filter(|surface| self.enabled.contains(surface.id()))
            .map(HostSurfaceId::refresh_target)
            .collect())
    }

    fn push_event(&mut self, kind: &str, surface_id: Option<&str>, detail: Option<String>) {
        self.next_seq = self.next_seq.saturating_add(1);
        self.events.push_back(HostUsageEvent {
            sequence: self.next_seq,
            kind: kind.to_owned(),
            surface_id: surface_id.map(str::to_owned),
            detail,
        });
        while self.events.len() > MAX_EVENT_LOG {
            self.events.pop_front();
        }
    }
}

impl Default for HostUsageRuntime {
    fn default() -> Self {
        Self::new()
    }
}

fn surface_for_target(target: &UsageRefreshTarget) -> Option<HostSurfaceId> {
    if let Some(provider) = target.provider.as_deref() {
        for surface in HostSurfaceId::ALL {
            if surface.provider_label() == Some(provider) {
                return Some(*surface);
            }
        }
    }
    HostSurfaceId::from_id(&target.agent)
}

/// Min-`remaining_percent` bucket (same selection as the legacy compact label).
fn driving_bucket_from_view(view: &FocusedUsageView) -> Option<DrivingBucket> {
    let mut best: Option<(u8, Option<i64>)> = None;
    for bucket in &view.buckets {
        let Some(remaining) = bucket.remaining_percent else {
            continue;
        };
        match best {
            Some((best_remaining, _)) if remaining >= best_remaining => {}
            _ => best = Some((remaining, bucket.resets_at)),
        }
    }
    best.map(|(remaining, resets_at)| DrivingBucket {
        remaining,
        resets_at,
    })
}

/// Model-scoped bucket label when the driving bucket has no status slot.
fn drive_label_prefix(view: &FocusedUsageView, remaining: u8) -> Option<&str> {
    view.buckets
        .iter()
        .find(|bucket| bucket.remaining_percent == Some(remaining) && bucket.status_slot.is_none())
        .map(|bucket| bucket.label.as_str())
        .filter(|label| !label.is_empty())
}

/// A view is auto-detected when it carries affirmative credential evidence (a
/// non-empty `credential_origin` that is not a `"needs …"` placeholder, even
/// under `Unsupported` status) or at least one bucket with a numeric/formatted
/// quota field. Bucket labels, pace/status prose, and non-Fresh status alone
/// are never evidence.
fn view_is_auto_detected(view: &FocusedUsageView) -> bool {
    let origin_affirmative = view
        .account
        .credential_origin
        .as_deref()
        .map(str::trim)
        .filter(|origin| !origin.is_empty())
        .is_some_and(|origin| !origin.to_ascii_lowercase().starts_with("needs "));
    let bucket_evidence = view.buckets.iter().any(|bucket| {
        bucket.remaining_percent.is_some()
            || bucket.used_label.is_some()
            || bucket.limit_label.is_some()
            || bucket.used_money.is_some()
            || bucket.limit_money.is_some()
            || bucket.reset_label.is_some()
            || bucket.resets_at.is_some()
    });
    origin_affirmative || bucket_evidence
}

/// Select the required semantic glance bucket: Weekly for the six non-Amp
/// providers and Daily for Amp. Never a Spend/Session/min-remaining or label
/// match — one provider's missing slot yields `–`, never a whole-list failure.
fn glance_bucket(
    surface: HostSurfaceId,
    view: &FocusedUsageView,
) -> Option<&jackin_protocol::control::QuotaBucketView> {
    let slot = if surface == HostSurfaceId::Amp {
        jackin_protocol::control::StatusSlot::Daily
    } else {
        jackin_protocol::control::StatusSlot::Weekly
    };
    view.buckets
        .iter()
        .find(|bucket| bucket.status_slot == Some(slot))
}

fn build_provider_glance_row(
    surface: HostSurfaceId,
    view: &FocusedUsageView,
    now: i64,
    prefs: UsageFormatPrefs,
) -> HostProviderGlanceRow {
    use jackin_protocol::control::UsageSnapshotStatus as Status;
    let display_label = if view.account.provider_label.is_empty() {
        provider_display_label(surface.label()).to_owned()
    } else {
        provider_display_label(&view.account.provider_label).to_owned()
    };
    let glance = glance_bucket(surface, view);
    let (bar_label, headline, glance_remaining_percent, reset_label, exact_reset) =
        match glance.and_then(|bucket| bucket.remaining_percent) {
            Some(percent) => {
                let (reset_label, exact_reset) =
                    glance
                        .and_then(|bucket| bucket.resets_at)
                        .map_or((None, None), |at| {
                            (
                                Some(reset_label_with_prefs(at, now, prefs)),
                                Some(exact_reset_parenthetical(at)),
                            )
                        });
                (
                    format!("{percent}%"),
                    format!("{percent}% left"),
                    Some(percent),
                    reset_label,
                    exact_reset,
                )
            }
            None => ("–".to_owned(), "–".to_owned(), None, None, None),
        };
    HostProviderGlanceRow {
        surface_id: surface.id().to_owned(),
        icon_key: surface.id().to_owned(),
        display_label,
        account_label: view.account.account_label.clone(),
        plan_label: view.account.plan_label.clone(),
        glance_remaining_percent,
        bar_label,
        headline,
        reset_label,
        exact_reset,
        status_word: usage_status_storage_label(view.status).to_owned(),
        is_refreshing: view.is_refreshing_placeholder(),
        status_label: usage_display_status_label(view.status).to_owned(),
        severity: worst_severity_label(view),
        updated_label: view.updated_label.clone(),
        last_error: view.last_error.clone(),
        dimmed: matches!(view.status, Status::Stale | Status::Error),
    }
}

fn worst_severity_label(view: &FocusedUsageView) -> String {
    let mut worst = UsageSeverity::Normal;
    for bucket in &view.buckets {
        match bucket.severity {
            UsageSeverity::Danger => worst = UsageSeverity::Danger,
            UsageSeverity::Warn if worst != UsageSeverity::Danger => {
                worst = UsageSeverity::Warn;
            }
            _ => {}
        }
    }
    match worst {
        UsageSeverity::Normal => "normal",
        UsageSeverity::Warn => "warn",
        UsageSeverity::Danger => "danger",
    }
    .to_owned()
}

/// Credential-root inventory for docs and debug (no secrets read).
#[must_use]
pub fn host_credential_root_matrix() -> Vec<HostCredentialRootRow> {
    use jackin_core::container_paths;
    vec![
        HostCredentialRootRow {
            surface: "claude",
            host_paths: "~/.claude/.credentials.json, ~/.claude.json, $CLAUDE_CONFIG_DIR",
            env_vars: "ANTHROPIC_API_KEY, ANTHROPIC_AUTH_TOKEN",
            container_handoff: container_paths::CLAUDE_CREDENTIALS,
        },
        HostCredentialRootRow {
            surface: "codex",
            host_paths: "$CODEX_HOME/auth.json, ~/.codex/auth.json",
            env_vars: "",
            container_handoff: container_paths::CODEX_AUTH,
        },
        HostCredentialRootRow {
            surface: "amp",
            host_paths: "Amp home secrets loaders",
            env_vars: "",
            container_handoff: container_paths::AMP_SECRETS,
        },
        HostCredentialRootRow {
            surface: "grok",
            host_paths: "~/.grok (auth + bin)",
            env_vars: "",
            container_handoff: container_paths::GROK_AUTH,
        },
        HostCredentialRootRow {
            surface: "kimi",
            host_paths: "~/.kimi-code, ~/.kimi",
            env_vars: "KIMI_AUTH_TOKEN, KIMI_CODE_API_KEY, kimi_auth_token",
            container_handoff: container_paths::KIMI_CODE_DIR,
        },
        HostCredentialRootRow {
            surface: "opencode",
            host_paths: "OpenCode home (probe-defined)",
            env_vars: "",
            container_handoff: "",
        },
        HostCredentialRootRow {
            surface: "zai",
            host_paths: "",
            env_vars: "ZAI_API_KEY, Z_AI_API_KEY",
            container_handoff: "",
        },
        HostCredentialRootRow {
            surface: "minimax",
            host_paths: "",
            env_vars: "MINIMAX_CODING_API_KEY, MINIMAX_API_KEY",
            container_handoff: "",
        },
    ]
}

/// One row of the host credential matrix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostCredentialRootRow {
    /// Surface id.
    pub surface: &'static str,
    /// Host path roots.
    pub host_paths: &'static str,
    /// Environment variables.
    pub env_vars: &'static str,
    /// Container handoff fallback.
    pub container_handoff: &'static str,
}

#[cfg(test)]
mod tests;
