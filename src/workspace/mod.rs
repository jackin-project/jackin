//! Workspace configuration types and resolution.
//!
//! Defines `WorkspaceConfig` and `MountConfig` — the on-disk TOML shapes for
//! saved workspaces — and re-exports path helpers, planner types, mount
//! validation, and the `ResolvedWorkspace` the launch pipeline consumes.
//!
//! Not responsible for: reading or writing workspace files (`config/editor.rs`
//! via `ConfigEditor`), or container mount materialization
//! (`isolation/materialize.rs`).

pub mod mounts;
pub mod paths;
pub(crate) mod planner;
pub mod resolve;
pub mod sensitive;
pub mod token_setup;

pub use mounts::{
    parse_mount_spec, parse_mount_spec_resolved, validate_mount_paths, validate_mount_specs,
    validate_mounts,
};
pub use paths::{expand_tilde, resolve_path};
pub use planner::{CollapseError, CollapsePlan, Removal, plan_collapse};
pub use resolve::{
    LoadWorkspaceInput, ResolvedWorkspace, current_dir_workspace, resolve_load_workspace,
    saved_workspace_match_depth,
};
pub use sensitive::{SensitiveMount, confirm_sensitive_mounts, find_sensitive_mounts};

use serde::{Deserialize, Serialize};

// serde skip_serializing_if requires &T; clippy's trivially_copy_pass_by_ref
// lint does not apply here because the signature is mandated by the attribute.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(b: &bool) -> bool {
    !b
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MountConfig {
    pub src: String,
    pub dst: String,
    #[serde(default)]
    pub readonly: bool,
    /// Old configs without this field deserialize to `MountIsolation::Shared`
    /// (the enum default). On save we always write the field — even when it's
    /// the default — so the stored TOML is explicit and old configs migrate to
    /// the new shape on first save instead of silently retaining their
    /// pre-isolation form.
    #[serde(default)]
    pub isolation: crate::isolation::MountIsolation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceConfig {
    #[serde(
        default = "crate::config::migrations::current_workspace_version",
        rename = "version"
    )]
    pub version: String,
    pub workdir: String,
    #[serde(default)]
    pub mounts: Vec<MountConfig>,
    #[serde(default)]
    pub allowed_roles: Vec<String>,
    #[serde(default)]
    pub default_role: Option<String>,
    /// Workspace-level default agent (claude, codex, amp, kimi, or opencode). When unset,
    /// `resolved_agent()` falls back to Claude. The field is omitted
    /// from serialized output when `None` so legacy config files stay
    /// byte-for-byte stable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_agent: Option<crate::agent::Agent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_role: Option<String>,
    /// Workspace-level operator env map. Keys are env var names;
    /// values use the `operator_env` dispatch syntax
    /// (`op://...` | `$NAME` | `${NAME}` | literal).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub env: std::collections::BTreeMap<String, crate::operator_env::EnvValue>,
    /// Per-(workspace × role) env overrides, keyed by the role
    /// selector (e.g. `"agent-smith"` or `"chainargos/agent-brown"`).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub roles: std::collections::BTreeMap<String, WorkspaceRoleOverride>,
    #[serde(default, skip_serializing_if = "KeepAwakeConfig::is_default")]
    pub keep_awake: KeepAwakeConfig,
    /// Workspace-level Claude auth configuration. Forms the middle
    /// layer of the 3-layer auth resolver
    /// (global → workspace → workspace × role × agent).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude: Option<crate::config::AgentAuthConfig>,
    /// Workspace-level Codex auth configuration. See `claude` above —
    /// same role in the resolver, parallel field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex: Option<crate::config::CodexAuthConfig>,
    /// Workspace-level Amp auth configuration. See `claude` above —
    /// same role in the resolver, parallel field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amp: Option<crate::config::AmpAuthConfig>,
    /// Workspace-level Kimi auth configuration. See `claude` above —
    /// same role in the resolver, parallel field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kimi: Option<crate::config::KimiAuthConfig>,
    /// Workspace-level `OpenCode` auth configuration. See `claude` above —
    /// same role in the resolver, parallel field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opencode: Option<crate::config::OpencodeAuthConfig>,
    /// Workspace-level GitHub CLI (`gh`) auth configuration. Middle
    /// layer of the layered resolver (global → workspace → workspace
    /// × role). GitHub auth is agent-neutral — `.config/gh/` is shared
    /// by every agent in the container — so unlike `claude` / `codex`
    /// this layer carries no per-agent split.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github: Option<crate::config::GithubAuthConfig>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub git_pull_on_entry: bool,
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            version: crate::config::CURRENT_WORKSPACE_VERSION.to_string(),
            workdir: String::new(),
            mounts: Vec::new(),
            allowed_roles: Vec::new(),
            default_role: None,
            default_agent: None,
            last_role: None,
            env: std::collections::BTreeMap::new(),
            roles: std::collections::BTreeMap::new(),
            keep_awake: KeepAwakeConfig::default(),
            claude: None,
            codex: None,
            amp: None,
            kimi: None,
            opencode: None,
            github: None,
            git_pull_on_entry: false,
        }
    }
}

impl jackin_console::workspace::WorkspaceRoleAccess for WorkspaceConfig {
    fn allowed_roles(&self) -> &[String] {
        &self.allowed_roles
    }
}

impl jackin_console::tui::components::workdir_pick::WorkdirMount for MountConfig {
    fn dst(&self) -> &str {
        &self.dst
    }
}

/// Per-workspace power-management opt-in.
///
/// macOS-only today: when `enabled = true`, jackin spawns
/// `caffeinate -imsu` while at least one role is running in any
/// workspace with this flag set, so the host stays awake while
/// roles work in the background. The reconciler runs at every
/// jackin command boundary; one caffeinate process covers all
/// active keep-awake workspaces.
///
/// Linux/Windows support (e.g. `systemd-inhibit`) is intentionally
/// out of scope until a non-mac user reports needing it. Adding
/// fields here is a schema change because the struct is
/// `deny_unknown_fields` — extend it intentionally rather than
/// silently accepting new keys.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct KeepAwakeConfig {
    #[serde(default)]
    pub enabled: bool,
}

impl KeepAwakeConfig {
    const fn is_default(&self) -> bool {
        !self.enabled
    }
}

impl WorkspaceConfig {
    /// Returns the workspace's selected agent, defaulting to Claude
    /// when no `default_agent` field is set (legacy workspace).
    pub fn resolved_agent(&self) -> crate::agent::Agent {
        self.default_agent.unwrap_or(crate::agent::Agent::Claude)
    }
}

/// Per-(workspace × role) operator overrides.
///
/// Holds the **most-specific** layer of the 3-layer auth resolver
/// (global → workspace → workspace × role × agent). The `claude` and
/// `codex` fields here override anything set at the workspace or global
/// layer for the matching agent. They are inert until Task 8 wires the
/// resolver to consult them.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceRoleOverride {
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub env: std::collections::BTreeMap<String, crate::operator_env::EnvValue>,
    /// Per-(workspace × role) Claude auth override — most-specific
    /// layer of the 3-layer auth resolver. Inert until Task 8 wires
    /// the resolver to consult this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude: Option<crate::config::AgentAuthConfig>,
    /// Per-(workspace × role) Codex auth override. See `claude` above —
    /// same role in the resolver, parallel field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex: Option<crate::config::CodexAuthConfig>,
    /// Per-(workspace × role) Amp auth override. See `claude` above —
    /// same role in the resolver, parallel field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amp: Option<crate::config::AmpAuthConfig>,
    /// Per-(workspace × role) Kimi auth override. See `claude` above —
    /// same role in the resolver, parallel field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kimi: Option<crate::config::KimiAuthConfig>,
    /// Per-(workspace × role) `OpenCode` auth override. See `claude` above —
    /// same role in the resolver, parallel field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opencode: Option<crate::config::OpencodeAuthConfig>,
    /// Per-(workspace × role) GitHub CLI auth override — most-specific
    /// layer of the layered resolver. The `[github]` axis has no agent
    /// dimension because `.config/gh/` is shared by every agent in the
    /// container.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github: Option<crate::config::GithubAuthConfig>,
}

#[derive(Debug, Clone, Default)]
pub struct WorkspaceEdit {
    pub workdir: Option<String>,
    pub upsert_mounts: Vec<MountConfig>,
    pub remove_destinations: Vec<String>,
    pub no_workdir_mount: bool,
    pub allowed_roles_to_add: Vec<String>,
    pub allowed_roles_to_remove: Vec<String>,
    pub default_role: Option<Option<String>>,
    /// Workspace default-agent change. `None` = no change,
    /// `Some(Some(h))` = set to `h`, `Some(None)` = clear the
    /// explicit field so the workspace falls back to Claude.
    pub default_agent: Option<Option<crate::agent::Agent>>,
    pub mount_isolation_overrides: Vec<(String, crate::isolation::MountIsolation)>,
    /// Toggle for the macOS keep-awake reconciler. `None` = no change,
    /// `Some(true)` = opt in, `Some(false)` = opt out. The CLI's paired
    /// `--keep-awake` / `--no-keep-awake` flags map onto this; the TUI
    /// derives it by diffing `pending.keep_awake` vs `original`.
    pub keep_awake_enabled: Option<bool>,
    pub git_pull_on_entry_enabled: Option<bool>,
}

/// Validate the isolation layout for a workspace's mounts. Two rules:
///
/// 1. **No nested isolated mounts.** Two non-`Shared` mounts where one's
///    `dst` is a strict ancestor of the other's are rejected. The
///    inner worktree's `.git` would land inside the outer worktree's
///    tree, which is unsafe regardless of mode.
///
/// 2. **No same-host-repo isolated siblings.** Two `Worktree` mounts
///    that resolve to the same host repository are rejected. Each
///    isolated worktree creates an admin entry under
///    `<host_repo>/.git/worktrees/<n>/`, and our naming uses the
///    container name as `<n>` so admin entries are globally unique
///    per (`host_repo`, container). Allowing two isolated mounts on the
///    same host repo in one container would force `<container>` to
///    appear twice in that namespace; that case is rare/unmotivated
///    in practice (no operator workflow has surfaced for it). Reject
///    upfront. Revisit if a real use case shows up.
pub fn validate_isolation_layout(mounts: &[MountConfig]) -> anyhow::Result<()> {
    use crate::isolation::MountIsolation;

    let isolated: Vec<(usize, &MountConfig, &str)> = mounts
        .iter()
        .enumerate()
        .filter(|(_, m)| !m.isolation.is_shared())
        .map(|(i, m)| (i, m, m.dst.trim_end_matches('/')))
        .collect();

    for (i, (_, ma, a)) in isolated.iter().enumerate() {
        for (_, mb, b) in &isolated[i + 1..] {
            // Rule 1: nested dst paths.
            if is_strict_ancestor(a, b) || is_strict_ancestor(b, a) {
                anyhow::bail!(
                    "isolated mount `{b}` cannot be nested inside isolated mount `{a}`; \
                     either make the inner mount `shared` or move the inner mount outside \
                     the parent's path",
                    a = if is_strict_ancestor(a, b) { a } else { b },
                    b = if is_strict_ancestor(a, b) { b } else { a },
                );
            }
            // Rule 2: same host repo for worktree mode (same `src` after canonicalization
            // best-effort; falls back to literal string equality if a
            // path can't be canonicalized — e.g., `src` doesn't exist
            // yet on disk).
            if matches!(ma.isolation, MountIsolation::Worktree)
                && matches!(mb.isolation, MountIsolation::Worktree)
                && same_host_repo(&ma.src, &mb.src)
            {
                anyhow::bail!(
                    "isolated mounts `{}` and `{}` cannot share the same host repository `{}`; \
                     remove one of them or change one to `shared` (V1 limitation — see roadmap)",
                    ma.dst,
                    mb.dst,
                    ma.src,
                );
            }
        }
    }
    Ok(())
}

fn same_host_repo(a: &str, b: &str) -> bool {
    let ca = std::fs::canonicalize(a).ok();
    let cb = std::fs::canonicalize(b).ok();
    match (ca, cb) {
        (Some(x), Some(y)) => x == y,
        // If either path can't be canonicalized (e.g. doesn't exist
        // on disk yet during planning), fall back to literal string
        // equality. Stricter checks happen later at materialize time.
        _ => a == b,
    }
}

fn is_strict_ancestor(parent: &str, child: &str) -> bool {
    let parent = parent.trim_end_matches('/');
    let child = child.trim_end_matches('/');
    if parent == child {
        return false;
    }
    let prefix = format!("{parent}/");
    child.starts_with(&prefix)
}

pub fn validate_workspace_config(name: &str, workspace: &WorkspaceConfig) -> anyhow::Result<()> {
    if workspace.workdir.is_empty() {
        anyhow::bail!("workspace {name:?} must define workdir");
    }
    if !workspace.workdir.starts_with('/') {
        anyhow::bail!("workspace {name:?} workdir must be an absolute container path");
    }
    if workspace.mounts.is_empty() {
        anyhow::bail!("workspace {name:?} must define at least one mount");
    }

    validate_mount_specs(&workspace.mounts)?;
    validate_isolation_layout(&workspace.mounts)?;

    let covers_workdir = workspace.mounts.iter().any(|mount| {
        let dst = mount.dst.trim_end_matches('/');
        workspace.workdir == dst
            || workspace.workdir.starts_with(&format!("{dst}/"))
            || dst.starts_with(&format!("{}/", workspace.workdir.trim_end_matches('/')))
    });
    anyhow::ensure!(
        covers_workdir,
        "workspace {name:?} workdir must be equal to, inside, or a parent of one of the workspace mount destinations"
    );

    if let Some(default_role) = &workspace.default_role
        && !workspace.allowed_roles.is_empty()
        && !workspace
            .allowed_roles
            .iter()
            .any(|role| role == default_role)
    {
        anyhow::bail!(
            "workspace {name:?} default_role must be a member of allowed_roles when allowed_roles is set"
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests;
