// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use jackin_core::{EnvName, SandboxPath};
use jackin_openshell::{GatewayError, GatewayOp};

#[derive(Debug)]
pub struct FakeGateway {}

#[derive(Clone, Debug)]
pub struct SandboxHandle {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgramScript {
    pub exit_code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunRecord {
    pub env: BTreeMap<EnvName, String>,
    pub exit_code: i32,
}

#[derive(Debug)]
pub enum Fault {
    FailNext {
        op: GatewayOp,
        error: GatewayError,
    },
    FailAfter {
        op: GatewayOp,
        calls: u32,
        error: GatewayError,
    },
    StallProgram {
        program: SandboxPath,
    },
    CorruptDownload,
    ProvisionFor {
        polls: u32,
    },
    ProvisionFails {
        creates: u32,
    },
}
