// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Protected per-instance credential transport, separate from public capsule config.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[cfg(test)]
mod tests;

/// Credential envelope for one spawned instance, keyed by config id in [`AgentCredentialEnv`].
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceCredentialEnv {
    /// Agent runtime slug for this instance.
    pub agent: String,
    /// Owning account ID for this instance.
    pub account_id: String,
    /// ONLY this instance's variables.
    pub env: BTreeMap<String, String>,
}

impl std::fmt::Debug for InstanceCredentialEnv {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("InstanceCredentialEnv([REDACTED])")
    }
}

/// Resolved credential and routing variables grouped by instance config id.
/// Debug output never contains credential values.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentCredentialEnv {
    schema_version: u16,
    instances: BTreeMap<String, InstanceCredentialEnv>,
}

impl AgentCredentialEnv {
    /// Wrap a host-resolved per-instance map as schema v2.
    pub fn new(instances: BTreeMap<String, InstanceCredentialEnv>) -> Self {
        Self {
            schema_version: 2,
            instances,
        }
    }
    /// Envelope schema version.
    pub fn schema_version(&self) -> u16 {
        self.schema_version
    }
    /// Credential variables belonging exclusively to this instance.
    pub fn for_instance(&self, instance: &str) -> Option<&BTreeMap<String, String>> {
        self.instances.get(instance).map(|entry| &entry.env)
    }
    /// Full credential entry for this instance.
    pub fn instance(&self, instance: &str) -> Option<&InstanceCredentialEnv> {
        self.instances.get(instance)
    }
    /// Whether no instance has credentials.
    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }
    /// Iterate instance config ids and their credential entries.
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

/// One on-disk credential transport unit. A file contains one admitted
/// instance only; the former container-wide v2 envelope is not accepted for
/// launch.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StagedInstanceCredential {
    /// Transport schema version.
    pub schema_version: u16,
    /// Instance ID expected by the mount/config entry.
    pub instance: String,
    /// The sole admitted instance credential entry.
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
