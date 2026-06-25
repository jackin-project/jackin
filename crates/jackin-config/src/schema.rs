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

use jackin_core::{Agent, EnvValue, MountIsolation};
use serde::{Deserialize, Serialize};

use jackin_core::AuthForwardMode;

use crate::auth::{AgentAuthConfig, GithubAuthConfig};
use crate::versions::current_workspace_version;

// ─── Serde helper ────────────────────────────────────────────────────────────

/// `skip_serializing_if` requires `fn(&T) -> bool`.
#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_false(v: &bool) -> bool {
    !*v
}

// ─── Mount types ─────────────────────────────────────────────────────────────

/// A single workspace mount: a host path bound into the container.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MountConfig {
    pub src: String,
    pub dst: String,
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
    #[serde(default)]
    pub enabled: bool,
}

impl KeepAwakeConfig {
    pub const fn is_default(&self) -> bool {
        !self.enabled
    }
}

// ─── Per-(workspace × role) override ─────────────────────────────────────────

/// Per-(workspace × role) operator overrides — the most-specific auth layer.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceRoleOverride {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, EnvValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude: Option<AgentAuthConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex: Option<AgentAuthConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amp: Option<AgentAuthConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kimi: Option<AgentAuthConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opencode: Option<AgentAuthConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grok: Option<AgentAuthConfig>,
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceConfig {
    #[serde(default = "current_workspace_version", rename = "version")]
    pub version: String,
    pub workdir: String,
    #[serde(default)]
    pub mounts: Vec<MountConfig>,
    #[serde(default)]
    pub allowed_roles: Vec<String>,
    #[serde(default)]
    pub default_role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_agent: Option<Agent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_role: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, EnvValue>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub roles: BTreeMap<String, WorkspaceRoleOverride>,
    #[serde(default, skip_serializing_if = "KeepAwakeConfig::is_default")]
    pub keep_awake: KeepAwakeConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude: Option<AgentAuthConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex: Option<AgentAuthConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amp: Option<AgentAuthConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kimi: Option<AgentAuthConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opencode: Option<AgentAuthConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grok: Option<AgentAuthConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github: Option<GithubAuthConfig>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub git_pull_on_entry: bool,
    #[serde(default, skip_serializing_if = "WorkspaceRuntimeConfig::is_default")]
    pub runtime: WorkspaceRuntimeConfig,
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
                anyhow::bail!(
                    "auth_forward 'oauth_token' is not supported for {}",
                    agent.slug()
                );
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
                    anyhow::bail!(
                        "auth_forward 'oauth_token' is not supported for {} in role {}",
                        agent.slug(),
                        role
                    );
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
    pub git: String,
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
    pub src: String,
    pub dst: String,
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
    Mount(GlobalMountConfig),
    Scoped(BTreeMap<String, GlobalMountConfig>),
}

/// The `[docker.mounts]` section in `config.toml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DockerMounts(BTreeMap<String, MountEntry>);

impl DockerMounts {
    pub fn get(&self, key: &str) -> Option<&MountEntry> {
        self.0.get(key)
    }

    pub fn insert(&mut self, key: String, value: MountEntry) -> Option<MountEntry> {
        self.0.insert(key, value)
    }

    pub fn entry(
        &mut self,
        key: String,
    ) -> std::collections::btree_map::Entry<'_, String, MountEntry> {
        self.0.entry(key)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &MountEntry)> {
        self.0.iter()
    }
}

/// Top-level `[docker]` block in `config.toml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DockerConfig {
    #[serde(default)]
    pub mounts: DockerMounts,
}

// ─── Resolved workspace ──────────────────────────────────────────────────────

/// A workspace with its global mounts already resolved and merged.
///
/// This is the runtime view of a workspace: mounts expanded from named
/// global mount sets, workdir validated, and keep-awake flag determined.
/// The binary's `workspace::resolve` builds it from an `AppConfig`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedWorkspace {
    pub label: String,
    pub workdir: String,
    pub mounts: Vec<MountConfig>,
    /// Whether the keep-awake reconciler is active for this workspace.
    pub keep_awake_enabled: bool,
    /// Workspace-level default agent (`None` for ad-hoc / current-dir workspaces).
    pub default_agent: Option<Agent>,
    pub git_pull_on_entry: bool,
}

// AppConfig stays in the binary crate — it has many inherent impl blocks
// (load_or_init, edit_workspace, sync_builtin_agents, etc.) that depend on
// binary-only types (ConfigEditor, fs2, JackinPaths). Moving AppConfig would
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
            anyhow::bail!("mount source must be absolute: {}", mount.src);
        }
        if !mount.dst.starts_with('/') {
            anyhow::bail!("mount destination must be an absolute path: {}", mount.dst);
        }
        if !seen_dst.insert(mount.dst.clone()) {
            anyhow::bail!("duplicate mount destination: {}", mount.dst);
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
            anyhow::bail!("mount source does not exist: {}", mount.src);
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
    pub workdir: Option<String>,
    pub upsert_mounts: Vec<MountConfig>,
    pub remove_destinations: Vec<String>,
    pub no_workdir_mount: bool,
    pub allowed_roles_to_add: Vec<String>,
    pub allowed_roles_to_remove: Vec<String>,
    pub default_role: Option<Option<String>>,
    /// `None` = no change, `Some(Some(h))` = set to `h`,
    /// `Some(None)` = clear (fall back to Claude).
    pub default_agent: Option<Option<Agent>>,
    pub mount_isolation_overrides: Vec<(String, MountIsolation)>,
    pub keep_awake_enabled: Option<bool>,
    pub git_pull_on_entry_enabled: Option<bool>,
}

// ─── Git config ───────────────────────────────────────────────────────────────

/// Global `[git]` block: co-author trailer and DCO settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GitConfig {
    #[serde(default, skip_serializing_if = "is_false")]
    pub coauthor_trailer: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub dco: bool,
}

impl GitConfig {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
}

// AppConfig stays in the binary crate for now — it has impl blocks that
// depend on JackinPaths and fs2 (binary-crate types). Migration to
// jackin-config happens in Phase 2 after JackinPaths is extractable.
// This note documents the deliberate deferral so the next agent doesn't
// redo the analysis.
