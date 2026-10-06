// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Container backend resource records.

use jackin_core::ContainerId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DockerResources {
    pub role_container: String,
    /// `DinD` sidecar container name. `None` when the launch used
    /// `dind = "none"` (DinD-free role or `locked`/`hardened` profile without
    /// an explicit `DinD` grant).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dind_container: Option<String>,
    pub network: String,
    /// `DinD` TLS cert volume name. `None` when there is no `DinD` sidecar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub certs_volume: Option<String>,
}

/// Immutable Docker IDs captured by the launch that created these resources.
/// Names are descriptive only and cannot establish container ownership.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DockerIdentity {
    pub role_container_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dind_container_id: Option<String>,
}

/// Resources backing an apple-container instance. The lifecycle CLI
/// (`container run/exec/stop/rm`) keys off `container_name`; the other fields
/// record what the VM is running for the session contract and reconnect path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppleContainerResources {
    /// Name of the apple/container container: `jackin-<instance-id>`.
    pub container_name: String,
    /// OCI image ref used to start the container.
    pub role_image_ref: String,
    /// Whether an inner rootless Docker daemon (`DinD`) is running. Gated on the
    /// Phase 0 empirical validation of rootless `DinD` inside an apple/container
    /// VM; `false` until that gate passes.
    pub inner_docker_enabled: bool,
}

/// Backend-specific resources for an instance. `Docker` carries the four
/// container/network/volume names; `AppleContainer` carries the VM container
/// identity. Serialized as a tagged union so the manifest is self-describing
/// about which backend launched it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendResources {
    Docker(DockerResources),
    AppleContainer(AppleContainerResources),
}

impl DockerResources {
    /// Derive Docker resource names from an already validated container id.
    #[must_use]
    pub fn from_container_id(container_id: &ContainerId) -> Self {
        Self::from_container_name(container_id.as_str())
    }

    /// Derive all four Docker resource names from the role container name.
    ///
    /// Invariant: all derived names follow the same suffix conventions used
    /// by `runtime::naming` helpers, so `docker inspect` on any of the four
    /// names produces results consistent with the naming registry.
    pub fn from_container_name(container_name: &str) -> Self {
        Self {
            role_container: container_name.to_owned(),
            dind_container: Some(crate::naming::dind_container_name(container_name)),
            network: crate::naming::role_network_name(container_name),
            certs_volume: Some(crate::naming::dind_certs_volume(container_name)),
        }
    }
}
