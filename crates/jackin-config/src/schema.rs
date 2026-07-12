//! Core configuration schema: `AppConfig`, `WorkspaceConfig`, and all supporting
//! data types.
//!
//! This module breaks the `config ↔ workspace` mutual cycle by placing both
//! `AppConfig` and `WorkspaceConfig` in the same crate — their mutual references
//! become intra-crate. The behavior that operates on these types (TOML
//! read/write, migrations, workspace resolution, mount planning) lives in the
//! binary crate and imports from here.
//!
//! **Dependency tier:** `jackin-core` → `jackin-config` (this module)

use std::collections::BTreeMap;

use jackin_core::docker_security::{DockerGrants, DockerSecurityProfile};
use jackin_core::{Agent, EnvValue, MountIsolation};
use serde::{Deserialize, Serialize};

use jackin_core::AuthForwardMode;

use crate::ConfigError;
use crate::auth::{AgentAuthConfig, GithubAuthConfig};
use crate::versions::current_workspace_version;

// ─── Serde helper ────────────────────────────────────────────────────────────

/// `skip_serializing_if` requires `fn(&T) -> bool`.
#[allow(clippy::trivially_copy_pass_by_ref, reason = "documented residual allow; prefer expect when site is lint-true")]
const fn is_false(v: &bool) -> bool {
    !*v
}

// ─── DirtyExitPolicy ─────────────────────────────────────────────────────────

/// What jackin❯ does when a foreground session ends with dirty or unpushed
/// isolated work (D8). Clean+pushed sessions always auto-clean regardless of
/// this setting.
///
/// Resolution order: per-workspace override → global `AppConfig` default →
/// built-in `Ask`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DirtyExitPolicy {
    /// Prompt the operator with the exit dialog (default). (D8)
    #[default]
    Ask,
    /// Auto-preserve the instance for resume — no prompt. (D8)
    Keep,
    /// Auto-discard everything including uncommitted edits and unpushed
    /// commits — no prompt. Explicit operator opt-in for disposable
    /// workspaces; jackin❯ does not second-guess it (D17).
    Discard,
}

impl DirtyExitPolicy {
    /// Snake-case wire / TOML form of this policy.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::Keep => "keep",
            Self::Discard => "discard",
        }
    }
}

// ─── Mount types ─────────────────────────────────────────────────────────────

/// A single workspace mount: a host path bound into the container.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MountConfig {
    /// Host path bound into the container.
    pub src: String,
    /// Absolute container destination path.
    pub dst: String,
    /// When true, the bind is read-only.
    #[serde(default)]
    pub readonly: bool,
    /// Old configs without this field deserialize to `MountIsolation::Shared`
    /// (the enum default). On save we always write the field so the stored TOML
    /// is explicit and old configs migrate to the new shape on first save.
    #[serde(default)]
    pub isolation: MountIsolation,
}

// ─── Keep-awake ──────────────────────────────────────────────────────────────

/// Per-workspace power-management opt-in (macOS: `caffeinate`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct KeepAwakeConfig {
    /// When true, keep the host awake while a session uses this workspace.
    #[serde(default)]
    pub enabled: bool,
}

impl KeepAwakeConfig {
    /// `true` when equal to the serde default (skip empty tables on serialize).
    pub const fn is_default(&self) -> bool {
        !self.enabled
    }
}

// ─── Per-(workspace × role) override ─────────────────────────────────────────

/// Per-(workspace × role) operator overrides — the most-specific auth layer.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceRoleOverride {
    /// Role-layer operator env (most specific env merge layer).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, EnvValue>,
    /// Claude auth override for this workspace×role.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude: Option<AgentAuthConfig>,
    /// Codex auth override for this workspace×role.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex: Option<AgentAuthConfig>,
    /// Amp auth override for this workspace×role.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amp: Option<AgentAuthConfig>,
    /// Kimi auth override for this workspace×role.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kimi: Option<AgentAuthConfig>,
    /// `OpenCode` auth override for this workspace×role.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opencode: Option<AgentAuthConfig>,
    /// Grok auth override for this workspace×role.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grok: Option<AgentAuthConfig>,
    /// GitHub auth override for this workspace×role.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github: Option<GithubAuthConfig>,
}

impl WorkspaceRoleOverride {
    /// Auth-forward mode for `agent` at the workspace×role override layer.
    ///
    /// Keep this match parallel with `WorkspaceConfig` and `AppConfig`: these
    /// are versioned TOML structs with named agent fields, so the dispatch
    /// stays as one accessor per layer until a schema-bumped map migration.
    pub fn auth_forward_for(&self, agent: Agent) -> Option<AuthForwardMode> {
        match agent {
            Agent::Claude => self.claude.as_ref().map(|c| c.auth_forward),
            Agent::Codex => self.codex.as_ref().map(|c| c.auth_forward),
            Agent::Amp => self.amp.as_ref().map(|c| c.auth_forward),
            Agent::Kimi => self.kimi.as_ref().map(|c| c.auth_forward),
            Agent::Opencode => self.opencode.as_ref().map(|c| c.auth_forward),
            Agent::Grok => self.grok.as_ref().map(|c| c.auth_forward),
        }
    }

    /// Sync source dir override for `agent` at the workspace×role layer.
    ///
    /// Same named-field exception as `auth_forward_for`: centralizing the match
    /// here prevents call-site fan-out without changing the persisted schema.
    pub fn sync_source_dir_for(&self, agent: Agent) -> Option<std::path::PathBuf> {
        match agent {
            Agent::Claude => self.claude.as_ref().and_then(|c| c.sync_source_dir.clone()),
            Agent::Codex => self.codex.as_ref().and_then(|c| c.sync_source_dir.clone()),
            Agent::Amp => self.amp.as_ref().and_then(|c| c.sync_source_dir.clone()),
            Agent::Kimi => self.kimi.as_ref().and_then(|c| c.sync_source_dir.clone()),
            Agent::Opencode => self
                .opencode
                .as_ref()
                .and_then(|c| c.sync_source_dir.clone()),
            Agent::Grok => self.grok.as_ref().and_then(|c| c.sync_source_dir.clone()),
        }
    }
}

// ─── Runtime backend selection ───────────────────────────────────────────────

/// Host-wide container backend selection (`config.toml` `[runtime]`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RuntimeConfig {
    /// Default backend for new launches: `docker` (the default when unset) or
    /// `apple-container`. A per-workspace `[runtime].backend` overrides it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_backend: Option<String>,
}

impl RuntimeConfig {
    /// True when nothing diverges from the serde default, so the whole
    /// `[runtime]` table is skipped on serialize and existing files stay clean.
    #[must_use]
    pub const fn is_default(&self) -> bool {
        self.default_backend.is_none()
    }
}

// ─── Telemetry selection ─────────────────────────────────────────────────────

/// Host-wide telemetry verbosity (`config.toml` `[telemetry].level`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TelemetryLevelConfig {
    /// Default operator-facing verbosity.
    Info,
    /// Include debug diagnostics.
    Debug,
    /// Maximum verbosity (trace-level categories).
    Trace,
}

impl TelemetryLevelConfig {
    /// Snake-case wire form of this level.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        }
    }
}

/// Host-wide telemetry filtering (`config.toml` `[telemetry]`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct TelemetryConfig {
    /// Optional floor for structured log verbosity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<TelemetryLevelConfig>,
    /// Category allow-list for fine-grained telemetry filters.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<String>,
}

impl TelemetryConfig {
    /// `true` when nothing diverges from the serde default.
    #[must_use]
    pub fn is_default(&self) -> bool {
        self.level.is_none() && self.categories.is_empty()
    }
}

/// Per-workspace container backend override (`workspaces/<name>.toml`
/// `[runtime]`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct WorkspaceRuntimeConfig {
    /// Backend override for this workspace: `docker` or `apple-container`.
    /// `None` inherits the host-wide `[runtime].default_backend`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
}

impl WorkspaceRuntimeConfig {
    /// True when nothing diverges from the serde default — see
    /// [`RuntimeConfig::is_default`].
    #[must_use]
    pub const fn is_default(&self) -> bool {
        self.backend.is_none()
    }
}

// ─── WorkspaceConfig ─────────────────────────────────────────────────────────

/// A saved workspace: the workdir, mounts, and per-agent auth config.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkspaceConfig {
    /// On-disk schema version for this workspace file.
    #[serde(default = "current_workspace_version", rename = "version")]
    pub version: String,
    /// Absolute container working directory for sessions in this workspace.
    pub workdir: String,
    /// Workspace-local bind mounts.
    #[serde(default)]
    pub mounts: Vec<MountConfig>,
    /// When non-empty, only these role keys may launch into the workspace.
    #[serde(default)]
    pub allowed_roles: Vec<String>,
    /// Preferred role when none is specified on launch.
    #[serde(default)]
    pub default_role: Option<String>,
    /// Preferred agent when none is specified on launch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_agent: Option<Agent>,
    /// Last role used in this workspace (sticky default for CLI/console).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_role: Option<String>,
    /// Workspace-layer operator env map.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, EnvValue>,
    /// Per-role overrides nested under this workspace.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub roles: BTreeMap<String, WorkspaceRoleOverride>,
    /// Keep-awake (power management) opt-in for this workspace.
    #[serde(default, skip_serializing_if = "KeepAwakeConfig::is_default")]
    pub keep_awake: KeepAwakeConfig,
    /// Workspace-layer Claude auth policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude: Option<AgentAuthConfig>,
    /// Workspace-layer Codex auth policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex: Option<AgentAuthConfig>,
    /// Workspace-layer Amp auth policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amp: Option<AgentAuthConfig>,
    /// Workspace-layer Kimi auth policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kimi: Option<AgentAuthConfig>,
    /// Workspace-layer `OpenCode` auth policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opencode: Option<AgentAuthConfig>,
    /// Workspace-layer Grok auth policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grok: Option<AgentAuthConfig>,
    /// Workspace-layer GitHub auth policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github: Option<GithubAuthConfig>,
    /// When true, pull git remotes on workspace entry.
    #[serde(default, skip_serializing_if = "is_false")]
    pub git_pull_on_entry: bool,
    /// Per-workspace container backend override.
    #[serde(default, skip_serializing_if = "WorkspaceRuntimeConfig::is_default")]
    pub runtime: WorkspaceRuntimeConfig,
    /// Per-workspace override for the dirty-exit decision policy (D8).
    /// `None` means inherit from the global `AppConfig::dirty_exit_policy`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dirty_exit_policy: Option<DirtyExitPolicy>,
    /// Optional Docker security profile/grants for this workspace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docker: Option<WorkspaceDockerConfig>,
}

/// Docker security settings scoped to one saved workspace.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceDockerConfig {
    /// Security profile name (inherits host default when `None`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<DockerSecurityProfile>,
    /// Explicit capability grants for this workspace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grants: Option<DockerGrants>,
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            version: current_workspace_version(),
            workdir: String::new(),
            mounts: Vec::new(),
            allowed_roles: Vec::new(),
            default_role: None,
            default_agent: None,
            last_role: None,
            env: BTreeMap::new(),
            roles: BTreeMap::new(),
            keep_awake: KeepAwakeConfig::default(),
            claude: None,
            codex: None,
            amp: None,
            kimi: None,
            opencode: None,
            grok: None,
            github: None,
            git_pull_on_entry: false,
            runtime: WorkspaceRuntimeConfig::default(),
            dirty_exit_policy: None,
            docker: None,
        }
    }
}

impl WorkspaceConfig {
    /// Returns the workspace's selected agent, defaulting to Claude.
    pub fn resolved_agent(&self) -> Agent {
        self.default_agent.unwrap_or(Agent::Claude)
    }

    /// Auth-forward mode for `agent` at the workspace layer.
    ///
    /// Keep this match parallel with `WorkspaceRoleOverride` and `AppConfig`:
    /// the persisted schema exposes named agent fields, so this accessor is the
    /// single allowed dispatch point for this layer until a schema-bumped map.
    pub fn auth_forward_for(&self, agent: Agent) -> Option<AuthForwardMode> {
        match agent {
            Agent::Claude => self.claude.as_ref().map(|c| c.auth_forward),
            Agent::Codex => self.codex.as_ref().map(|c| c.auth_forward),
            Agent::Amp => self.amp.as_ref().map(|c| c.auth_forward),
            Agent::Kimi => self.kimi.as_ref().map(|c| c.auth_forward),
            Agent::Opencode => self.opencode.as_ref().map(|c| c.auth_forward),
            Agent::Grok => self.grok.as_ref().map(|c| c.auth_forward),
        }
    }

    /// Sync source dir override for `agent` at the workspace layer.
    ///
    /// Same named-field exception as `auth_forward_for`; callers must use this
    /// accessor rather than matching over `Agent` themselves.
    pub fn sync_source_dir_for(&self, agent: Agent) -> Option<std::path::PathBuf> {
        match agent {
            Agent::Claude => self.claude.as_ref().and_then(|c| c.sync_source_dir.clone()),
            Agent::Codex => self.codex.as_ref().and_then(|c| c.sync_source_dir.clone()),
            Agent::Amp => self.amp.as_ref().and_then(|c| c.sync_source_dir.clone()),
            Agent::Kimi => self.kimi.as_ref().and_then(|c| c.sync_source_dir.clone()),
            Agent::Opencode => self
                .opencode
                .as_ref()
                .and_then(|c| c.sync_source_dir.clone()),
            Agent::Grok => self.grok.as_ref().and_then(|c| c.sync_source_dir.clone()),
        }
    }

    /// Validates that no configured agent uses an auth mode unsupported by that agent.
    ///
    /// Mirrors `AppConfig::validate_auth_modes` for the workspace layer.
    /// Checks both the workspace-level config and every per-role override.
    pub fn validate_auth_modes(&self) -> anyhow::Result<()> {
        let workspace_pairs: &[(Agent, Option<&AgentAuthConfig>)] = &[
            (Agent::Codex, self.codex.as_ref()),
            (Agent::Amp, self.amp.as_ref()),
            (Agent::Kimi, self.kimi.as_ref()),
            (Agent::Opencode, self.opencode.as_ref()),
        ];
        for (agent, cfg) in workspace_pairs {
            if cfg.is_some_and(|c| {
                c.auth_forward == AuthForwardMode::OAuthToken
                    && !agent
                        .supported_modes()
                        .contains(&AuthForwardMode::OAuthToken)
            }) {
                return Err(ConfigError::msg(format!(
                    "auth_forward 'oauth_token' is not supported for {}",
                    agent.slug()
                ))
                .into());
            }
        }
        for (role, override_cfg) in &self.roles {
            let role_pairs: &[(Agent, Option<&AgentAuthConfig>)] = &[
                (Agent::Codex, override_cfg.codex.as_ref()),
                (Agent::Amp, override_cfg.amp.as_ref()),
                (Agent::Kimi, override_cfg.kimi.as_ref()),
                (Agent::Opencode, override_cfg.opencode.as_ref()),
            ];
            for (agent, cfg) in role_pairs {
                if cfg.is_some_and(|c| {
                    c.auth_forward == AuthForwardMode::OAuthToken
                        && !agent
                            .supported_modes()
                            .contains(&AuthForwardMode::OAuthToken)
                }) {
                    return Err(ConfigError::msg(format!(
                        "auth_forward 'oauth_token' is not supported for {} in role {}",
                        agent.slug(),
                        role
                    ))
                    .into());
                }
            }
        }
        Ok(())
    }
}

// ─── Role source ─────────────────────────────────────────────────────────────

/// A role source entry in the global config.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleSource {
    /// Git URL for the role repository.
    pub git: String,
    /// Whether the operator has marked this role source as trusted.
    #[serde(default, skip_serializing_if = "is_false")]
    pub trusted: bool,
    /// Role-layer operator env map. Merged on top of the global `[env]` map.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, EnvValue>,
}

// ─── Global mount config ──────────────────────────────────────────────────────

/// Global mount entry in `[[mounts]]` / `[mounts.<scope>]`.
///
/// Unlike `MountConfig` (which carries `MountIsolation`), global mounts in
/// the operator config have no isolation field — isolation is workspace-only.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GlobalMountConfig {
    /// Host path for the global mount.
    pub src: String,
    /// Absolute container destination.
    pub dst: String,
    /// When true, the bind is read-only.
    #[serde(default)]
    pub readonly: bool,
}

impl From<GlobalMountConfig> for MountConfig {
    fn from(g: GlobalMountConfig) -> Self {
        Self {
            src: g.src,
            dst: g.dst,
            readonly: g.readonly,
            isolation: MountIsolation::Shared,
        }
    }
}

// ─── Docker mount entries ─────────────────────────────────────────────────────

/// Serde-untagged mount entry: either a single `GlobalMountConfig` or a
/// scope-keyed map.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MountEntry {
    /// Single unscoped global mount keyed by name.
    Mount(GlobalMountConfig),
    /// Scope-keyed map of named mounts (`namespace/*` or role key).
    Scoped(BTreeMap<String, GlobalMountConfig>),
}

/// The `[docker.mounts]` section in `config.toml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DockerMounts(BTreeMap<String, MountEntry>);

impl DockerMounts {
    /// Look up a mount entry by outer key (name or scope).
    pub fn get(&self, key: &str) -> Option<&MountEntry> {
        self.0.get(key)
    }

    /// Insert or replace a mount entry; returns the previous value if any.
    pub fn insert(&mut self, key: String, value: MountEntry) -> Option<MountEntry> {
        self.0.insert(key, value)
    }

    /// Entry API for in-place scope map updates.
    pub fn entry(
        &mut self,
        key: String,
    ) -> std::collections::btree_map::Entry<'_, String, MountEntry> {
        self.0.entry(key)
    }

    /// Iterate outer keys and mount entries.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &MountEntry)> {
        self.0.iter()
    }
}

/// Top-level `[docker]` block in `config.toml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DockerConfig {
    /// Named and scoped global mounts.
    #[serde(default)]
    pub mounts: DockerMounts,
    /// Default Docker security profile for launches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<DockerSecurityProfile>,
    /// Default capability grants for launches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grants: Option<DockerGrants>,
}

// ─── Resolved workspace ──────────────────────────────────────────────────────

/// A workspace with its global mounts already resolved and merged.
///
/// This is the runtime view of a workspace: mounts expanded from named
/// global mount sets, workdir validated, and keep-awake flag determined.
/// The binary's `workspace::resolve` builds it from an `AppConfig`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedWorkspace {
    /// Workspace name (saved) or workdir path (ad-hoc).
    pub name: String,
    /// Operator-facing label (often same as `name`).
    pub label: String,
    /// Absolute container working directory.
    pub workdir: String,
    /// Effective mounts after global merge.
    pub mounts: Vec<MountConfig>,
    /// Whether the keep-awake reconciler is active for this workspace.
    pub keep_awake_enabled: bool,
    /// Workspace-level default agent (`None` for ad-hoc / current-dir workspaces).
    pub default_agent: Option<Agent>,
    /// Whether git pull-on-entry is enabled for this resolved workspace.
    pub git_pull_on_entry: bool,
}

// AppConfig stays in the binary crate — it has many inherent impl blocks
// (load_or_init, edit_workspace, sync_builtin_agents, etc.) that depend on
// binary-only types (ConfigEditor, fs4, JackinPaths). Moving AppConfig would
// require all those impls to also move, creating a very large extraction.
// This note documents the deliberate deferral.

// ─── Mount validation ─────────────────────────────────────────────────────────

/// Structural validation: absolute paths, no duplicate destinations.
///
/// # Errors
/// Returns an error if any mount has a relative path or duplicate destination.
pub fn validate_mount_specs(mounts: &[MountConfig]) -> anyhow::Result<()> {
    use std::collections::HashSet;
    use std::path::Path;
    let mut seen_dst = HashSet::new();
    for mount in mounts {
        if !Path::new(&mount.src).is_absolute() {
            return Err(ConfigError::MountSrcNotAbsolute(mount.src.clone()).into());
        }
        if !mount.dst.starts_with('/') {
            return Err(ConfigError::MountDstNotAbsolute(mount.dst.clone()).into());
        }
        if !seen_dst.insert(mount.dst.clone()) {
            return Err(ConfigError::DuplicateMountDst(mount.dst.clone()).into());
        }
    }
    Ok(())
}

/// Filesystem validation: checks that mount sources exist on disk.
///
/// # Errors
/// Returns an error if any mount source path does not exist.
pub fn validate_mount_paths(mounts: &[MountConfig]) -> anyhow::Result<()> {
    use std::path::Path;
    for mount in mounts {
        if !Path::new(&mount.src).exists() {
            return Err(ConfigError::MountSrcMissing(mount.src.clone()).into());
        }
    }
    Ok(())
}

/// Full validation: structural + filesystem checks combined.
///
/// # Errors
/// Returns an error if structural or filesystem validation fails.
pub fn validate_mounts(mounts: &[MountConfig]) -> anyhow::Result<()> {
    validate_mount_specs(mounts)?;
    validate_mount_paths(mounts)
}

// ─── Workspace edit ──────────────────────────────────────────────────────────

/// Workspace mutation spec: built by the TUI/CLI from the pending-vs-original
/// diff, applied by `AppConfig::edit_workspace`.
#[derive(Debug, Default, Clone)]
pub struct WorkspaceEdit {
    /// New workdir when `Some`.
    pub workdir: Option<String>,
    /// Mounts to insert or replace by destination.
    pub upsert_mounts: Vec<MountConfig>,
    /// Destination paths of mounts to remove.
    pub remove_destinations: Vec<String>,
    /// When true, drop the auto-mounted workdir (`src = dst = workdir`).
    pub no_workdir_mount: bool,
    /// Role keys to append to `allowed_roles`.
    pub allowed_roles_to_add: Vec<String>,
    /// Role keys to remove from `allowed_roles`.
    pub allowed_roles_to_remove: Vec<String>,
    /// `None` = no change; `Some(None)` clears; `Some(Some(r))` sets default role.
    pub default_role: Option<Option<String>>,
    /// `None` = no change, `Some(Some(h))` = set to `h`,
    /// `Some(None)` = clear (fall back to Claude).
    pub default_agent: Option<Option<Agent>>,
    /// Per-destination isolation mode overrides applied after mount edits.
    pub mount_isolation_overrides: Vec<(String, MountIsolation)>,
    /// Keep-awake toggle when `Some`.
    pub keep_awake_enabled: Option<bool>,
    /// Git pull-on-entry toggle when `Some`.
    pub git_pull_on_entry_enabled: Option<bool>,
}

// ─── Git config ───────────────────────────────────────────────────────────────

/// Global `[git]` block: co-author trailer and DCO settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GitConfig {
    /// Append a co-author trailer on agent commits when true.
    #[serde(default, skip_serializing_if = "is_false")]
    pub coauthor_trailer: bool,
    /// Require a DCO sign-off trailer when true.
    #[serde(default, skip_serializing_if = "is_false")]
    pub dco: bool,
}

impl GitConfig {
    /// `true` when both flags are off (serde default).
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
}

// AppConfig stays in the binary crate for now — it has impl blocks that
// depend on JackinPaths and fs4 (binary-crate types). Migration to
// jackin-config happens in Phase 2 after JackinPaths is extractable.
// This note documents the deliberate deferral so the next agent doesn't
// redo the analysis.
