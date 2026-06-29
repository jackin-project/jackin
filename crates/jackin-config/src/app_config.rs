//! `AppConfig`: top-level operator configuration schema.
//!
//! Defines the `AppConfig` struct and its `Default` implementation.
//! Behavior (load, save, workspace CRUD, mount resolution, role
//! resolution) lives in the sibling `app_config_persist`,
//! `app_config_workspaces`, `app_config_mounts`, and `app_config_roles`
//! modules.

use std::collections::BTreeMap;

use jackin_core::EnvValue;
use serde::{Deserialize, Serialize};

use jackin_core::agent::Agent;

use jackin_core::AuthForwardMode;

use crate::auth::{AgentAuthConfig, GithubAuthConfig};
use crate::schema::{
    DirtyExitPolicy, DockerConfig, GitConfig, RoleSource, RuntimeConfig, WorkspaceConfig,
};
use crate::versions::CURRENT_CONFIG_VERSION;

/// Top-level operator configuration (`~/.config/jackin/config.toml`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(
        default = "crate::versions::current_config_version",
        rename = "version"
    )]
    pub version: String,
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
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, EnvValue>,
    #[serde(default)]
    pub roles: BTreeMap<String, RoleSource>,
    #[serde(default)]
    pub docker: DockerConfig,
    #[serde(default, skip_serializing_if = "RuntimeConfig::is_default")]
    pub runtime: RuntimeConfig,
    #[serde(default, skip_serializing_if = "GitConfig::is_default")]
    pub git: GitConfig,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub workspaces: BTreeMap<String, WorkspaceConfig>,
    /// Global dirty-exit policy (D8). Per-workspace `dirty_exit_policy`
    /// overrides this. Defaults to `ask` when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dirty_exit_policy: Option<DirtyExitPolicy>,
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
                anyhow::bail!(
                    "auth_forward 'oauth_token' is not supported for {}",
                    agent.slug()
                );
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
            git: GitConfig::default(),
            workspaces: BTreeMap::new(),
            dirty_exit_policy: None,
        }
    }
}

pub mod mounts;
pub mod persist;
pub mod roles;
pub mod workspaces;

#[cfg(test)]
mod tests;
