// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use thiserror::Error;

use crate::ids::{EnvName, ModelName, RoleName};
use crate::paths::RepoPath;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoleManifest {
    pub schema: SchemaVersion,
    pub role: RoleSection,
    pub codex: CodexSection,
    pub docker: DockerSection,
    pub hooks: HooksSection,
    pub env: BTreeMap<EnvName, EnvSpec>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SchemaVersion;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoleSection {
    pub name: RoleName,
    pub dockerfile: RepoPath,
    pub policy: RepoPath,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodexSection {
    pub model: Option<ModelName>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DockerSection {
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HooksSection {
    pub setup: Option<RepoPath>,
    pub preflight: Option<RepoPath>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvSpec {
    pub default: Option<String>,
    pub prompt: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HookKind {
    Setup,
    Preflight,
}

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("syntax error at line {line}, column {column}: {message}")]
    Syntax {
        line: usize,
        column: usize,
        message: String,
    },
    #[error("unknown field {key}")]
    UnknownField { key: String },
    #[error("missing field {key}")]
    MissingField { key: String },
    #[error("invalid value for {key}: {message}")]
    InvalidValue { key: String, message: String },
    #[error("unsupported schema version {found}")]
    UnsupportedSchema { found: i64 },
    #[error("reserved environment name {0:?}")]
    ReservedEnvName(EnvName),
}
