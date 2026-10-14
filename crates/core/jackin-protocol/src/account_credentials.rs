// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[cfg(test)]
mod tests;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceCredentialEnv {
    pub agent: String,
    pub account_id: String,
    pub env: BTreeMap<String, String>,
}

impl std::fmt::Debug for InstanceCredentialEnv {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("InstanceCredentialEnv([REDACTED])")
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentCredentialEnv {
    schema_version: u16,
    instances: BTreeMap<String, InstanceCredentialEnv>,
}

impl AgentCredentialEnv {
    pub fn new(instances: BTreeMap<String, InstanceCredentialEnv>) -> Self {
        Self {
            schema_version: 2,
            instances,
        }
    }
    pub fn schema_version(&self) -> u16 {
        self.schema_version
    }
    pub fn for_instance(&self, instance: &str) -> Option<&BTreeMap<String, String>> {
        self.instances.get(instance).map(|entry| &entry.env)
    }
    pub fn instance(&self, instance: &str) -> Option<&InstanceCredentialEnv> {
        self.instances.get(instance)
    }
    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }
    pub fn iter(&self) -> impl Iterator<Item = (&String, &InstanceCredentialEnv)> {
        self.instances.iter()
    }
}

impl Default for AgentCredentialEnv {
    fn default() -> Self {
        Self {
            schema_version: 2,
            instances: BTreeMap::new(),
        }
    }
}

impl std::fmt::Debug for AgentCredentialEnv {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AgentCredentialEnv([REDACTED])")
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StagedInstanceCredential {
    pub schema_version: u16,
    pub instance: String,
    pub credential: InstanceCredentialEnv,
}

impl std::fmt::Debug for StagedInstanceCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StagedInstanceCredential")
            .field("schema_version", &self.schema_version)
            .field("instance", &self.instance)
            .field("credential", &"<redacted>")
            .finish()
    }
}
