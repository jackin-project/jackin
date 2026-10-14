// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};

use jackin_core::EnvValue;
use serde::{Deserialize, Serialize};

use jackin_core::Agent;

use crate::auth::GithubAuthConfig;
use crate::schema::{
    DirtyExitPolicy, DockerConfig, GitConfig, RoleSource, RuntimeConfig, TelemetryConfig,
    WorkspaceConfig,
};
use crate::versions::CURRENT_CONFIG_VERSION;

pub const DEFAULT_ROLE_REPO_REFRESH_TTL_SECONDS: u64 = 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppConfig {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub accounts: BTreeMap<String, crate::AccountConfig>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub account_bindings: BTreeMap<Agent, String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub account_scan_exclusions: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub agent_configurations: BTreeMap<String, crate::AgentConfiguration>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_launch: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bootstrap: Option<crate::BootstrapState>,
    #[serde(
        default = "crate::versions::current_config_version",
        rename = "version"
    )]
    pub version: String,
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
    #[serde(default, skip_serializing_if = "TelemetryConfig::is_default")]
    pub telemetry: TelemetryConfig,
    #[serde(default, skip_serializing_if = "GitConfig::is_default")]
    pub git: GitConfig,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub workspaces: BTreeMap<String, WorkspaceConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dirty_exit_policy: Option<DirtyExitPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role_repo_refresh_ttl_seconds: Option<u64>,
}

impl AppConfig {
    pub fn resolve_dirty_exit_policy(
        &self,
        workspace: Option<&WorkspaceConfig>,
    ) -> DirtyExitPolicy {
        workspace
            .and_then(|w| w.dirty_exit_policy)
            .or(self.dirty_exit_policy)
            .unwrap_or_default()
    }

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
            account_scan_exclusions: BTreeSet::new(),
            agent_configurations: BTreeMap::new(),
            default_launch: None,
            bootstrap: None,
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
