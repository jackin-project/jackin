// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod certs;
mod decide;
pub(crate) mod hooks;
pub(crate) mod probe;
mod run;
mod tracker;

use std::fmt;
use std::time::Duration;

use jackin_core::{
    AccountName, AttemptId, DockerLabel, EngineCert, EngineCertProblem, FsError, HookKind,
    HostPath, ImageLabel, LabelError, LaunchId, PathError, Phase, ProcessError, SandboxName,
    SupervisorReading, SupervisorState, TerminalText, plain, plain_duration, plain_list,
};
use jackin_image::{ImageError, ImageEvent};
use jackin_openshell::GatewayError;
use thiserror::Error;
use tokio::sync::mpsc;

use crate::accounts::AccountError;
use crate::credentials::CredentialError;
use crate::project::{HostGitError, ProjectStep, TreeMismatch, UnsupportedProject};

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

impl fmt::Display for LaunchStep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            LaunchStep::Image => "image",
            LaunchStep::Provider => "provider",
            LaunchStep::Sandbox => "sandbox",
            LaunchStep::Ready => "ready",
            LaunchStep::Credentials => "credentials",
            LaunchStep::Upload => "upload",
            LaunchStep::Supervisor => "supervisor",
            LaunchStep::Hooks => "hooks",
            LaunchStep::Start => "start",
        })
    }
}

fn holder_text(other: Option<&LaunchId>) -> String {
    other.map_or_else(
        || "a sandbox with no launch ID".to_owned(),
        |launch| format!("another launch {launch}"),
    )
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

pub const LAUNCH_EVENT_CAPACITY: usize = 64;

#[derive(Clone, Debug)]
pub struct LaunchEvents {
    sender: mpsc::Sender<LaunchEvent>,
    lines: LineDelivery,
}

#[derive(Debug)]
pub struct LaunchFeed {
    receiver: mpsc::Receiver<LaunchEvent>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LineDelivery {
    Reliable,
    BestEffort,
}

pub fn launch_events(lines: LineDelivery) -> (LaunchEvents, LaunchFeed) {
    let (sender, receiver) = mpsc::channel(LAUNCH_EVENT_CAPACITY);
    (LaunchEvents { sender, lines }, LaunchFeed { receiver })
}

impl LaunchEvents {
    pub async fn emit(&self, event: LaunchEvent) {
        let droppable = self.lines == LineDelivery::BestEffort
            && matches!(event, LaunchEvent::Image(ImageEvent::Line { .. }));
        if droppable {
            drop(self.sender.try_send(event));
        } else {
            drop(self.sender.send(event).await);
        }
    }
}

impl LaunchFeed {
    pub async fn recv(&mut self) -> Option<LaunchEvent> {
        self.receiver.recv().await
    }
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

#[derive(Debug, Error)]
#[error("launch {launch_id} failed at {step}")]
pub struct LaunchFailure {
    pub launch_id: LaunchId,
    pub step: LaunchStep,
    pub sandbox: Option<SandboxName>,
    #[source]
    pub error: Box<LaunchError>,
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
    #[error(transparent)]
    Process(#[from] ProcessError),
    #[error(transparent)]
    Path(#[from] PathError),
    #[error(transparent)]
    HostGit(#[from] HostGitError),
    #[error(transparent)]
    Unsupported(#[from] UnsupportedProject),
    #[error("{0}")]
    ProjectMismatch(Box<TreeMismatch>),
    #[error("project step {step} failed with exit code {exit_code}: {output}")]
    ProjectFailed {
        step: ProjectStep,
        exit_code: i32,
        output: TerminalText,
    },
    #[error("project step {step} produced unusable output: {}", plain(.message))]
    ProjectOutput { step: ProjectStep, message: String },
    #[error("the provider for account {account} is unavailable")]
    ProviderUnavailable {
        account: AccountName,
        source: Box<AccountError>,
    },
    #[error("launch {launch} matches several sandboxes: {}", plain_list(.sandboxes))]
    AmbiguousLaunch {
        launch: LaunchId,
        sandboxes: Vec<SandboxName>,
    },
    #[error("sandbox name {name} belongs to {}", holder_text(.other.as_ref()))]
    NameCollision {
        name: SandboxName,
        other: Option<LaunchId>,
    },
    #[error("the main process of {sandbox} has ended: {phase}")]
    MainProcessEnded { sandbox: SandboxName, phase: Phase },
    #[error("sandbox {sandbox} is suspended")]
    SandboxSuspended { sandbox: SandboxName },
    #[error("provisioning of {sandbox} failed after {attempts} attempts: {message}")]
    ProvisioningFailed {
        sandbox: SandboxName,
        attempts: u8,
        message: TerminalText,
    },
    #[error("the attempt IDs of {sandbox} are exhausted")]
    AttemptsExhausted { sandbox: SandboxName },
    #[error("sandbox {sandbox} was {phase} after {}", plain_duration(.after))]
    ReadyTimeout {
        sandbox: SandboxName,
        phase: Phase,
        after: Duration,
    },
    #[error("the supervisor of {sandbox} did not report within {}", plain_duration(.after))]
    SupervisorTimeout {
        sandbox: SandboxName,
        after: Duration,
    },
    #[error("attempt {attempt} of {sandbox} did not start; last reading {last}")]
    AttemptTimeout {
        sandbox: SandboxName,
        attempt: AttemptId,
        last: SupervisorReading,
    },
    #[error("the {hook} hook exited with {exit_code}: {output}")]
    HookFailed {
        hook: HookKind,
        exit_code: i32,
        output: TerminalText,
    },
    #[error("the engine certificate {} in {directory} is unusable: {problem}", file.file_name())]
    EngineCert {
        file: EngineCert,
        directory: HostPath,
        problem: EngineCertProblem,
    },
    #[error(
        "launch {launch} cannot continue sandbox {sandbox}: the sandbox has image {found_image} and Docker {found_docker}, this launch has image {wanted_image} and Docker {wanted_docker}; start a new launch with a new launch ID"
    )]
    StaleSandbox {
        launch: LaunchId,
        sandbox: SandboxName,
        found_image: ImageLabel,
        wanted_image: ImageLabel,
        found_docker: DockerLabel,
        wanted_docker: DockerLabel,
    },
    #[error("the labels of sandbox {sandbox} are unreadable: {error}")]
    SandboxLabels {
        sandbox: SandboxName,
        error: LabelError,
    },
    #[error("the sandbox probe is invalid: {}", plain(.message))]
    ProbeInvalid { message: String },
    #[error("the sandbox {sandbox} has no value for {}", plain(.key))]
    PlaceholderMissing { sandbox: SandboxName, key: String },
}
