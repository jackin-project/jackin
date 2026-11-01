// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Compatibility, FileSystem, GatewayName, HostPath, ProcessRunner};
use jackin_openshell::{Gateway, GatewayError};

#[derive(Clone, Debug)]
pub struct DoctorInput<'a> {
    pub runner: &'a dyn ProcessRunner,
    pub fs: &'a dyn FileSystem,
    pub gateway: GatewayAccess<'a>,
    pub engine_socket: HostPath,
    pub pinned: &'a Compatibility,
}

#[derive(Clone, Copy, Debug)]
pub enum GatewayAccess<'a> {
    NotChosen,
    Connected(&'a dyn Gateway),
    Failed {
        name: &'a GatewayName,
        error: &'a GatewayError,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CheckId {
    OpenshellBinary,
    SshBinary,
    GitBinary,
    GatewayRegistered,
    GatewayHealth,
    PinnedVersion,
    EngineSocket,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
    Skip,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Check {
    pub id: CheckId,
    pub status: CheckStatus,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DoctorReport {
    pub checks: Vec<Check>,
}
