// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Capsule-free host usage projection for the macOS menu-bar app and CLI.
//!
//! Provider work and shared state are owned by the host usage broker. This
//! runtime holds presentation state only.

mod accounts;
mod broker;
mod credential_resolver;
mod discovery;
mod projection;

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use jackin_core::{Agent, account_key_hash};
use jackin_protocol::control::{FocusedUsageView, UsageIdentityPresentation, UsageSeverity};
use jackin_protocol::usage_broker::{UsageAccountCapability, UsageProjectionV1, UsageRefreshPhase};

use crate::usage::{
    UsageCache, UsageFormatPrefs, compact_duration_label, estimate_caption,
    exact_reset_parenthetical, percent_headline, provider_display_label, reset_label_with_prefs,
    usage_display_status_label, usage_identity_presentation, usage_status_storage_label,
};

pub use accounts::{
    AccountLifecycle, AccountProvenance, CanonicalAccountIdentity, CanonicalAccountSubject,
    HostAccountDescriptor, account_key_for_view, canonical_account_id_for_view, min_remaining,
    short_account_identity,
};
pub use broker::{
    ForwardedUsageSources, UsageBrokerClient, UsageBrokerConfig, UsageBrokerHandle,
    ensure_usage_broker, ensure_usage_broker_process, ensure_usage_broker_with_executor,
    forwarded_usage_capabilities, run_usage_broker_service, run_usage_broker_service_with_executor,
    usage_broker_capabilities,
};
pub use credential_resolver::{
    CachedProviderCredentialResolver, ProviderCredentialSecretOutcome,
    ProviderCredentialSecretResolution, ProviderCredentialSecretSource,
};
pub use discovery::{
    DiscoveredAccountDescriptor, ForwardedUsageAccount, HostCredentialRootRow,
    OpaqueCredentialHandle, ProviderCredentialEnvOutcome, ProviderCredentialEnvResolution,
    ProviderCredentialEnvResolver, ProviderCredentialIdentityOutcome,
    ProviderCredentialRefreshOutcome, UsageCredentialKind, UsageDiscoveryCatalog,
    UsageDiscoveryDiagnostic, UsageDiscoveryIssue, UsageDiscoveryScope,
    UsageSourceCandidateDescriptor, ValidatedUsageDiscovery, discover_usage_sources,
    host_credential_root_matrix, validate_usage_sources,
};
pub use projection::{NormalizedUsageDestination, UsageDestination, normalize_destination};

/// Relative data-dir subtree for menu-bar durable state.
pub const HOST_USAGE_STATE_REL: &str = "usage-menu-bar";

static CANONICAL_INSTANCE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn canonical_instance_id() -> String {
    let epoch_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let sequence = CANONICAL_INSTANCE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    account_key_hash(
        "usage-broker-instance-v1",
        &format!("{epoch_nanos}:{sequence}"),
    )
}

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
        Self::Codex,
        Self::Claude,
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

    /// Canonical provider identity, separate from legacy agent routing ids.
    #[must_use]
    pub const fn provider_id(self) -> &'static str {
        match self {
            Self::Claude => "anthropic",
            Self::Codex => "openai",
            Self::Amp => "amp",
            Self::Grok => "xai",
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
            Self::Claude => "Anthropic",
            Self::Codex => "OpenAI",
            Self::Amp => "Amp",
            Self::Grok => "xAI",
            Self::Zai => "Z.AI",
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

    /// Canonical provider label used by durable account-key hashing.
    #[must_use]
    pub const fn account_provider_label(self) -> &'static str {
        self.label()
    }

    /// Rust-owned fallback glyph used only when the native icon cannot load.
    #[must_use]
    pub const fn fallback_glyph(self) -> &'static str {
        self.compact_prefix()
    }

    /// Provider-owned usage/settings destination for Desktop actions.
    #[must_use]
    pub const fn usage_url(self) -> Option<&'static str> {
        match self {
            Self::Codex => Some("https://chatgpt.com/codex/settings/usage"),
            Self::Claude => Some("https://claude.ai/settings/usage"),
            Self::Amp => Some("https://ampcode.com/settings"),
            Self::Grok => Some("https://console.x.ai/team/default/usage"),
            Self::Zai => Some("https://z.ai/manage-apikey/coding-plan/personal/usage"),
            Self::Kimi => Some("https://www.kimi.com/membership/subscription?tab=quota"),
            Self::Minimax => Some("https://platform.minimax.io/console/usage"),
            Self::OpenCode => None,
        }
    }

    /// Agent slug used by shared presentation helpers.
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
            Self::Claude => Some("Anthropic"),
            Self::Codex => Some("OpenAI"),
            Self::Amp => Some("Amp"),
            Self::Grok => Some("xAI"),
            Self::Zai => Some("Z.AI"),
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

    /// Parse an enumerated provider alias into exact ownership.
    ///
    /// This deliberately does not inspect [`Self::agent_slug`]: Z.AI and
    /// `MiniMax` route through the Codex probe but never own `OpenAI` accounts.
    #[must_use]
    pub fn from_provider_alias(alias: &str) -> Option<Self> {
        let normalized = alias
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .collect::<String>()
            .to_ascii_lowercase();
        match normalized.as_str() {
            "claude" | "anthropic" | "anthropicclaude" => Some(Self::Claude),
            "codex" | "openai" | "openaicodex" => Some(Self::Codex),
            "amp" => Some(Self::Amp),
            "grok" | "grokbuild" | "xai" | "xaigrok" => Some(Self::Grok),
            "zai" | "glm" | "glmzai" => Some(Self::Zai),
            "kimi" => Some(Self::Kimi),
            "minimax" => Some(Self::Minimax),
            "opencode" => Some(Self::OpenCode),
            _ => None,
        }
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
}

/// Descriptor returned to `boltffi` / CLI (no secrets).
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
    /// Account-discovery authority for this runtime.
    pub discovery_scope: UsageDiscoveryScope,
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
            discovery_scope: UsageDiscoveryScope::Capsule {
                forwarded_accounts: Vec::new(),
            },
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

/// One enabled-surface overview row for jackin❯ desktop (popover + Usage window).
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
    /// Rust-owned fallback glyph.
    pub fallback_glyph: String,
    /// Provider usage/settings URL.
    pub usage_url: Option<String>,
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
    /// Compact countdown token used by the menu-bar chip (`<1m`, `2h 14m`).
    pub compact_reset_label: Option<String>,
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
    /// The single Rust-owned activity phrase for this selected provider/account.
    pub activity_label: String,
    /// Machine activity kind (`idle` | `updating` | `exceptional`).
    pub activity_kind: String,
    /// Complete menu-bar/popover accessibility and tooltip copy.
    pub accessibility_label: String,
    /// Rust-owned last error, when present.
    pub last_error: Option<String>,
    /// Whether the native bar value is visually dimmed (stale/error).
    pub dimmed: bool,
}

/// Provider state when detection succeeds without a stable account identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostDesktopProviderState {
    pub status_word: String,
    pub status_label: String,
    pub updated_label: String,
    pub last_error: Option<String>,
    pub is_refreshing: bool,
}

/// One Rust-ordered Desktop provider group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostDesktopProviderGroup {
    pub surface_id: String,
    pub display_label: String,
    pub icon_key: String,
    pub fallback_glyph: String,
    pub usage_url: Option<String>,
    pub account_column_label: String,
    pub plan_or_status_label: String,
    pub remaining_label: String,
    pub reset_display_label: String,
    pub accessibility_label: String,
    pub accounts: Vec<HostAccountDescriptor>,
    pub empty_state: Option<HostDesktopProviderState>,
}

/// Atomic account inventory consumed by jackin❯ desktop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostDesktopInventory {
    pub groups: Vec<HostDesktopProviderGroup>,
}

/// One provider group plus its selected, fully-presented usage snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostDesktopProviderProjection {
    pub group: HostDesktopProviderGroup,
    pub selected_account_key: Option<String>,
    pub selected_usage: FocusedUsageView,
    pub identity: UsageIdentityPresentation,
    pub is_updating: bool,
}

/// One immutable Desktop state boundary, produced while the runtime is locked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostDesktopProjection {
    pub generation: u64,
    pub refresh_in_progress: bool,
    pub error_message: Option<String>,
    pub next_refresh_label: String,
    pub surfaces: Vec<HostSurfaceDescriptor>,
    pub providers: Vec<HostDesktopProviderProjection>,
    pub glance_rows: Vec<HostProviderGlanceRow>,
    pub status_bar_glance_rows: Vec<HostProviderGlanceRow>,
    pub diagnostics: Vec<UsageDiscoveryDiagnostic>,
}

/// Driving bucket for compact/overview labels: min remaining + its reset epoch.
#[derive(Debug, Clone, Copy)]
struct DrivingBucket {
    remaining: u8,
    resets_at: Option<i64>,
}

/// Hard cap for burn-first status-bar chips (SB-3 / SB-14). Never more than three.
pub const STATUS_BAR_MAX_CHIPS: usize = 3;

/// SB-17 rank keys for ascending sort: **soonest reset first**, then **higher
/// remaining %** (invert remaining so ascending puts larger headroom first).
/// Missing `resets_at` sorts last on the time key.
#[must_use]
pub(crate) fn status_bar_rank_key(remaining: u8, resets_at: Option<i64>) -> (i64, u8) {
    let time_key = resets_at.unwrap_or(i64::MAX);
    let remaining_key = u8::MAX.saturating_sub(remaining);
    (time_key, remaining_key)
}

/// Capsule-free host usage runtime.
#[derive(Debug)]
pub struct HostUsageRuntime {
    cache: UsageCache,
    enabled: HashSet<String>,
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
    /// Last completed current-membership discovery generation.
    discovery: Option<ValidatedUsageDiscovery>,
    /// Last quota snapshots fetched from explicit current discovery sources.
    discovered_views: BTreeMap<(HostSurfaceId, String), FocusedUsageView>,
    /// Explicit source state without authenticated account identity yet.
    discovered_provider_views: BTreeMap<HostSurfaceId, FocusedUsageView>,
    /// Scope retained for explicit manual reconciliation only.
    discovery_scope: Option<UsageDiscoveryScope>,
    /// Broker generation phase per canonical account.
    broker_phases: BTreeMap<UsageAccountCapability, UsageRefreshPhase>,
    canonical_instance_id: String,
    canonical_content_id: Option<String>,
    canonical_projection_cache: Option<UsageProjectionV1>,
    canonical_identity_graph: accounts::CanonicalIdentityGraph,
}

impl HostUsageRuntime {
    /// Construct a closed runtime (call [`Self::open`] before use).
    #[must_use]
    pub fn new() -> Self {
        Self {
            cache: UsageCache::default(),
            enabled: HashSet::new(),
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
            discovery: None,
            discovered_views: BTreeMap::new(),
            discovered_provider_views: BTreeMap::new(),
            discovery_scope: None,
            broker_phases: BTreeMap::new(),
            canonical_instance_id: canonical_instance_id(),
            canonical_content_id: None,
            canonical_projection_cache: None,
            canonical_identity_graph: accounts::CanonicalIdentityGraph::default(),
        }
    }

    /// Open with host paths; enables all surfaces when config list empty.
    pub fn open(&mut self, config: HostRuntimeConfig) -> Result<(), String> {
        self.open_prepared(config, None)
    }

    /// Open after Rust-owned config/env discovery completes.
    pub fn open_with_discovery(
        &mut self,
        config: HostRuntimeConfig,
        resolver: &dyn ProviderCredentialEnvResolver,
    ) -> Result<(), String> {
        let discovered = validate_usage_sources(
            discover_usage_sources(&config.discovery_scope, resolver)?,
            resolver,
        );
        self.open_prepared(config, Some(discovered))
    }

    fn open_prepared(
        &mut self,
        config: HostRuntimeConfig,
        discovery: Option<ValidatedUsageDiscovery>,
    ) -> Result<(), String> {
        let enabled = if config.enabled_surface_ids.is_empty() {
            HostSurfaceId::ALL
                .iter()
                .map(|surface| surface.id().to_owned())
                .collect::<HashSet<_>>()
        } else {
            let unknown = config
                .enabled_surface_ids
                .iter()
                .filter(|id| HostSurfaceId::from_id(id).is_none())
                .cloned()
                .collect::<Vec<_>>();
            if !unknown.is_empty() {
                return Err(format!(
                    "unknown enabled surface ids: {}",
                    unknown.join(", ")
                ));
            }
            config.enabled_surface_ids.iter().cloned().collect()
        };
        let data_dir_changed = self
            .data_dir
            .as_ref()
            .is_some_and(|current| current != &config.data_dir);
        if data_dir_changed {
            self.cache = UsageCache::default();
            self.events.clear();
            self.next_seq = 0;
            self.selected_accounts.clear();
            self.desktop_detected_surfaces.clear();
            self.discovery = None;
            self.discovered_views.clear();
            self.discovered_provider_views.clear();
            self.broker_phases.clear();
        }
        let accounts_path = host_accounts_path(&config.data_dir);
        self.cache.set_accounts_materialize_path(accounts_path);
        self.refresh_floor_secs = config.refresh_floor_secs.max(60);
        self.last_refresh = None;
        self.enabled = enabled;
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
        self.discovery_scope = Some(config.discovery_scope);
        self.discovery = discovery;
        self.discovered_views.clear();
        self.discovered_provider_views.clear();
        self.desktop_detected_surfaces.clear();
        self.broker_phases.clear();
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

    /// Clone the current validated catalog for host broker attachment.
    #[must_use]
    pub fn validated_discovery(&self) -> Option<ValidatedUsageDiscovery> {
        self.discovery.clone()
    }

    /// Whether one provider surface is enabled for refresh.
    #[must_use]
    pub fn surface_enabled(&self, surface_id: &str) -> bool {
        self.enabled.contains(surface_id)
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

    /// Sanitized discovery failures for the current completed catalog generation.
    pub fn discovery_diagnostics(&self) -> Result<Vec<UsageDiscoveryDiagnostic>, String> {
        self.require_open()?;
        Ok(self
            .discovery
            .as_ref()
            .map(|discovery| discovery.diagnostics.clone())
            .unwrap_or_default())
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
        if let Some(discovery) = &mut self.discovery {
            let injected = self
                .cache
                .focused_snapshot(Some(surface.agent_slug()), surface.provider_label());
            if let Some(identity) = CanonicalAccountIdentity::from_view(surface, &injected) {
                let account_key = identity.account_key();
                if !discovery
                    .accounts
                    .iter()
                    .any(|account| account.account_key == account_key)
                {
                    discovery.accounts.push(DiscoveredAccountDescriptor {
                        surface_id: surface.id().to_owned(),
                        account_key,
                        account_label: injected.account.account_label.clone(),
                        provenance: Vec::new(),
                        source_ids: vec!["fixture".to_owned()],
                        identity,
                    });
                }
            }
        }
        self.push_event(
            "snapshot_updated",
            Some(surface.id()),
            Some("injected".to_owned()),
        );
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
        if self.broker_refresh_in_progress() {
            return true;
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
        let catalog = self.materialize_account_catalog()?;
        self.reconcile_selected_accounts(&catalog, std::slice::from_ref(&surface))?;
        let selected = self.selected_accounts.get(surface.id());
        Ok(selected
            .and_then(|key| catalog.entry(surface, key))
            .map(|entry| entry.view.clone())
            .or_else(|| catalog.provider_state(surface).cloned())
            .unwrap_or(live))
    }

    /// List known accounts for one surface (or all surfaces when `None`).
    ///
    /// Sources: current broker discovery and durable broker history.
    pub fn list_accounts(
        &mut self,
        surface_id: Option<&str>,
    ) -> Result<Vec<HostAccountDescriptor>, String> {
        self.require_open()?;
        let surfaces: Vec<HostSurfaceId> = match surface_id {
            Some(id) => {
                let surface =
                    HostSurfaceId::from_id(id).ok_or_else(|| format!("unknown surface: {id}"))?;
                vec![surface]
            }
            None => HostSurfaceId::DESKTOP_PROVIDER_ORDER.to_vec(),
        };
        let catalog = self.materialize_account_catalog()?;
        self.reconcile_selected_accounts(&catalog, &surfaces)?;
        let now = chrono::Utc::now().timestamp();
        let prefs = self.format_prefs;
        let mut out = Vec::new();
        for surface in surfaces {
            let selected = self.selected_accounts.get(surface.id()).map(String::as_str);
            for entry in catalog.entries_for_surface(surface) {
                out.push(account_descriptor(
                    surface,
                    entry,
                    selected == Some(entry.account_key.as_str()),
                    now,
                    prefs,
                ));
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
            let catalog = self.materialize_account_catalog()?;
            if catalog.entry(surface, account_key).is_none() {
                return Err(format!(
                    "account key does not belong to surface {surface_id}"
                ));
            }
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
        Ok(Some(self.snapshot(surface_id)?.status_bar_label))
    }

    /// Merged compact bar text from enabled surfaces that have labels.
    pub fn merged_status_bar_label(&mut self) -> Result<String, String> {
        self.require_open()?;
        let mut parts = Vec::new();
        for surface in HostSurfaceId::ALL {
            if !self.enabled.contains(surface.id()) {
                continue;
            }
            let label = self.snapshot(surface.id())?.status_bar_label;
            // Skip pure loading noise when other surfaces already contribute.
            if label == "refreshing" && !parts.is_empty() {
                continue;
            }
            parts.push(format!("{}: {label}", surface.label()));
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

    /// Burn-first multi-surface strip (SB-3/14/17/19), capped ≤3, joined with ` · `.
    ///
    /// Eligible surfaces only (numeric remaining **> 0**). Order: **soonest
    /// reset first**, then **higher remaining %** (SB-17). Never more than
    /// [`STATUS_BAR_MAX_CHIPS`] tokens.
    pub fn compact_status_bar_strip(&mut self, max: u32) -> Result<String, String> {
        self.require_open()?;
        let cap = (max as usize).clamp(1, STATUS_BAR_MAX_CHIPS);
        let prefs = self.format_prefs;
        let mut rows: Vec<(u8, HostSurfaceId, Option<i64>)> = Vec::new();
        for surface in HostSurfaceId::ALL.iter().copied() {
            if !self.enabled.contains(surface.id()) {
                continue;
            }
            if let Some(drive) = self.driving_bucket_for(surface) {
                // SB-19: depleted never appears on the burn-first bar.
                if drive.remaining == 0 {
                    continue;
                }
                rows.push((drive.remaining, surface, drive.resets_at));
            }
        }
        rows.sort_by_key(|(remaining, surface, resets_at)| {
            let (time_key, rem_key) = status_bar_rank_key(*remaining, *resets_at);
            (
                time_key,
                rem_key,
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

    /// Desktop **status-bar** provider chips only (SB-3/14/17/19).
    ///
    /// Unlike [`Self::provider_glance_rows`] (full inventory for popover/Usage),
    /// this drops 0% rows, ranks soonest-then-remaining, and hard-caps at
    /// [`STATUS_BAR_MAX_CHIPS`]. `max` is clamped into `1…STATUS_BAR_MAX_CHIPS`.
    #[must_use = "status-bar rows are the multi-item NSStatusItem source"]
    pub fn status_bar_provider_glance_rows(
        &mut self,
        max: u32,
    ) -> Result<Vec<HostProviderGlanceRow>, String> {
        self.require_open()?;
        let catalog = self.materialize_account_catalog()?;
        self.reconcile_selected_accounts(&catalog, HostSurfaceId::DESKTOP_PROVIDER_ORDER)?;
        let cap = (max as usize).clamp(1, STATUS_BAR_MAX_CHIPS);
        let prefs = self.format_prefs;
        let now = chrono::Utc::now().timestamp();
        let mut candidates: Vec<(u8, Option<i64>, HostProviderGlanceRow)> = Vec::new();
        for surface in HostSurfaceId::DESKTOP_PROVIDER_ORDER.iter().copied() {
            if !self.enabled.contains(surface.id()) {
                self.desktop_detected_surfaces.remove(surface.id());
                continue;
            }
            let selected = self
                .selected_accounts
                .get(surface.id())
                .and_then(|key| catalog.entry(surface, key));
            let Some(view) = selected
                .map(|entry| &entry.view)
                .or_else(|| catalog.provider_state(surface))
            else {
                self.desktop_detected_surfaces.remove(surface.id());
                continue;
            };
            let detected = if view_is_auto_detected(view) {
                self.desktop_detected_surfaces
                    .insert(surface.id().to_owned());
                true
            } else if view.is_refreshing_placeholder() {
                self.desktop_detected_surfaces.contains(surface.id())
            } else {
                self.desktop_detected_surfaces.remove(surface.id());
                false
            };
            if !detected {
                continue;
            }
            let glance = glance_bucket(surface, view);
            let remaining = glance.and_then(|b| b.remaining_percent);
            // SB-19: no numeric remaining or 0% → out of bar membership.
            let Some(rem) = remaining else {
                continue;
            };
            if rem == 0 {
                continue;
            }
            let resets_at = glance.and_then(|b| b.resets_at);
            let row = build_provider_glance_row(
                surface,
                view,
                self.surface_refresh_in_progress(surface.id()),
                now,
                prefs,
            );
            candidates.push((rem, resets_at, row));
        }
        candidates.sort_by_key(|(remaining, resets_at, row)| {
            let (time_key, rem_key) = status_bar_rank_key(*remaining, *resets_at);
            (
                time_key,
                rem_key,
                HostSurfaceId::DESKTOP_PROVIDER_ORDER
                    .iter()
                    .position(|s| s.id() == row.surface_id)
                    .unwrap_or(usize::MAX),
            )
        });
        Ok(candidates
            .into_iter()
            .take(cap)
            .map(|(_, _, row)| row)
            .collect())
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
            let view = self.snapshot(surface.id())?;
            let status_word = usage_status_storage_label(view.status).to_owned();
            let severity = worst_severity_label(&view);
            let display_label = provider_display_label(surface.label()).to_owned();

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

    /// One atomic, Rust-owned grouped account projection for jackin❯ desktop.
    pub fn desktop_inventory(&mut self) -> Result<HostDesktopInventory, String> {
        self.require_open()?;
        let catalog = self.materialize_account_catalog()?;
        self.reconcile_selected_accounts(&catalog, HostSurfaceId::DESKTOP_PROVIDER_ORDER)?;
        let now = chrono::Utc::now().timestamp();
        let prefs = self.format_prefs;
        let mut groups = Vec::new();
        for surface in HostSurfaceId::DESKTOP_PROVIDER_ORDER.iter().copied() {
            if !self.enabled.contains(surface.id()) {
                self.desktop_detected_surfaces.remove(surface.id());
                continue;
            }
            let entries = catalog.entries_for_surface(surface);
            let has_current = entries
                .iter()
                .any(|entry| entry.lifecycle == AccountLifecycle::Current);
            let provider_state = catalog.provider_state(surface);
            let detected = if has_current || provider_state.is_some_and(view_is_auto_detected) {
                self.desktop_detected_surfaces
                    .insert(surface.id().to_owned());
                true
            } else if provider_state.is_some_and(FocusedUsageView::is_refreshing_placeholder) {
                self.desktop_detected_surfaces.contains(surface.id())
            } else {
                self.desktop_detected_surfaces.remove(surface.id());
                false
            };
            if !detected {
                continue;
            }
            let selected = self.selected_accounts.get(surface.id()).map(String::as_str);
            let accounts = entries
                .into_iter()
                .map(|entry| {
                    account_descriptor(
                        surface,
                        entry,
                        selected == Some(entry.account_key.as_str()),
                        now,
                        prefs,
                    )
                })
                .collect::<Vec<_>>();
            let empty_state = accounts.is_empty().then(|| {
                let view = provider_state.cloned().unwrap_or_else(|| {
                    self.cache
                        .focused_snapshot(Some(surface.agent_slug()), surface.provider_label())
                });
                let is_refreshing = view.is_refreshing_placeholder();
                HostDesktopProviderState {
                    status_word: usage_status_storage_label(view.status).to_owned(),
                    status_label: usage_display_status_label(view.status).to_owned(),
                    updated_label: view.updated_label,
                    last_error: view.last_error,
                    is_refreshing,
                }
            });
            let display_label = provider_display_label(surface.label()).to_owned();
            let plan_or_status_label = empty_state
                .as_ref()
                .filter(|state| state.status_word != "fresh")
                .map_or_else(|| "—".to_owned(), |state| state.status_label.clone());
            let accessibility_label = empty_state.as_ref().map_or_else(
                || display_label.clone(),
                |state| format!("{display_label}, {}", state.status_label),
            );
            groups.push(HostDesktopProviderGroup {
                surface_id: surface.id().to_owned(),
                display_label,
                icon_key: surface.id().to_owned(),
                fallback_glyph: surface.fallback_glyph().to_owned(),
                usage_url: surface.usage_url().map(str::to_owned),
                account_column_label: "—".to_owned(),
                plan_or_status_label,
                remaining_label: "—".to_owned(),
                reset_display_label: "—".to_owned(),
                accessibility_label,
                accounts,
                empty_state,
            });
        }
        Ok(HostDesktopInventory { groups })
    }

    /// Build the complete native Desktop model from one uninterrupted runtime
    /// snapshot. The `boltffi` bridge holds the runtime mutex for this whole call,
    /// so no broker generation can interleave partial provider/account state.
    pub fn desktop_projection(
        &mut self,
        status_bar_max: u32,
    ) -> Result<HostDesktopProjection, String> {
        self.require_open()?;
        let surfaces = self.list_surfaces()?;
        let inventory = self.desktop_inventory()?;
        let mut providers = Vec::with_capacity(inventory.groups.len());
        for group in inventory.groups {
            let surface_id = group.surface_id.clone();
            let selected_account_key = group
                .accounts
                .iter()
                .find(|account| account.selected)
                .map(|account| account.account_key.clone());
            let selected_usage = self.snapshot(&surface_id)?;
            let is_updating = self.surface_refresh_in_progress(&surface_id);
            let identity =
                usage_identity_presentation(&group.display_label, &selected_usage, is_updating);
            providers.push(HostDesktopProviderProjection {
                group,
                selected_account_key,
                selected_usage,
                identity,
                is_updating,
            });
        }
        let glance_rows = self.provider_glance_rows()?;
        let status_bar_glance_rows = self.status_bar_provider_glance_rows(status_bar_max)?;
        let diagnostics = self.discovery_diagnostics()?;
        let global_messages = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.surface_id.is_none())
            .map(|diagnostic| {
                format!(
                    "{}: {}",
                    diagnostic.scope_label,
                    diagnostic.issue.display_message()
                )
            })
            .collect::<Vec<_>>();
        Ok(HostDesktopProjection {
            generation: self.next_seq,
            refresh_in_progress: self.broker_refresh_in_progress(),
            error_message: (!global_messages.is_empty()).then(|| global_messages.join("\n")),
            next_refresh_label: self.next_refresh_label(),
            surfaces,
            providers,
            glance_rows,
            status_bar_glance_rows,
            diagnostics,
        })
    }

    /// Detected providers in the canonical Desktop model order, each a
    /// selected-account-aware glance row. Iterates only
    /// [`HostSurfaceId::DESKTOP_PROVIDER_ORDER`], materializes account sources
    /// once, resolves exact selected-account ownership, and re-evaluates
    /// detection on every call. Affirmative evidence inserts membership, a
    /// non-refreshing view without evidence removes it, and the cold refreshing
    /// placeholder alone reuses prior membership so refresh cannot drop a row.
    /// Returns an empty vector for zero detected providers.
    #[must_use = "the glance rows are the Desktop surface source"]
    pub fn provider_glance_rows(&mut self) -> Result<Vec<HostProviderGlanceRow>, String> {
        self.require_open()?;
        let catalog = self.materialize_account_catalog()?;
        self.reconcile_selected_accounts(&catalog, HostSurfaceId::DESKTOP_PROVIDER_ORDER)?;
        let prefs = self.format_prefs;
        let now = chrono::Utc::now().timestamp();
        let mut rows = Vec::new();
        for surface in HostSurfaceId::DESKTOP_PROVIDER_ORDER.iter().copied() {
            if !self.enabled.contains(surface.id()) {
                self.desktop_detected_surfaces.remove(surface.id());
                continue;
            }
            let selected = self
                .selected_accounts
                .get(surface.id())
                .and_then(|key| catalog.entry(surface, key));
            let Some(view) = selected
                .map(|entry| &entry.view)
                .or_else(|| catalog.provider_state(surface))
            else {
                self.desktop_detected_surfaces.remove(surface.id());
                continue;
            };
            let detected = if view_is_auto_detected(view) {
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
                rows.push(build_provider_glance_row(
                    surface,
                    view,
                    self.surface_refresh_in_progress(surface.id()),
                    now,
                    prefs,
                ));
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
        let view = self.snapshot(surface.id()).ok()?;
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
        self.discovery = None;
        self.discovery_scope = None;
        self.discovered_views.clear();
        self.discovered_provider_views.clear();
        self.broker_phases.clear();
        self.canonical_content_id = None;
        self.canonical_projection_cache = None;
    }

    fn materialize_account_catalog(&mut self) -> Result<accounts::AccountCatalog, String> {
        let mut live_views = Vec::with_capacity(HostSurfaceId::ALL.len());
        for surface in HostSurfaceId::ALL.iter().copied() {
            let view = self
                .cache
                .focused_snapshot(Some(surface.agent_slug()), surface.provider_label());
            live_views.push((surface, view, true));
        }
        let store_path = self
            .data_dir
            .as_ref()
            .map(|dir| host_snapshot_store_path(dir))
            .unwrap_or_default();
        accounts::materialize_account_catalog(
            &live_views,
            &self.discovered_views,
            &self.discovered_provider_views,
            &store_path,
            self.discovery
                .as_ref()
                .map(|discovery| discovery.accounts.as_slice()),
        )
    }

    fn reconcile_selected_accounts(
        &mut self,
        catalog: &accounts::AccountCatalog,
        surfaces: &[HostSurfaceId],
    ) -> Result<(), String> {
        let before = self.selected_accounts.clone();
        self.selected_accounts.retain(|surface_id, account_key| {
            HostSurfaceId::from_id(surface_id)
                .is_some_and(|surface| catalog.entry(surface, account_key).is_some())
        });
        for surface in surfaces {
            if !self.selected_accounts.contains_key(surface.id())
                && let Some(key) = catalog.preferred_current_key(*surface)
            {
                self.selected_accounts.insert(surface.id().to_owned(), key);
            }
        }
        if self.selected_accounts != before
            && let Some(data_dir) = &self.data_dir
        {
            accounts::save_selected_accounts(
                &accounts::selected_accounts_path(data_dir),
                &self.selected_accounts,
            )?;
        }
        Ok(())
    }

    fn require_open(&self) -> Result<(), String> {
        if self.open {
            Ok(())
        } else {
            Err("runtime not open".to_owned())
        }
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

fn discovered_account_keys(
    discovery: Option<&ValidatedUsageDiscovery>,
) -> HashSet<(HostSurfaceId, String)> {
    discovery
        .into_iter()
        .flat_map(|discovery| discovery.accounts.iter())
        .map(|account| (account.identity.surface, account.account_key.clone()))
        .collect()
}

impl Default for HostUsageRuntime {
    fn default() -> Self {
        Self::new()
    }
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
    is_updating: bool,
    now: i64,
    prefs: UsageFormatPrefs,
) -> HostProviderGlanceRow {
    use jackin_protocol::control::UsageSnapshotStatus as Status;
    let display_label = provider_display_label(surface.label()).to_owned();
    let glance = glance_bucket(surface, view);
    let (
        bar_label,
        headline,
        glance_remaining_percent,
        reset_label,
        compact_reset_label,
        exact_reset,
    ) = match glance.and_then(|bucket| bucket.remaining_percent) {
        Some(percent) => {
            let (reset_label, compact_reset_label, exact_reset) = glance
                .and_then(|bucket| bucket.resets_at)
                .map_or((None, None, None), |at| {
                    (
                        Some(reset_label_with_prefs(at, now, prefs)),
                        Some(if at <= now {
                            "now".to_owned()
                        } else {
                            compact_duration_label(at.saturating_sub(now))
                        }),
                        Some(exact_reset_parenthetical(at)),
                    )
                });
            (
                format!("{percent}%"),
                format!("{percent}% left"),
                Some(percent),
                reset_label,
                compact_reset_label,
                exact_reset,
            )
        }
        None => ("–".to_owned(), "–".to_owned(), None, None, None, None),
    };
    let identity = usage_identity_presentation(&display_label, view, is_updating);
    HostProviderGlanceRow {
        surface_id: surface.id().to_owned(),
        icon_key: surface.id().to_owned(),
        fallback_glyph: surface.fallback_glyph().to_owned(),
        usage_url: surface.usage_url().map(str::to_owned),
        display_label,
        account_label: identity.account_label.clone(),
        plan_label: view.account.plan_label.clone(),
        glance_remaining_percent,
        bar_label,
        headline,
        reset_label,
        compact_reset_label,
        exact_reset,
        status_word: usage_status_storage_label(view.status).to_owned(),
        is_refreshing: is_updating || view.is_refreshing_placeholder(),
        status_label: usage_display_status_label(view.status).to_owned(),
        severity: worst_severity_label(view),
        updated_label: view.updated_label.clone(),
        activity_label: identity.activity_label,
        activity_kind: match identity.activity_kind {
            jackin_protocol::control::UsageActivityKind::Idle => "idle",
            jackin_protocol::control::UsageActivityKind::Updating => "updating",
            jackin_protocol::control::UsageActivityKind::Exceptional => "exceptional",
        }
        .to_owned(),
        accessibility_label: identity.accessibility_label,
        last_error: view.last_error.clone(),
        dimmed: matches!(view.status, Status::Stale | Status::Error),
    }
}

fn account_descriptor(
    surface: HostSurfaceId,
    entry: &accounts::AccountCatalogEntry,
    selected: bool,
    now: i64,
    prefs: UsageFormatPrefs,
) -> HostAccountDescriptor {
    use jackin_protocol::control::UsageSnapshotStatus as Status;

    let view = &entry.view;
    let bucket = glance_bucket(surface, view).or_else(|| {
        view.buckets
            .iter()
            .filter(|bucket| bucket.remaining_percent.is_some())
            .min_by_key(|bucket| bucket.remaining_percent)
    });
    let remaining_percent = bucket.and_then(|bucket| bucket.remaining_percent);
    let (remaining_label, headline) = remaining_percent.map_or_else(
        || ("—".to_owned(), "—".to_owned()),
        |percent| (format!("{percent}%"), percent_headline(percent, prefs)),
    );
    let (reset_label, exact_reset) =
        bucket
            .and_then(|bucket| bucket.resets_at)
            .map_or((None, None), |reset| {
                (
                    Some(reset_label_with_prefs(reset, now, prefs)),
                    Some(exact_reset_parenthetical(reset)),
                )
            });
    let mut provenance = entry
        .provenance
        .iter()
        .map(|source| source.display_label().to_owned())
        .collect::<Vec<_>>();
    provenance.extend(entry.discovery_provenance.iter().cloned());
    provenance.sort();
    provenance.dedup();
    let provenance_label = provenance.join(" · ");
    let status_label = usage_display_status_label(view.status).to_owned();
    let plan_or_status_label = entry.plan_label.clone().unwrap_or_else(|| {
        if matches!(view.status, Status::Fresh) {
            "—".to_owned()
        } else {
            status_label.clone()
        }
    });
    let reset_display_label = reset_label.clone().unwrap_or_else(|| "—".to_owned());
    let accessibility_label = format!(
        "{}, {}, {}, {}, {}",
        provider_display_label(surface.label()),
        entry.account_label,
        plan_or_status_label,
        remaining_label,
        reset_display_label
    );
    HostAccountDescriptor {
        surface_id: surface.id().to_owned(),
        provider_column_label: "—".to_owned(),
        account_key: entry.account_key.clone(),
        account_label: entry.account_label.clone(),
        plan_label: entry.plan_label.clone(),
        selected,
        lifecycle: entry.lifecycle.label().to_owned(),
        lifecycle_label: match entry.lifecycle {
            AccountLifecycle::Current => "Current account",
            AccountLifecycle::Historical => "Historical account",
            AccountLifecycle::ProviderPresenceOnly => "Provider presence only",
        }
        .to_owned(),
        provenance,
        provenance_label,
        plan_or_status_label,
        remaining_percent,
        remaining_label,
        headline,
        reset_label,
        reset_display_label,
        exact_reset,
        status_word: usage_status_storage_label(view.status).to_owned(),
        status_label,
        severity: worst_severity_label(view),
        updated_label: view.updated_label.clone(),
        last_error: view.last_error.clone(),
        dimmed: matches!(view.status, Status::Stale | Status::Error),
        accessibility_label,
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

#[cfg(test)]
mod tests;
