// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::ContainerId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DockerResources {
    pub role_container: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dind_container: Option<String>,
    pub network: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub certs_volume: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DockerIdentity {
    pub role_container_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dind_container_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppleContainerResources {
    pub container_name: String,

    pub role_image_ref: String,

    pub inner_docker_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendResources {
    Docker(DockerResources),
    AppleContainer(AppleContainerResources),
}

impl DockerResources {
    #[must_use]
    pub fn from_container_id(container_id: &ContainerId) -> Self {
        Self::from_container_name(container_id.as_str())
    }

    pub fn from_container_name(container_name: &str) -> Self {
        Self {
            role_container: container_name.to_owned(),
            dind_container: Some(crate::naming::dind_container_name(container_name)),
            network: crate::naming::role_network_name(container_name),
            certs_volume: Some(crate::naming::dind_certs_volume(container_name)),
        }
    }
}
