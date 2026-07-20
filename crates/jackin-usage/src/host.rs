// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Capsule-free host usage orchestration for the macOS menu-bar app and CLI.
//!
//! Reuses [`crate::usage::UsageCache`] probes, cache, cooldown, and
//! `FocusedUsageView` shaping. State roots live under the operator jackin
//! data dir (not container `/jackin/...` paths).

use std::collections::{BTreeMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use jackin_core::Agent;
use jackin_protocol::control::FocusedUsageView;
use jackin_protocol::Provider;

use crate::usage::{UsageCache, UsageRefreshTarget};

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
}

impl HostRuntimeConfig {
    /// Default host layout under `data_dir`.
    #[must_use]
    pub fn under_data_dir(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
            refresh_floor_secs: 300,
            enabled_surface_ids: Vec::new(),
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
    open: bool,
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
            open: false,
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
        let surface =
            HostSurfaceId::from_id(surface_id).ok_or_else(|| format!("unknown surface: {surface_id}"))?;
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
        let surface =
            HostSurfaceId::from_id(surface_id).ok_or_else(|| format!("unknown surface: {surface_id}"))?;
        self.cache.insert_snapshot_for_test(
            surface.agent_slug(),
            surface.provider_label(),
            view,
        );
        self.push_event("snapshot_updated", Some(surface.id()), Some("injected".to_owned()));
        Ok(())
    }

    /// Refresh enabled surfaces (blocking probes when due).
    ///
    /// When `force` is false, a call within [`Self::refresh_floor_secs`] of the
    /// last network refresh is a no-op (poll-safe). When `force` is true (manual
    /// Refresh / Settings), the floor is bypassed and targets are marked due.
    pub fn refresh(&mut self, surface_id: Option<&str>, force: bool) -> Result<(), String> {
        self.require_open()?;
        if !force
            && let Some(last) = self.last_refresh
        {
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
        self.cache.refresh_active_account_snapshots(
            &targets,
            None,
            &self.provider_keys,
            now,
        );
        self.last_refresh = Some(now);
        for target in &targets {
            let surface = surface_for_target(target);
            let view = self.cache.focused_snapshot(
                Some(&target.agent),
                target.provider.as_deref(),
            );
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
            self.push_event(kind, surface.map(HostSurfaceId::id), view.last_error.clone());
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
        match self.last_refresh {
            None => true,
            Some(last) => last.elapsed() >= Duration::from_secs(self.refresh_floor_secs),
        }
    }

    /// Cached snapshot for one surface (honest refreshing/unavailable).
    pub fn snapshot(&mut self, surface_id: &str) -> Result<FocusedUsageView, String> {
        self.require_open()?;
        let surface =
            HostSurfaceId::from_id(surface_id).ok_or_else(|| format!("unknown surface: {surface_id}"))?;
        if !self.enabled.contains(surface.id()) {
            return Err(format!("surface disabled: {surface_id}"));
        }
        Ok(self
            .cache
            .focused_snapshot(Some(surface.agent_slug()), surface.provider_label()))
    }

    /// Compact bar label for one enabled surface, if known.
    pub fn status_bar_label(&mut self, surface_id: &str) -> Result<Option<String>, String> {
        self.require_open()?;
        let surface =
            HostSurfaceId::from_id(surface_id).ok_or_else(|| format!("unknown surface: {surface_id}"))?;
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

    fn refresh_targets(
        &self,
        surface_id: Option<&str>,
    ) -> Result<Vec<UsageRefreshTarget>, String> {
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

    fn push_event(
        &mut self,
        kind: &str,
        surface_id: Option<&str>,
        detail: Option<String>,
    ) {
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

/// Credential-root inventory for docs and debug (no secrets read).
#[must_use]
pub fn host_credential_root_matrix() -> Vec<HostCredentialRootRow> {
    vec![
        HostCredentialRootRow {
            surface: "claude",
            host_paths: "~/.claude/.credentials.json, ~/.claude.json, $CLAUDE_CONFIG_DIR",
            env_vars: "ANTHROPIC_API_KEY, ANTHROPIC_AUTH_TOKEN",
            container_handoff: "/jackin/claude/credentials.json",
        },
        HostCredentialRootRow {
            surface: "codex",
            host_paths: "$CODEX_HOME/auth.json, ~/.codex/auth.json",
            env_vars: "",
            container_handoff: "/jackin/codex/auth.json",
        },
        HostCredentialRootRow {
            surface: "amp",
            host_paths: "Amp home secrets loaders",
            env_vars: "",
            container_handoff: "/jackin/amp/secrets.json",
        },
        HostCredentialRootRow {
            surface: "grok",
            host_paths: "~/.grok (auth + bin)",
            env_vars: "",
            container_handoff: "/jackin/grok/auth.json",
        },
        HostCredentialRootRow {
            surface: "kimi",
            host_paths: "~/.kimi-code, ~/.kimi",
            env_vars: "KIMI_AUTH_TOKEN, KIMI_CODE_API_KEY, kimi_auth_token",
            container_handoff: "/jackin/kimi-code",
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
#[path = "host/tests.rs"]
mod tests;
