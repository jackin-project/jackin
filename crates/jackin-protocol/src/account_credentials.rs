// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Protected per-agent credential transport, separate from public capsule config.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Resolved credential and routing variables grouped by agent.
/// Debug output never contains credential values.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AgentCredentialEnv(BTreeMap<String, BTreeMap<String, String>>);
impl AgentCredentialEnv {
    /// Wrap a host-resolved per-agent map.
    pub fn new(values: BTreeMap<String, BTreeMap<String, String>>) -> Self {
        Self(values)
    }
    /// Variables belonging exclusively to this agent.
    pub fn for_agent(&self, agent: &str) -> Option<&BTreeMap<String, String>> {
        self.0.get(agent)
    }
    /// Whether no agent has credentials.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    /// Iterate agent names and their protected variables.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &BTreeMap<String, String>)> {
        self.0.iter()
    }
}
impl std::fmt::Debug for AgentCredentialEnv {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AgentCredentialEnv([REDACTED])")
    }
}
