// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fmt;
use std::num::NonZeroU16;

use thiserror::Error;

use crate::ids::EnvName;
use crate::paths::{HostPath, sandbox_layout};

pub const ENGINE_HOST: &str = "host.openshell.internal";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineConfig {
    pub port: NonZeroU16,
    pub client_certs: HostPath,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DockerSource {
    Role,
    Workspace,
    Both,
}

impl fmt::Display for DockerSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            DockerSource::Role => "the role",
            DockerSource::Workspace => "the workspace",
            DockerSource::Both => "both the role and the workspace",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineAccess {
    pub engine: EngineConfig,
    pub enabled_by: DockerSource,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DockerCapability {
    Disabled,
    Enabled(EngineAccess),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DockerLayer {
    Absent,
    Client,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DockerRisk {
    ClientKeyReadable,
    ContainersBypassPolicy,
    BindMountsNameEngineHost,
    EngineOutlivesSandbox,
}

impl DockerRisk {
    pub const ALL: [DockerRisk; 4] = [
        DockerRisk::ClientKeyReadable,
        DockerRisk::ContainersBypassPolicy,
        DockerRisk::BindMountsNameEngineHost,
        DockerRisk::EngineOutlivesSandbox,
    ];
}

#[derive(Debug, Error)]
pub enum DockerError {
    #[error(
        "the Docker capability is enabled by {enabled_by} but the gateway has no Docker engine"
    )]
    EngineMissing { enabled_by: DockerSource },
}

impl DockerCapability {
    pub fn resolve(
        role_enabled: bool,
        workspace_enabled: bool,
        engine: Option<&EngineConfig>,
    ) -> Result<DockerCapability, DockerError> {
        let enabled_by = match (role_enabled, workspace_enabled) {
            (false, false) => return Ok(DockerCapability::Disabled),
            (true, false) => DockerSource::Role,
            (false, true) => DockerSource::Workspace,
            (true, true) => DockerSource::Both,
        };
        engine
            .map(|engine| {
                DockerCapability::Enabled(EngineAccess {
                    engine: engine.clone(),
                    enabled_by,
                })
            })
            .ok_or(DockerError::EngineMissing { enabled_by })
    }

    pub fn layer(&self) -> DockerLayer {
        match self {
            DockerCapability::Disabled => DockerLayer::Absent,
            DockerCapability::Enabled(_) => DockerLayer::Client,
        }
    }

    pub fn environment(&self) -> BTreeMap<EnvName, String> {
        let DockerCapability::Enabled(access) = self else {
            return BTreeMap::new();
        };
        [
            (
                "DOCKER_HOST",
                format!("tcp://{ENGINE_HOST}:{}", access.engine.port),
            ),
            ("DOCKER_TLS_VERIFY", "1".to_owned()),
            (
                "DOCKER_CERT_PATH",
                sandbox_layout::DOCKER_DIR.as_str().to_owned(),
            ),
        ]
        .into_iter()
        .map(|(name, value)| (EnvName::from_static(name), value))
        .collect()
    }
}
