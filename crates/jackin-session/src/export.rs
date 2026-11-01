// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    BranchName, CommitId, PathError, Phase, ProcessStatus, SandboxName, Sha256, TerminalText,
    WorkspaceName,
};
use jackin_openshell::GatewayError;
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportedBranch {
    pub branch: BranchName,
    pub tip: CommitId,
    pub host_ref: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportReport {
    pub sandbox: SandboxName,
    pub bundle: Sha256,
    pub branches: Vec<ExportedBranch>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnknownReason {
    NotRunning(Phase),
    WorkspaceUnknown(WorkspaceName),
    NoRepository,
    InspectionFailed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unexported {
    pub uncommitted: bool,
    pub branches: Vec<BranchName>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkState {
    Exported,
    Unexported(Unexported),
    Unknown(UnknownReason),
}

#[derive(Debug, Error)]
pub enum ExportError {
    #[error(transparent)]
    Gateway(#[from] GatewayError),
    #[error("sandbox {0:?} was not found")]
    NotFound(SandboxName),
    #[error("sandbox {sandbox:?} is not running: {phase:?}")]
    NotRunning { sandbox: SandboxName, phase: Phase },
    #[error("unknown workspace {0:?}")]
    WorkspaceUnknown(WorkspaceName),
    #[error(transparent)]
    Project(#[from] PathError),
    #[error("git in the sandbox exited with {exit_code}: {output:?}")]
    SandboxGit {
        exit_code: i32,
        output: TerminalText,
    },
    #[error("git on the host failed with {status:?}: {output:?}")]
    HostGit {
        status: ProcessStatus,
        output: TerminalText,
    },
    #[error("ref conflict at {host_ref}: {output:?}")]
    RefConflict {
        host_ref: String,
        output: TerminalText,
    },
    #[error("branch {branch:?} expected tip {expected:?} but found {found:?}")]
    TipMismatch {
        branch: BranchName,
        expected: CommitId,
        found: Option<CommitId>,
    },
    #[error("cancelled")]
    Cancelled,
}
