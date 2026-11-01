// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_core::{
    AccountName, AttemptId, FsError, HookKind, LaunchId, Phase, SandboxName, SupervisorReading,
    SupervisorState, TerminalText,
};
use jackin_image::{ImageError, ImageEvent};
use jackin_openshell::GatewayError;
use thiserror::Error;

use crate::accounts::AccountError;
use crate::credentials::CredentialError;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LaunchStep {
    Image,
    Provider,
    Sandbox,
    Ready,
    Credentials,
    Upload,
    Supervisor,
    Hooks,
    Start,
}

impl LaunchStep {
    pub const ALL: [LaunchStep; 9] = [
        LaunchStep::Image,
        LaunchStep::Provider,
        LaunchStep::Sandbox,
        LaunchStep::Ready,
        LaunchStep::Credentials,
        LaunchStep::Upload,
        LaunchStep::Supervisor,
        LaunchStep::Hooks,
        LaunchStep::Start,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LaunchWarning {
    DockerCapability,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SkipReason {
    ImageCached,
    ProviderPresent,
    SandboxExists,
    AlreadyUploaded,
    SetupAlreadyDone,
    AlreadyRunning,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StepResult {
    Done,
    Skipped(SkipReason),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaunchEvent {
    Warning(LaunchWarning),
    StepStarted(LaunchStep),
    StepFinished {
        step: LaunchStep,
        result: StepResult,
    },
    Image(ImageEvent),
    SandboxKnown(SandboxName),
}

#[derive(Clone, Debug)]
pub struct LaunchEvents {}

#[derive(Debug)]
pub struct LaunchFeed {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LineDelivery {
    Reliable,
    BestEffort,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttachTarget {
    pub sandbox: SandboxName,
    pub state: SupervisorState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestoreCandidate {
    pub launch_id: LaunchId,
    pub sandbox: Option<SandboxName>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaunchOutcome {
    Ready(AttachTarget),
    Cancelled(RestoreCandidate),
}

#[derive(Debug)]
pub struct LaunchFailure {
    pub launch_id: LaunchId,
    pub step: LaunchStep,
    pub sandbox: Option<SandboxName>,
    pub error: LaunchError,
}

#[derive(Debug, Error)]
pub enum LaunchError {
    #[error(transparent)]
    Image(#[from] ImageError),
    #[error(transparent)]
    Gateway(#[from] GatewayError),
    #[error(transparent)]
    Credential(#[from] CredentialError),
    #[error(transparent)]
    Fs(#[from] FsError),
    #[error("the provider for account {account:?} is unavailable")]
    ProviderUnavailable {
        account: AccountName,
        source: Box<AccountError>,
    },
    #[error("launch {launch:?} matches several sandboxes: {sandboxes:?}")]
    AmbiguousLaunch {
        launch: LaunchId,
        sandboxes: Vec<SandboxName>,
    },
    #[error("sandbox name {name:?} belongs to another launch {other:?}")]
    NameCollision {
        name: SandboxName,
        other: Option<LaunchId>,
    },
    #[error("the main process of {sandbox:?} has ended: {phase:?}")]
    MainProcessEnded { sandbox: SandboxName, phase: Phase },
    #[error("sandbox {sandbox:?} is suspended")]
    SandboxSuspended { sandbox: SandboxName },
    #[error("provisioning of {sandbox:?} failed after {attempts} attempts: {message:?}")]
    ProvisioningFailed {
        sandbox: SandboxName,
        attempts: u8,
        message: TerminalText,
    },
    #[error("all create attempts for {sandbox:?} are used")]
    AttemptsExhausted { sandbox: SandboxName },
    #[error("sandbox {sandbox:?} was {phase:?} after {after:?}")]
    ReadyTimeout {
        sandbox: SandboxName,
        phase: Phase,
        after: Duration,
    },
    #[error("the supervisor of {sandbox:?} did not report within {after:?}")]
    SupervisorTimeout {
        sandbox: SandboxName,
        after: Duration,
    },
    #[error("attempt {attempt:?} of {sandbox:?} did not start; last reading {last:?}")]
    AttemptTimeout {
        sandbox: SandboxName,
        attempt: AttemptId,
        last: SupervisorReading,
    },
    #[error("the {hook:?} hook exited with {exit_code}: {output:?}")]
    HookFailed {
        hook: HookKind,
        exit_code: i32,
        output: TerminalText,
    },
    #[error("the sandbox probe is invalid: {message}")]
    ProbeInvalid { message: String },
}
