// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::num::NonZeroU8;
use std::time::Duration;

use jackin_core::{
    AccountName, CodexAuthMode, ConfigError, DockerError, EnvFileError, EnvName, FileMode,
    PathError, SandboxPath, SelectionError,
};
use jackin_image::ImageError;
use jackin_openshell::PolicyError;
use thiserror::Error;

#[derive(Clone, Debug)]
pub struct LaunchRequest {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvQuestion {
    pub name: EnvName,
    pub prompt: Option<String>,
    pub default: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LaunchLimits {
    pub ready: Duration,
    pub supervisor: Duration,
    pub attempt: Duration,
    pub hook: Duration,
    pub upload: Duration,
    pub create_attempts: NonZeroU8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Prompting {
    Never,
    Interactive,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannedFile {
    pub path: SandboxPath,
    pub mode: FileMode,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannedFiles {
    pub codex_config: PlannedFile,
    pub env: PlannedFile,
}

#[derive(Clone, Debug)]
pub struct LaunchPlan {}

#[derive(Clone, Debug)]
pub struct SandboxDraft {}

#[derive(Debug, Error)]
pub enum PlanError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Selection(#[from] SelectionError),
    #[error(transparent)]
    Project(#[from] PathError),
    #[error(transparent)]
    Image(#[from] ImageError),
    #[error(transparent)]
    Policy(#[from] PolicyError),
    #[error(transparent)]
    Docker(#[from] DockerError),
    #[error("environment values are needed: {0:?}")]
    EnvNeeded(Vec<EnvQuestion>),
    #[error("environment names are not declared by the role: {0:?}")]
    EnvUndeclared(Vec<EnvName>),
    #[error(transparent)]
    EnvFile(#[from] EnvFileError),
    #[error("account {account:?} uses {mode:?}, which is not supported yet")]
    Unsupported {
        account: AccountName,
        mode: CodexAuthMode,
    },
}
