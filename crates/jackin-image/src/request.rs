// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    Agent, BuilderName, CommitId, DockerLayer, HostPath, Platform, RepoPath, RoleName, Sha256,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceFile {
    pub path: RepoPath,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HookFiles {
    pub setup: Option<SourceFile>,
    pub preflight: Option<SourceFile>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RoleRevision {
    Clean { commit: CommitId },
    Dirty { commit: CommitId, digest: Sha256 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rebuild {
    No,
    Yes,
}

impl From<bool> for Rebuild {
    fn from(flag: bool) -> Rebuild {
        if flag { Rebuild::Yes } else { Rebuild::No }
    }
}

#[derive(Clone, Debug)]
pub struct ImageRequest {
    pub role: RoleName,
    pub context: HostPath,
    pub context_files: Vec<RepoPath>,
    pub dockerfile: SourceFile,
    pub hooks: HookFiles,
    pub revision: RoleRevision,
    pub agent: Agent,
    pub platform: Platform,
    pub builder: BuilderName,
    pub docker: DockerLayer,
    pub rebuild: Rebuild,
}
