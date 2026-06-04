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

use crate::auth::{
    AgentAuthConfig, AmpAuthConfig, CodexAuthConfig, GithubAuthConfig, KimiAuthConfig,
    OpencodeAuthConfig,
};
use crate::schema::{DockerConfig, GitConfig, RoleSource, WorkspaceConfig};
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
    pub codex: Option<CodexAuthConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amp: Option<AmpAuthConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kimi: Option<KimiAuthConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opencode: Option<OpencodeAuthConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github: Option<GithubAuthConfig>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, EnvValue>,
    #[serde(default)]
    pub roles: BTreeMap<String, RoleSource>,
    #[serde(default)]
    pub docker: DockerConfig,
    #[serde(default, skip_serializing_if = "GitConfig::is_default")]
    pub git: GitConfig,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub workspaces: BTreeMap<String, WorkspaceConfig>,
}

impl AppConfig {
    /// Auth-forward mode for `agent` at the global (top-level) config layer.
    ///
    /// Collapses the five-arm `match agent { Agent::Claude => self.claude…, … }`
    /// pattern used in `resolve_mode_with_trace` (Defect 46 Phase 2).
    pub fn auth_forward_for(&self, agent: Agent) -> Option<AuthForwardMode> {
        match agent {
            Agent::Claude => self.claude.as_ref().map(|c| c.auth_forward),
            Agent::Codex => self.codex.as_ref().map(|c| c.auth_forward),
            Agent::Amp => self.amp.as_ref().map(|c| c.auth_forward),
            Agent::Kimi => self.kimi.as_ref().map(|c| c.auth_forward),
            Agent::Opencode => self.opencode.as_ref().map(|c| c.auth_forward),
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: CURRENT_CONFIG_VERSION.to_string(),
            claude: None,
            codex: None,
            amp: None,
            kimi: None,
            opencode: None,
            github: None,
            env: BTreeMap::new(),
            roles: BTreeMap::new(),
            docker: DockerConfig::default(),
            git: GitConfig::default(),
            workspaces: BTreeMap::new(),
        }
    }
}
