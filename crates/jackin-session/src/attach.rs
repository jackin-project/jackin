// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{AttemptId, EnvFileError, HookKind, Phase, SandboxName, TerminalText};
use jackin_openshell::{GatewayError, InputMode, TermSize};
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttachRequest {
    pub sandbox: SandboxName,
    pub size: TermSize,
    pub input: InputMode,
}

#[derive(Debug)]
pub struct AttachedTerminal {}

#[derive(Debug)]
pub enum EnvFileProblem {
    NoLaunchLabel,
    Missing,
    Invalid(EnvFileError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShellRequest {
    pub sandbox: SandboxName,
    pub size: TermSize,
}

#[derive(Debug, Error)]
pub enum AttachError {
    #[error(transparent)]
    Gateway(#[from] GatewayError),
    #[error("sandbox {0:?} was not found")]
    NotFound(SandboxName),
    #[error("the main process of {sandbox:?} has ended: {phase:?}")]
    MainProcessEnded { sandbox: SandboxName, phase: Phase },
    #[error("the env file of {sandbox:?} is unusable: {problem:?}")]
    EnvFile {
        sandbox: SandboxName,
        problem: EnvFileProblem,
    },
}

#[derive(Debug, Error)]
pub enum RestartError {
    #[error(transparent)]
    Gateway(#[from] GatewayError),
    #[error("the main process of {sandbox:?} has ended: {phase:?}")]
    MainProcessEnded { sandbox: SandboxName, phase: Phase },
    #[error("the agent in {sandbox:?} is already running as attempt {attempt:?}")]
    AlreadyRunning {
        sandbox: SandboxName,
        attempt: AttemptId,
    },
    #[error("no attempt IDs are left for {sandbox:?}")]
    AttemptsExhausted { sandbox: SandboxName },
    #[error("the env file of {sandbox:?} is unusable: {problem:?}")]
    EnvFile {
        sandbox: SandboxName,
        problem: EnvFileProblem,
    },
    #[error("the supervisor state of {sandbox:?} is unreadable")]
    StateUnreadable { sandbox: SandboxName },
    #[error("the {hook:?} hook exited with {exit_code}: {output:?}")]
    HookFailed {
        hook: HookKind,
        exit_code: i32,
        output: TerminalText,
    },
    #[error("attempt {attempt:?} of {sandbox:?} did not start")]
    AttemptTimeout {
        sandbox: SandboxName,
        attempt: AttemptId,
    },
}
