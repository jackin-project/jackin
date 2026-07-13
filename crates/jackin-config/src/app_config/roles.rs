// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Auth-forward mode resolution: walk global → workspace → workspace×role layers.
//!
//! Not responsible for auth token acquisition, credential forwarding
//! mechanics, or GitHub CLI interaction — only the three-layer config
//! precedence lookup that yields `AuthForwardMode`.

use crate::ConfigError;
use std::collections::BTreeMap;

use jackin_core::{Agent, AuthForwardMode, EnvValue, RoleSelector, WorkspaceName};

use super::AppConfig;
use crate::auth::GithubAuthMode;
use crate::schema::RoleSource;

/// Map key for workspace-scoped lookups. `None` = global-only (empty map key).
fn workspace_key(workspace: Option<&WorkspaceName>) -> &str {
    workspace.map_or("", WorkspaceName::as_str)
}

/// Resolve the effective auth-forward mode for an agent in a (workspace, role) scope.
///
/// Walks three layers, most-specific wins:
///
/// 1. `workspaces[ws].roles[role].<agent>.auth_forward`
/// 2. `workspaces[ws].<agent>.auth_forward`
/// 3. `<agent>.auth_forward` (global)
///
/// Returns [`AuthForwardMode::Sync`] if no layer is set. The `<agent>`
/// selector picks the matching per-agent field at each layer.
///
/// Passing `workspace = None` (or a name not present in the config)
/// naturally falls through to the global layer; this is the supported
/// way for non-workspace-scoped callers (e.g. `jackin config auth show`)
/// to read the global default through the same code path.
pub fn resolve_mode(
    cfg: &AppConfig,
    agent: Agent,
    workspace: Option<&WorkspaceName>,
    role: &str,
) -> AuthForwardMode {
    resolve_mode_with_trace(cfg, agent, workspace, role).0
}

/// Like [`resolve_mode`] but also returns the resolution trace.
///
/// The trace is a vector of `(layer_label, value_at_layer)` pairs, lowest
/// precedence last. Used by `runtime::launch` to build error messages that
/// show the operator exactly which config layer resolved the credential mode.
pub fn resolve_mode_with_trace(
    cfg: &AppConfig,
    agent: Agent,
    workspace: Option<&WorkspaceName>,
    role: &str,
) -> (AuthForwardMode, Vec<(String, Option<AuthForwardMode>)>) {
    let ws = workspace_key(workspace);
    let agent_at_global = cfg.auth_forward_for(agent);
    let agent_at_workspace = cfg
        .workspaces
        .get(ws)
        .and_then(|w| w.auth_forward_for(agent));
    let agent_at_ws_role = cfg
        .workspaces
        .get(ws)
        .and_then(|w| w.roles.get(role))
        .and_then(|ro| ro.auth_forward_for(agent));
    let winning = agent_at_ws_role
        .or(agent_at_workspace)
        .or(agent_at_global)
        .unwrap_or_default();
    let trace = vec![
        (format!("workspace × role × {agent}"), agent_at_ws_role),
        (format!("workspace × {agent}"), agent_at_workspace),
        (format!("global × {agent}"), agent_at_global),
    ];
    (winning, trace)
}

/// Resolve the effective GitHub CLI auth-forward mode for a
/// (workspace, role) scope.
///
/// Walks three layers, most-specific wins:
///
/// 1. `workspaces[ws].roles[role].github`
/// 2. `workspaces[ws].github`
/// 3. `github` (global)
///
/// Returns [`GithubAuthMode::Sync`] when no layer is set. Unlike
/// Claude / Codex, the GitHub axis has no agent dimension because
/// `.config/gh/` is shared by every agent in the container.
pub fn resolve_github_mode(
    cfg: &AppConfig,
    workspace: Option<&WorkspaceName>,
    role: &str,
) -> GithubAuthMode {
    let ws = workspace_key(workspace);
    if let Some(m) = cfg
        .workspaces
        .get(ws)
        .and_then(|w| w.roles.get(role))
        .and_then(|ro| ro.github.as_ref().map(|g| g.auth_forward))
    {
        return m;
    }

    if let Some(m) = cfg
        .workspaces
        .get(ws)
        .and_then(|w| w.github.as_ref().map(|g| g.auth_forward))
    {
        return m;
    }

    cfg.github
        .as_ref()
        .map_or_else(GithubAuthMode::default, |g| g.auth_forward)
}

/// Resolve the effective sync source folder override for an agent in a
/// (workspace, role) scope — the parallel axis to `resolve_mode`.
///
/// Walks three layers, most-specific wins:
///
/// 1. `workspaces[ws].roles[role].<agent>.sync_source_dir`
/// 2. `workspaces[ws].<agent>.sync_source_dir`
/// 3. `<agent>.sync_source_dir` (global)
///
/// Returns `None` when no layer is set — caller falls back to the per-agent
/// hardcoded default folder from `AgentRuntime::state_paths().credential_dir`.
///
/// Resolves the optional source folder override for sync-mode credentials.
pub fn resolve_sync_source_dir(
    cfg: &AppConfig,
    agent: Agent,
    workspace: Option<&WorkspaceName>,
    role: &str,
) -> Option<std::path::PathBuf> {
    let ws = cfg.workspaces.get(workspace_key(workspace));
    // Most-specific first: workspace × role override.
    if let Some(dir) = ws
        .and_then(|ws| ws.roles.get(role))
        .and_then(|ro| ro.sync_source_dir_for(agent))
    {
        return Some(dir);
    }
    // Workspace-level.
    if let Some(dir) = ws.and_then(|ws| ws.sync_source_dir_for(agent)) {
        return Some(dir);
    }
    // Global level.
    cfg.sync_source_dir_for(agent)
}

/// Walk the three `[…github.env]` layers for the given pair.
///
/// Merges later layers over earlier ones. Used by the launcher to
/// discover `GH_TOKEN` / `GH_HOST` / `GH_ENTERPRISE_TOKEN` declarations
/// specific to GitHub auth without requiring operators to also list
/// them under the regular role/workspace `[*.env]` blocks.
pub fn build_github_env_layers(
    cfg: &AppConfig,
    workspace: Option<&WorkspaceName>,
    role: &str,
) -> BTreeMap<String, EnvValue> {
    let ws = cfg.workspaces.get(workspace_key(workspace));
    let layers = [
        cfg.github.as_ref().map(|g| &g.env),
        ws.and_then(|w| w.github.as_ref()).map(|g| &g.env),
        ws.and_then(|w| w.roles.get(role))
            .and_then(|ro| ro.github.as_ref())
            .map(|g| &g.env),
    ];
    let mut merged: BTreeMap<String, EnvValue> = BTreeMap::new();
    for env in layers.into_iter().flatten() {
        for (k, v) in env {
            merged.insert(k.clone(), v.clone());
        }
    }
    merged
}

/// Built-in role name → git URL pairs shipped with the binary.
pub const BUILTIN_ROLES: &[(&str, &str)] = &[
    (
        "agent-smith",
        "https://github.com/jackin-project/jackin-agent-smith.git",
    ),
    (
        "the-architect",
        "https://github.com/jackin-project/jackin-the-architect.git",
    ),
];

impl AppConfig {
    /// Resolve an existing role source or derive a new one from the selector.
    ///
    /// Returns `(source, is_new)`. When `is_new` is `true` the source has been
    /// inserted into the in-memory config but **not** persisted — the caller
    /// should call [`ConfigEditor::save`](crate::ConfigEditor::save) after
    /// validating that the repository is reachable.
    pub fn resolve_role_source(
        &mut self,
        selector: &RoleSelector,
    ) -> anyhow::Result<(RoleSource, bool)> {
        if let Some(source) = self.roles.get(&selector.key()) {
            return Ok((source.clone(), false));
        }

        let namespace = selector.namespace.as_ref().ok_or_else(|| {
            anyhow::Error::from(ConfigError::msg(format!(
                "unknown selector {}",
                selector.key()
            )))
        })?;

        // Agent roles on GitHub always follow the `jackin-{name}` convention.
        // When a namespaced selector is given as `owner/short-name`, we
        // synthesize `owner/jackin-short-name`.
        // If the caller already used the full repo slug (e.g.
        // `jackin-project/jackin-the-architect`), we keep it verbatim.
        let repo = if selector.name.starts_with("jackin-") {
            selector.name.clone()
        } else {
            format!("jackin-{}", selector.name)
        };

        let source = RoleSource {
            git: format!("https://github.com/{namespace}/{repo}.git"),
            trusted: false,
            env: BTreeMap::new(),
        };
        self.roles.insert(selector.key(), source.clone());
        Ok((source, true))
    }

    /// Mark a role source as trusted.  Returns `true` when the flag changed.
    // pub(crate): test-only affordance; production callers use ConfigEditor.
    pub fn trust_agent(&mut self, key: &str) -> bool {
        if let Some(source) = self.roles.get_mut(key)
            && !source.trusted
        {
            source.trusted = true;
            return true;
        }
        false
    }

    /// Revoke trust for a role source.  Returns `true` when the flag changed.
    /// Note: does not prevent revoking builtins — the caller should check
    /// [`Self::is_builtin_agent`] first.
    // pub(crate): test-only affordance; production callers use ConfigEditor.
    pub fn untrust_agent(&mut self, key: &str) -> bool {
        if let Some(source) = self.roles.get_mut(key)
            && source.trusted
        {
            source.trusted = false;
            return true;
        }
        false
    }

    /// Returns `true` when `key` matches a built-in role shipped with the
    /// binary.  Built-in roles are always trusted and cannot be revoked.
    pub fn is_builtin_agent(key: &str) -> bool {
        BUILTIN_ROLES.iter().any(|&(name, _)| name == key)
    }

    /// Ensures all built-in role entries match the current binary version.
    /// Returns `true` if any entries were added or updated.
    pub fn sync_builtin_agents(&mut self) -> bool {
        let mut changed = false;
        for &(name, git) in BUILTIN_ROLES {
            let expected = RoleSource {
                git: git.to_owned(),
                trusted: true,
                env: BTreeMap::new(),
            };
            match self.roles.get(name) {
                Some(existing) if existing.git == expected.git && existing.trusted => {}
                _ => {
                    self.roles.insert(name.to_owned(), expected);
                    changed = true;
                }
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests;
