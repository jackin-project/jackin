//! Shared host CLI ↔ in-container Capsule contracts.
//!
//! Lives in its own crate so the host (`jackin`) and the
//! in-container binary (`jackin-capsule`) can both depend on it
//! without the host pulling in `jackin-capsule`'s tokio + PTY +
//! VT-parser stack. Most declarations here are wire-format types;
//! small constants that name the host↔Capsule runtime contract live
//! here too so the two binaries cannot drift.

pub mod control;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Filename written under `/jackin/run/` by the host launcher.
pub const CAPSULE_CONFIG_FILENAME: &str = "agent.toml";

/// Normalized runtime config path read by Capsule PID 1.
pub const CAPSULE_CONFIG_PATH: &str = "/jackin/run/agent.toml";

/// Host-validated role/session facts Capsule needs to spawn panes.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapsuleConfig {
    pub role: String,
    pub workdir: String,
    #[serde(default)]
    pub agents: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub models: BTreeMap<String, String>,
    /// When the operator picked a specific provider in the console's
    /// launch flow (before the container existed), this field tells the
    /// capsule's initial spawn to use that provider and env overrides
    /// instead of defaulting to Anthropic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_provider: Option<InitialProvider>,
}

/// Provider selection for the capsule's initial session spawn.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InitialProvider {
    pub label: String,
    pub env_overrides: Vec<(String, String)>,
}

impl CapsuleConfig {
    pub fn supported_agents(&self) -> Vec<String> {
        self.agents.clone()
    }

    pub fn model_for_agent(&self, agent: &str) -> Option<&str> {
        self.models.get(agent).map(String::as_str)
    }
}

/// Container-name grammar shared by the host launcher and the
/// in-container capsule. The launcher constructs names of the shape
/// `jk-<id>[-<workspace>]-<role>`; both binaries must agree on how
/// to parse them.
pub const CONTAINER_PREFIX: &str = "jk";

/// Prefix with the trailing separator, used by [`instance_id_from_container_base`]
/// to strip the family marker before splitting.
pub const CONTAINER_PREFIX_DASH: &str = "jk-";

/// Extract the instance-ID component from a container base name.
///
/// Returns `None` when the name does not start with `jk-` or has no
/// `-` after the id component. Single parser shared by host
/// manifest construction (`JACKIN_INSTANCE_ID` injection) and the
/// capsule's status bar so the two surfaces cannot drift on what a
/// `jk-…` name means.
#[must_use]
pub fn instance_id_from_container_base(container_base: &str) -> Option<&str> {
    container_base
        .strip_prefix(CONTAINER_PREFIX_DASH)?
        .split_once('-')
        .map(|(id, _)| id)
}
