// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_instance_agents::{AgentRuntimeState, ProvisionedAuth};
use jackin_instance_credentials::{AuthMountLease, AuthProvisionOutcome, GithubProvisionOutcome};

use jackin_config::AuthForwardMode;

use std::collections::{BTreeMap, BTreeSet};

use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct RoleState {
    pub root: PathBuf,
    pub gh_config_dir: PathBuf,
    pub gh_provision_outcome: GithubProvisionOutcome,
    pub agent_runtime: AgentRuntimeState,
    pub auth: ProvisionedAuth,
    pub auth_outcomes: BTreeMap<jackin_core::Agent, AuthProvisionOutcome>,
    pub auth_mount_paths: BTreeSet<PathBuf>,
    pub auth_mount_leases: Vec<AuthMountLease>,
    pub provider_config_mounts: Vec<(PathBuf, String)>,
}

impl RoleState {
    pub fn auth_mount_file_allowed(&self, path: &Path) -> anyhow::Result<bool> {
        if !self.auth_mount_paths.contains(path) {
            return Ok(false);
        }
        jackin_instance_credentials::mount_file_present(path)
    }

    pub fn auth_mount_directory_allowed(&self, path: &Path) -> anyhow::Result<bool> {
        if !self.auth_mount_paths.contains(path) {
            return Ok(false);
        }
        jackin_instance_credentials::mount_directory_present(path)
    }

    pub fn mount_directory_allowed(&self, path: &Path) -> anyhow::Result<bool> {
        jackin_instance_credentials::mount_directory_present(path)
    }

    pub fn mount_file_allowed(&self, path: &Path) -> anyhow::Result<bool> {
        jackin_instance_credentials::mount_file_present(path)
    }

    #[must_use]
    pub fn claude_account_json(&self) -> Option<&Path> {
        self.auth
            .for_agent(jackin_core::Agent::Claude)
            .and_then(|slot| slot.credential_paths.first().map(PathBuf::as_path))
    }

    #[must_use]
    pub fn claude_model(&self) -> Option<&str> {
        if self.agent_runtime.agent == jackin_core::Agent::Claude {
            self.agent_runtime.model.as_deref()
        } else {
            None
        }
    }

    #[must_use]
    pub fn claude_credentials_json(&self) -> Option<&Path> {
        self.auth
            .for_agent(jackin_core::Agent::Claude)
            .and_then(|slot| slot.credential_paths.get(1).map(PathBuf::as_path))
    }

    #[must_use]
    pub fn claude_forwards_auth(&self) -> bool {
        self.auth
            .for_agent(jackin_core::Agent::Claude)
            .is_some_and(|slot| slot.forward_auth)
    }

    #[must_use]
    pub fn codex_model(&self) -> Option<&str> {
        if self.agent_runtime.agent == jackin_core::Agent::Codex {
            self.agent_runtime.model.as_deref()
        } else {
            None
        }
    }

    #[must_use]
    pub fn kimi_model(&self) -> Option<&str> {
        if self.agent_runtime.agent == jackin_core::Agent::Kimi {
            self.agent_runtime.model.as_deref()
        } else {
            None
        }
    }

    #[must_use]
    pub fn opencode_model(&self) -> Option<&str> {
        if self.agent_runtime.agent == jackin_core::Agent::Opencode {
            self.agent_runtime.model.as_deref()
        } else {
            None
        }
    }

    #[must_use]
    pub fn grok_model(&self) -> Option<&str> {
        if self.agent_runtime.agent == jackin_core::Agent::Grok {
            self.agent_runtime.model.as_deref()
        } else {
            None
        }
    }
}

#[expect(
    missing_debug_implementations,
    reason = "PrepareResolvers carries borrowed closures; callers log resolved values instead."
)]
pub struct PrepareResolvers<'a> {
    pub auth_modes: &'a dyn Fn(jackin_core::Agent) -> AuthForwardMode,
    pub sync_source_dirs: &'a dyn Fn(jackin_core::Agent) -> Option<PathBuf>,
}
