// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `RoleState` record and preparation context.

use jackin_instance_agents::{AgentRuntimeState, ProvisionedAuth};
use jackin_instance_credentials::{AuthMountLease, AuthProvisionOutcome, GithubProvisionOutcome};

use jackin_config::AuthForwardMode;

use std::collections::{BTreeMap, BTreeSet};

use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct RoleState {
    pub root: PathBuf,
    pub gh_config_dir: PathBuf,
    /// Resolved GitHub provisioning outcome from
    /// [`jackin_instance_agents::provision_github_auth`]. The variant carries the resolved
    /// token (when applicable) so callers can derive `GH_TOKEN` /
    /// `GITHUB_TOKEN` via [`GithubProvisionOutcome::token`] without a
    /// parallel `Option<String>` field.
    pub gh_provision_outcome: GithubProvisionOutcome,
    pub agent_runtime: AgentRuntimeState,
    pub auth: ProvisionedAuth,
    pub auth_outcomes: BTreeMap<jackin_core::Agent, AuthProvisionOutcome>,
    /// Auth paths admitted with descriptor checks before runtime mount
    /// construction. These leases are uniquely owned by launch state and
    /// stay held until that state is dropped.
    pub auth_mount_paths: BTreeSet<PathBuf>,
    pub auth_mount_leases: Vec<AuthMountLease>,
    /// Generated provider config overlays from the host authority namespace.
    /// Each source is mounted read-only at its per-instance destination.
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

    /// Host path to Claude's account-metadata file. `None` when Claude is
    /// not in `supported_agents()`. Pair with [`Self::claude_forwards_auth`]
    /// when filtering for runtime reachability.
    #[must_use]
    pub fn claude_account_json(&self) -> Option<&Path> {
        self.auth
            .for_agent(jackin_core::Agent::Claude)
            .and_then(|slot| slot.credential_paths.first().map(PathBuf::as_path))
    }

    /// Manifest model override for Claude, or `None` when the selected agent
    /// is not Claude or when no override is configured.
    #[must_use]
    pub fn claude_model(&self) -> Option<&str> {
        if self.agent_runtime.agent == jackin_core::Agent::Claude {
            self.agent_runtime.model.as_deref()
        } else {
            None
        }
    }

    /// Host path to Claude's OAuth credentials file. `None` when Claude is
    /// not in `supported_agents()`. Pair with [`Self::claude_forwards_auth`].
    #[must_use]
    pub fn claude_credentials_json(&self) -> Option<&Path> {
        self.auth
            .for_agent(jackin_core::Agent::Claude)
            .and_then(|slot| slot.credential_paths.get(1).map(PathBuf::as_path))
    }

    /// Whether Claude's auth files flow into the container under
    /// `/jackin/claude/`. `false` for env-driven modes (`ignore` /
    /// `api_key` / `oauth_token`) and when Claude is not in
    /// `supported_agents()`.
    #[must_use]
    pub fn claude_forwards_auth(&self) -> bool {
        self.auth
            .for_agent(jackin_core::Agent::Claude)
            .is_some_and(|slot| slot.forward_auth)
    }

    /// Manifest model override for Codex, or `None` if not Codex or no override.
    #[must_use]
    pub fn codex_model(&self) -> Option<&str> {
        if self.agent_runtime.agent == jackin_core::Agent::Codex {
            self.agent_runtime.model.as_deref()
        } else {
            None
        }
    }

    /// Manifest model override for Kimi, or `None` if not Kimi or no override.
    #[must_use]
    pub fn kimi_model(&self) -> Option<&str> {
        if self.agent_runtime.agent == jackin_core::Agent::Kimi {
            self.agent_runtime.model.as_deref()
        } else {
            None
        }
    }

    /// Manifest model override for `OpenCode`, or `None` if not `OpenCode` or no override.
    #[must_use]
    pub fn opencode_model(&self) -> Option<&str> {
        if self.agent_runtime.agent == jackin_core::Agent::Opencode {
            self.agent_runtime.model.as_deref()
        } else {
            None
        }
    }

    /// Manifest model override for Grok, or `None` if not Grok or no override.
    #[must_use]
    pub fn grok_model(&self) -> Option<&str> {
        if self.agent_runtime.agent == jackin_core::Agent::Grok {
            self.agent_runtime.model.as_deref()
        } else {
            None
        }
    }
}

/// Resolver closures for [`RoleState::prepare`].
#[expect(
    missing_debug_implementations,
    reason = "PrepareResolvers carries borrowed closures; callers log resolved values instead."
)]
pub struct PrepareResolvers<'a> {
    pub auth_modes: &'a dyn Fn(jackin_core::Agent) -> AuthForwardMode,
    pub sync_source_dirs: &'a dyn Fn(jackin_core::Agent) -> Option<PathBuf>,
}
