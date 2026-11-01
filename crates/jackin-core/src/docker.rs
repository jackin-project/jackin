// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::num::NonZeroU16;

use thiserror::Error;

use crate::paths::HostPath;

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
        "the Docker capability is enabled by {enabled_by:?} but the gateway has no Docker engine"
    )]
    EngineMissing { enabled_by: DockerSource },
}
