// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    Agent, BuilderName, CommitId, DockerLayer, Platform, RepoPath, RoleName, Sha256,
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
pub struct RoleRevision {
    pub commit: CommitId,
    pub digest: Sha256,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContextEntry {
    File {
        path: RepoPath,
        bytes: Vec<u8>,
        executable: bool,
    },
    Link {
        path: RepoPath,
        target: String,
    },
}

impl ContextEntry {
    pub fn path(&self) -> &RepoPath {
        match self {
            ContextEntry::File { path, .. } | ContextEntry::Link { path, .. } => path,
        }
    }
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
    pub context_files: Vec<ContextEntry>,
    pub dockerfile: SourceFile,
    pub hooks: HookFiles,
    pub revision: RoleRevision,
    pub agent: Agent,
    pub platform: Platform,
    pub builder: BuilderName,
    pub docker: DockerLayer,
    pub rebuild: Rebuild,
}
