#![deny(
    clippy::string_slice,
    clippy::indexing_slicing,
    clippy::get_unwrap,
    clippy::unwrap_in_result,
    clippy::panic_in_result_fn,
    clippy::unchecked_time_subtraction
)]

mod account_credentials;
pub use account_credentials::{
    AgentCredentialEnv, InstanceCredentialEnv, StagedInstanceCredential,
};

use jackin_core::container_paths;

pub mod agent_status;
pub mod attach;
pub mod capsule_transport;
pub mod control;
pub mod snapshot;
pub mod telemetry_context;
pub mod usage_broker;
pub mod usage_monitor;

pub use telemetry_context::TelemetryContext;

pub use snapshot::InstanceSnapshot;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecKind {
    Op,

    Env,

    Literal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecBinding {
    pub name: String,

    pub kind: ExecKind,

    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CredRequest {
    pub ctx: TelemetryContext,

    pub refs: Vec<ExecBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CredReply {
    Ok { values: BTreeMap<String, String> },

    Error { error: String },
}

pub const CAPSULE_CONFIG_FILENAME: &str = "agent.toml";

pub const CAPSULE_CONFIG_PATH: &str = container_paths::CAPSULE_CONFIG;

pub const CAPSULE_SUPERVISOR_PID_ENV: &str = "JACKIN_CAPSULE_SUPERVISOR_PID";

pub const APPLE_CAPSULE_SUPERVISOR_PID: u32 = 2;

pub const HOST_SOCK_CONTAINER_PATH: &str = container_paths::HOST_SOCK;

pub const SESSION_CAPABILITY_ENV: &str = "JACKIN_SESSION_CAPABILITY";

pub const ISOLATION_SESSION_ID_ENV: &str = "JACKIN_ISOLATION_SESSION_ID";

pub const SESSION_ID_ENV: &str = "JACKIN_SESSION_ID";

pub const SESSION_STATE_DIR_ENV: &str = "JACKIN_SESSION_STATE_DIR";

pub const AUTH_MODE_ENV: &str = "JACKIN_AUTH_MODE";

pub const INSTANCE_ENV: &str = "JACKIN_INSTANCE";

pub const INSTANCE_FORWARDED_DIR_ENV: &str = "JACKIN_FORWARDED_DIR";

pub const ACCOUNT_CREDENTIALS_DIR: &str = "/jackin/account-credentials";

pub const EXIT_ACTION_FILENAME: &str = "exit-action.json";

pub const EXIT_ACTION_PATH: &str = container_paths::EXIT_ACTION;

#[must_use]
pub fn account_credentials_filename(instance: &str) -> String {
    let mut encoded = String::with_capacity(instance.len() * 2 + 5);
    encoded.push_str("acct-");
    for byte in instance.as_bytes() {
        encoded.push_str(&format!("{byte:02x}"));
    }
    encoded.push_str(".json");
    encoded
}

#[must_use]
pub fn account_credentials_container_path(instance: &str) -> String {
    format!(
        "{}/{}",
        ACCOUNT_CREDENTIALS_DIR,
        account_credentials_filename(instance)
    )
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionIdentity {
    pub uid: u32,

    pub gid: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExitAction {
    Keep,

    Discard,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapsuleConfig {
    pub role: String,

    pub workdir: String,
    #[serde(default)]
    pub instances: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub agents: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub models: BTreeMap<String, String>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub efforts: BTreeMap<String, String>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub auth_modes: BTreeMap<String, String>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub accounts: BTreeMap<String, String>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub usage_capabilities: BTreeMap<String, usage_broker::UsageAccountCapability>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub credential_provider_surfaces: BTreeMap<String, String>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<String, String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub claude_marketplaces: Vec<ClaudeMarketplace>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub claude_plugins: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exec_bindings: Vec<ExecBinding>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dirty_exit_policy: Option<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub isolated_worktrees: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub workspace_mounts: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub worktree_git_targets: Vec<String>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub instance_home_dirs: BTreeMap<String, String>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub instance_cache_dirs: BTreeMap<String, String>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub instance_forwarded_dirs: BTreeMap<String, String>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub instance_credential_files: BTreeMap<String, String>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub instance_mount_paths: BTreeMap<String, Vec<String>>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub instance_identities: BTreeMap<String, SessionIdentity>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell_identity: Option<SessionIdentity>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClaudeMarketplace {
    pub source: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sparse: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Provider {
    Anthropic,

    Openai,

    Zai,

    Minimax,

    Kimi,
}

impl Provider {
    pub const ALL: [Provider; 5] = [
        Provider::Anthropic,
        Provider::Openai,
        Provider::Zai,
        Provider::Minimax,
        Provider::Kimi,
    ];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Anthropic => "Anthropic",
            Self::Openai => "OpenAI",
            Self::Zai => "Z.AI",
            Self::Minimax => "MiniMax",
            Self::Kimi => "Kimi",
        }
    }

    #[must_use]
    pub fn from_label(label: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|provider| provider.label() == label)
    }
}

impl CapsuleConfig {
    pub fn supported_instances(&self) -> Vec<String> {
        self.instances.clone()
    }

    pub fn model_for_instance(&self, instance: &str) -> Option<&str> {
        self.models.get(instance).map(String::as_str)
    }

    #[must_use]
    pub fn effort_for_instance(&self, instance: &str) -> Option<&str> {
        self.efforts.get(instance).map(String::as_str)
    }

    #[must_use]
    pub fn auth_mode_for_instance(&self, instance: &str) -> Option<&str> {
        self.auth_modes.get(instance).map(String::as_str)
    }

    #[must_use]
    pub fn agent_for_instance(&self, instance: &str) -> Option<&str> {
        self.agents.get(instance).map(String::as_str)
    }

    #[must_use]
    pub fn account_for_instance(&self, instance: &str) -> Option<&str> {
        self.accounts.get(instance).map(String::as_str)
    }

    #[must_use]
    pub fn usage_capability_for_instance(
        &self,
        instance: &str,
    ) -> Option<&usage_broker::UsageAccountCapability> {
        self.usage_capabilities.get(instance)
    }

    #[must_use]
    pub fn credential_provider_surface_for_instance(&self, instance: &str) -> Option<&str> {
        self.credential_provider_surfaces
            .get(instance)
            .map(String::as_str)
    }

    #[must_use]
    pub fn label_for_instance(&self, instance: &str) -> Option<&str> {
        self.labels.get(instance).map(String::as_str)
    }

    #[must_use]
    pub fn home_for_instance(&self, instance: &str) -> Option<&str> {
        self.instance_home_dirs.get(instance).map(String::as_str)
    }

    #[must_use]
    pub fn cache_for_instance(&self, instance: &str) -> Option<&str> {
        self.instance_cache_dirs.get(instance).map(String::as_str)
    }

    #[must_use]
    pub fn forwarded_for_instance(&self, instance: &str) -> Option<&str> {
        self.instance_forwarded_dirs
            .get(instance)
            .map(String::as_str)
    }

    #[must_use]
    pub fn credential_file_for_instance(&self, instance: &str) -> Option<&str> {
        self.instance_credential_files
            .get(instance)
            .map(String::as_str)
    }

    #[must_use]
    pub fn identity_for_instance(&self, instance: &str) -> Option<SessionIdentity> {
        self.instance_identities.get(instance).copied()
    }

    #[must_use]
    pub fn mount_paths_for_instance(&self, instance: &str) -> &[String] {
        self.instance_mount_paths
            .get(instance)
            .map_or(&[][..], Vec::as_slice)
    }

    pub fn resolve_instance(&self, raw: &str) -> Result<&str, &'static str> {
        if let Some(id) = self.instances.iter().find(|id| id.as_str() == raw) {
            return if self.agents.contains_key(id) {
                Ok(id.as_str())
            } else {
                Err("instance has no agent runtime in launch config")
            };
        }
        let mut hit = None;
        for id in &self.instances {
            if self.agents.get(id).is_some_and(|slug| slug == raw) {
                if hit.is_some() {
                    return Err("ambiguous agent: several instances share it; pick an instance");
                }
                hit = Some(id.as_str());
            }
        }
        hit.ok_or("not in launch config allowlist")
    }
}

pub mod host_terminal;
#[cfg(test)]
mod tests;
