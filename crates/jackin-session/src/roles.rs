// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    FsError, HostPath, ManifestError, ProcessError, ProcessStatus, RepoPath, RoleName, TerminalText,
};
use jackin_image::DockerfileError;
use jackin_openshell::{ClosureViolation, PolicyError};
use thiserror::Error;

#[derive(Clone, Debug)]
pub struct RoleCatalog {}

#[derive(Clone, Debug)]
pub struct LoadedRole {}

#[derive(Debug, Error)]
pub enum RoleError {
    #[error("no role is selected and none is configured")]
    NoRole,
    #[error("unknown role {0:?}")]
    UnknownRole(RoleName),
    #[error("invalid role manifest at {path:?}: {source}")]
    Manifest {
        path: HostPath,
        source: ManifestError,
    },
    #[error("missing file {path:?}")]
    FileMissing { path: RepoPath },
    #[error("not a regular file: {path:?}")]
    FileNotRegular { path: RepoPath },
    #[error("path escapes the repository: {path:?}")]
    EscapesRepository { path: RepoPath },
    #[error(transparent)]
    Dockerfile(#[from] DockerfileError),
    #[error(transparent)]
    Policy(#[from] PolicyError),
    #[error("the role policy overlaps provider hosts: {0:?}")]
    ProviderHostClosure(Vec<ClosureViolation>),
    #[error("git failed with {status:?}: {output:?}")]
    Git {
        status: ProcessStatus,
        output: TerminalText,
    },
    #[error(transparent)]
    Fs(#[from] FsError),
    #[error(transparent)]
    Process(#[from] ProcessError),
}
