// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `AppConfig`: top-level operator configuration schema.
//!
//! Defines the `AppConfig` struct and its `Default` implementation.
//! Behavior (load, save, workspace CRUD, mount resolution, role
//! resolution) lives in the child modules `mounts`, `persist`,
//! `roles`, and `workspaces`.

use crate::ConfigError;
use std::collections::BTreeMap;

use jackin_core::EnvValue;
use serde::{Deserialize, Serialize};

use jackin_core::Agent;

use jackin_core::AuthForwardMode;

use crate::auth::{AgentAuthConfig, GithubAuthConfig};
use crate::schema::{
    DirtyExitPolicy, DockerConfig, GitConfig, RoleSource, RuntimeConfig, TelemetryConfig,
    WorkspaceConfig,
};
use crate::versions::CURRENT_CONFIG_VERSION;

/// Default launch-time role-repo refresh window when the config omits a TTL.
pub const DEFAULT_ROLE_REPO_REFRESH_TTL_SECONDS: u64 = 60;

/// Top-level operator configuration (`~/.config/jackin/config.toml`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// On-disk schema version (`version` key in `config.toml`).
    #[serde(
        default = "crate::versions::current_config_version",
        rename = "version"
    )]
    pub version: String,
    /// Global Claude auth-forward policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude: Option<AgentAuthConfig>,
    /// Global Codex auth-forward policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex: Option<AgentAuthConfig>,
    /// Global Amp auth-forward policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amp: Option<AgentAuthConfig>,
    /// Global Kimi auth-forward policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kimi: Option<AgentAuthConfig>,
    /// Global `OpenCode` auth-forward policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opencode: Option<AgentAuthConfig>,
    /// Global Grok auth-forward policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grok: Option<AgentAuthConfig>,
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
    /// Validates that no configured agent uses an auth mode unsupported by that agent.
    ///
    /// Preserves the "`OAuthToken` not supported" check formerly enforced by the
    /// per-agent serde newtypes.
    pub fn validate_auth_modes(&self) -> anyhow::Result<()> {
        let pairs: &[(Agent, Option<&AgentAuthConfig>)] = &[
            (Agent::Codex, self.codex.as_ref()),
            (Agent::Amp, self.amp.as_ref()),
            (Agent::Kimi, self.kimi.as_ref()),
            (Agent::Opencode, self.opencode.as_ref()),
        ];
        for (agent, cfg) in pairs {
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
        Ok(())
    }

    /// Auth-forward mode for `agent` at the global (top-level) config layer.
    ///
    /// Keep this match parallel with `WorkspaceConfig` and
    /// `WorkspaceRoleOverride`: these are versioned TOML structs with named
    /// agent fields, so the dispatch stays as one accessor per layer until a
    /// schema-bumped map migration.
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

    /// Sync source dir override for `agent` at the global config layer.
    ///
    /// Returns `None` when the field is absent at this layer — caller inherits
    /// from the per-agent hardcoded default.
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
            claude: None,
            codex: None,
            amp: None,
            kimi: None,
            opencode: None,
            grok: None,
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
