// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `AppConfig`: top-level operator configuration schema.
//!
//! Defines the `AppConfig` struct and its `Default` implementation.
//! Behavior (load, save, workspace CRUD, mount resolution, role
//! resolution) lives in the child modules `mounts`, `persist`,
//! `roles`, and `workspaces`.

use std::collections::BTreeMap;

use jackin_core::EnvValue;
use serde::{Deserialize, Serialize};

use jackin_core::Agent;

use crate::auth::GithubAuthConfig;
use crate::schema::{
    DirtyExitPolicy, DockerConfig, GitConfig, RoleSource, RuntimeConfig, TelemetryConfig,
    WorkspaceConfig,
};
use crate::versions::CURRENT_CONFIG_VERSION;

/// Default launch-time role-repo refresh window when the config omits a TTL.
pub const DEFAULT_ROLE_REPO_REFRESH_TTL_SECONDS: u64 = 60;

/// Top-level operator configuration (`~/.config/jackin/config.toml`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppConfig {
    /// Named agent and provider credentials.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub accounts: BTreeMap<String, crate::AccountConfig>,
    /// Global explicit account selections by agent.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub account_bindings: BTreeMap<Agent, String>,
    /// On-disk schema version (`version` key in `config.toml`).
    #[serde(
        default = "crate::versions::current_config_version",
        rename = "version"
    )]
    pub version: String,
    /// Global GitHub (`gh`) auth-forward policy and token env.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github: Option<GithubAuthConfig>,
    /// Global operator env map injected into every launch.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, EnvValue>,
    /// Named role sources (`[roles.<name>]`).
    #[serde(default)]
    pub roles: BTreeMap<String, RoleSource>,
    /// Global Docker security and named mount tables.
    #[serde(default)]
    pub docker: DockerConfig,
    /// Host-wide container backend defaults.
    #[serde(default, skip_serializing_if = "RuntimeConfig::is_default")]
    pub runtime: RuntimeConfig,
    /// Host-wide telemetry filtering.
    #[serde(default, skip_serializing_if = "TelemetryConfig::is_default")]
    pub telemetry: TelemetryConfig,
    /// Global git co-author / DCO settings.
    #[serde(default, skip_serializing_if = "GitConfig::is_default")]
    pub git: GitConfig,
    /// In-memory workspace map (loaded from split workspace files).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub workspaces: BTreeMap<String, WorkspaceConfig>,
    /// Global dirty-exit policy (D8). Per-workspace `dirty_exit_policy`
    /// overrides this. Defaults to `ask` when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dirty_exit_policy: Option<DirtyExitPolicy>,
    /// Freshness window for launch-time role repo fetches. `None` uses the
    /// built-in default; `Some(0)` preserves always-fetch behavior.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role_repo_refresh_ttl_seconds: Option<u64>,
}

impl AppConfig {
    /// Resolved dirty-exit policy for a session.
    ///
    /// Per-workspace `dirty_exit_policy` wins over the global setting; both
    /// fall back to the `Ask` built-in default (D8).
    pub fn resolve_dirty_exit_policy(
        &self,
        workspace: Option<&WorkspaceConfig>,
    ) -> DirtyExitPolicy {
        workspace
            .and_then(|w| w.dirty_exit_policy)
            .or(self.dirty_exit_policy)
            .unwrap_or_default()
    }

    /// Resolved role-repo fetch freshness window (config TTL or built-in default).
    pub fn role_repo_refresh_ttl(&self) -> std::time::Duration {
        std::time::Duration::from_secs(
            self.role_repo_refresh_ttl_seconds
                .unwrap_or(DEFAULT_ROLE_REPO_REFRESH_TTL_SECONDS),
        )
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: CURRENT_CONFIG_VERSION.to_owned(),
            accounts: BTreeMap::new(),
            account_bindings: BTreeMap::new(),
            github: None,
            env: BTreeMap::new(),
            roles: BTreeMap::new(),
            docker: DockerConfig::default(),
            runtime: RuntimeConfig::default(),
            telemetry: TelemetryConfig::default(),
            git: GitConfig::default(),
            workspaces: BTreeMap::new(),
            dirty_exit_policy: None,
            role_repo_refresh_ttl_seconds: None,
        }
    }
}

pub(crate) mod mounts;
pub(crate) mod persist;
pub(crate) mod roles;
pub(crate) mod workspaces;

#[cfg(test)]
mod tests;
