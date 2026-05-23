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
#[serde(deny_unknown_fields)]
pub struct CapsuleConfig {
    pub role: String,
    pub workdir: String,
    #[serde(default)]
    pub agents: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub models: BTreeMap<String, String>,
}

impl CapsuleConfig {
    pub fn supported_agents(&self) -> Vec<String> {
        self.agents.clone()
    }

    pub fn model_for_agent(&self, agent: &str) -> Option<&str> {
        self.models.get(agent).map(String::as_str)
    }
}
