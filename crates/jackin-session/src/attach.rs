// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fmt;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use jackin_core::{
    Agent, AttemptId, EnvFileError, EnvName, FileLock, FsError, GatewayName, HookKind, HostPath,
    LaunchId, PathError, Phase, SandboxLabels, SandboxName, StartRequest, SupervisorReading,
    SupervisorState, TerminalText, sandbox_layout,
};
use jackin_openshell::{
    Gateway, GatewayError, GatewayOp, InputMode, Missing, RepaintPolicy, SandboxFile, SandboxView,
    SessionClosed, TermSize, TerminalEvent, TerminalKind, TerminalRequest, TerminalSession,
};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::launch::hooks::{self, HookRun};
use crate::launch::{AttachTarget, LaunchEvent, LaunchEvents, LaunchStep, StepResult};
use crate::multi_session::{InstanceStop, InstanceStopError, instance_argv};
use crate::service::{POLL, Session, read_until_cancelled};
use crate::state::{EnvReadError, env_file, run_state};

pub const REPAINT_NUDGE: Duration = Duration::from_millis(600);
const REQUEST_MODE: jackin_core::FileMode = jackin_core::FileMode(0o600);
const ESC: u8 = 0x1b;
const BEL: u8 = 0x07;
const HELD_MIN: usize = 2;
const HELD_MAX: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttachRequest {
    pub sandbox: SandboxName,
    pub size: TermSize,
    pub input: InputMode,
    pub agent: Option<Agent>,
}

#[derive(Debug)]
pub struct AttachedTerminal {
    session: TerminalSession,
    gateway: Arc<dyn Gateway>,
    sandbox: SandboxName,
    supervisor: SupervisorReading,
    held: Vec<u8>,
}

#[derive(Clone, Debug)]
struct SupervisorProbe {
    gateway: Arc<dyn Gateway>,
    sandbox: SandboxName,
}

impl SupervisorProbe {
    async fn read(&self) -> SupervisorReading {
        run_state(self.gateway.as_ref(), &self.sandbox)
            .await
            .unwrap_or(SupervisorReading::Unreadable)
    }
}

pub struct SupervisorReads {
    probe: Option<SupervisorProbe>,
    pending: Option<Pin<Box<dyn Future<Output = SupervisorReading> + Send>>>,
}

impl fmt::Debug for SupervisorReads {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SupervisorReads").finish_non_exhaustive()
    }
}

impl SupervisorReads {
    pub fn idle() -> SupervisorReads {
        SupervisorReads {
            probe: None,
            pending: None,
        }
    }

    pub fn begin(&mut self) {
        if self.pending.is_none()
            && let Some(probe) = self.probe.clone()
        {
            self.pending = Some(Box::pin(async move { probe.read().await }));
        }
    }

    pub async fn finished(&mut self) -> SupervisorReading {
        match self.pending.as_mut() {
            Some(read) => {
                let reading = read.await;
                self.pending = None;
                reading
            }
            None => std::future::pending().await,
        }
    }
}

#[derive(Debug)]
pub enum EnvFileProblem {
    NoLaunchLabel,
    Missing,
    Invalid(EnvFileError),
}

impl fmt::Display for EnvFileProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EnvFileProblem::NoLaunchLabel => f.write_str("no launch label"),
            EnvFileProblem::Missing => f.write_str("missing"),
            EnvFileProblem::Invalid(error) => write!(f, "invalid: {error}"),
        }
    }
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
    #[error("sandbox {0} was not found")]
    NotFound(SandboxName),
    #[error("sandbox {0} exists but is not a jackin sandbox")]
    NotManaged(SandboxName),
    #[error("the main process of {sandbox} has ended: {phase}")]
    MainProcessEnded { sandbox: SandboxName, phase: Phase },
    #[error("the env file of {sandbox} is unusable: {problem}")]
    EnvFile {
        sandbox: SandboxName,
        problem: EnvFileProblem,
    },
    #[error("secondary attach for {} in sandbox {sandbox} on gateway {gateway}", .agent.as_str())]
    Secondary {
        sandbox: SandboxName,
        agent: Agent,
        gateway: GatewayName,
        #[source]
        failure: SecondaryAttachError,
    },
}

#[derive(Debug, Error)]
pub enum SecondaryAttachError {
    #[error("cancelled after stopping the previous instance")]
    CancelledAfterStop,
    #[error("could not confirm whether the previous instance stopped")]
    StopUnconfirmed(#[source] Box<GatewayError>),
    #[error("the previous instance stopped, but its instance record could not be cleared")]
    RecordCleanup(#[source] Box<GatewayError>),
    #[error("could not confirm whether the replacement terminal opened")]
    OpenUnconfirmed(#[source] Box<GatewayError>),
    #[error("the previous instance stopped, but the replacement terminal could not be opened")]
    OpenFailedAfterStop(#[source] Box<GatewayError>),
}

#[derive(Debug, Error)]
pub enum RestartError {
    #[error(transparent)]
    Gateway(#[from] GatewayError),
    #[error("sandbox {0} exists but is not a jackin sandbox")]
    NotManaged(SandboxName),
    #[error("the main process of {sandbox} has ended: {phase}")]
    MainProcessEnded { sandbox: SandboxName, phase: Phase },
    #[error("the agent in {sandbox} is already running as attempt {attempt}")]
    AlreadyRunning {
        sandbox: SandboxName,
        attempt: AttemptId,
    },
    #[error("no attempt IDs are left for {sandbox}")]
    AttemptsExhausted { sandbox: SandboxName },
    #[error("the env file of {sandbox} is unusable: {problem}")]
    EnvFile {
        sandbox: SandboxName,
        problem: EnvFileProblem,
    },
    #[error("the supervisor state of {sandbox} is unreadable")]
    StateUnreadable { sandbox: SandboxName },
    #[error("another restart of {sandbox} is in progress")]
    Busy { sandbox: SandboxName },
    #[error(transparent)]
    Fs(#[from] FsError),
    #[error(transparent)]
    Path(#[from] PathError),
    #[error("the {hook} hook exited with {exit_code}: {output}")]
    HookFailed {
        hook: HookKind,
        exit_code: i32,
        output: TerminalText,
    },
    #[error("attempt {attempt} of {sandbox} did not start")]
    AttemptTimeout {
        sandbox: SandboxName,
        attempt: AttemptId,
    },
}

impl AttachRequest {
    pub fn owner(target: AttachTarget, size: TermSize) -> AttachRequest {
        AttachRequest {
            sandbox: target.sandbox,
            size,
            input: InputMode::Owner,
            agent: None,
        }
    }

    pub fn viewer(sandbox: SandboxName, size: TermSize) -> AttachRequest {
        AttachRequest {
            sandbox,
            size,
            input: InputMode::Viewer,
            agent: None,
        }
    }
}

fn digits(bytes: &[u8], from: usize) -> usize {
    bytes[from..]
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count()
}

fn csi_reply(bytes: &[u8]) -> Option<usize> {
    let body = bytes.strip_prefix(&[ESC, b'['])?;
    if matches!(body.first(), Some(b'?' | b'>')) {
        let params = body[1..]
            .iter()
            .take_while(|byte| byte.is_ascii_digit() || matches!(byte, b';' | b':'))
            .count();
        return (body.get(1 + params) == Some(&b'c')).then_some(2 + 1 + params + 1);
    }
    let row = digits(body, 0);
    if row == 0 || body.get(row) != Some(&b';') {
        return None;
    }
    let col = digits(body, row + 1);
    (col > 0 && body.get(row + 1 + col) == Some(&b'R')).then_some(2 + row + 1 + col + 1)
}

fn osc_reply(bytes: &[u8]) -> Option<usize> {
    let body = bytes.strip_prefix(&[ESC, b']'])?;
    if !(body.starts_with(b"10;") || body.starts_with(b"11;")) {
        return None;
    }
    let end = body.iter().position(|byte| matches!(byte, &BEL | &ESC))?;
    match body[end] {
        BEL => Some(2 + end + 1),
        _ => (body.get(end + 1) == Some(&b'\\')).then_some(2 + end + 2),
    }
}

fn partial_csi(body: &[u8]) -> bool {
    if body.is_empty() {
        return true;
    }
    if matches!(body.first(), Some(b'?' | b'>')) {
        return body[1..]
            .iter()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b';' | b':'));
    }
    let row = digits(body, 0);
    row > 0
        && match body.get(row) {
            None => true,
            Some(b';') => digits(body, row + 1) == body.len() - row - 1,
            Some(_) => false,
        }
}

fn partial_osc(body: &[u8]) -> bool {
    if body.len() < 3 {
        return b"10;".starts_with(body) || b"11;".starts_with(body);
    }
    (body.starts_with(b"10;") || body.starts_with(b"11;"))
        && !body.contains(&BEL)
        && body
            .iter()
            .position(|byte| *byte == ESC)
            .is_none_or(|at| at + 1 == body.len())
}

fn partial_reply(bytes: &[u8]) -> bool {
    if bytes.len() < HELD_MIN || bytes.len() > HELD_MAX {
        return false;
    }
    if let Some(body) = bytes.strip_prefix(&[ESC, b'[']) {
        partial_csi(body)
    } else if let Some(body) = bytes.strip_prefix(&[ESC, b']']) {
        partial_osc(body)
    } else {
        false
    }
}

fn held_from(bytes: &[u8]) -> Option<usize> {
    (bytes.len().saturating_sub(HELD_MAX)..bytes.len())
        .find(|at| bytes[*at] == ESC && partial_reply(&bytes[*at..]))
}

pub(crate) fn split_replies(chunk: &[u8]) -> (Vec<u8>, bool) {
    let mut plain = Vec::with_capacity(chunk.len());
    let mut replies = false;
    let mut at = 0;
    while at < chunk.len() {
        if chunk[at] == ESC
            && let Some(length) = csi_reply(&chunk[at..]).or_else(|| osc_reply(&chunk[at..]))
        {
            replies = true;
            at += length;
        } else {
            plain.push(chunk[at]);
            at += 1;
        }
    }
    (plain, replies)
}

impl AttachedTerminal {
    pub fn supervisor_at_attach(&self) -> &SupervisorReading {
        &self.supervisor
    }

    pub async fn input(&mut self, typed: Vec<u8>) -> Result<(), SessionClosed> {
        let mut bytes = std::mem::take(&mut self.held);
        bytes.extend(typed);
        if let Some(at) = held_from(&bytes)
            && !self.running().await
        {
            self.held = bytes.split_off(at);
        }
        let (plain, replies) = split_replies(&bytes);
        let send = if !replies || self.running().await {
            bytes
        } else {
            plain
        };
        if send.is_empty() {
            return Ok(());
        }
        self.session.input(send).await
    }

    pub async fn reply(&mut self, bytes: Vec<u8>) -> Result<(), SessionClosed> {
        if bytes.is_empty() || !self.running().await {
            return Ok(());
        }
        self.session.input(bytes).await
    }

    pub async fn resize(&self, size: TermSize) -> Result<(), SessionClosed> {
        self.session.resize(size).await
    }

    pub async fn detach(&self) -> Result<(), SessionClosed> {
        self.session.detach().await
    }

    pub async fn next_event(&mut self) -> Option<TerminalEvent> {
        self.session.next_event().await
    }

    pub fn reads(&self) -> SupervisorReads {
        SupervisorReads {
            probe: Some(self.probe()),
            pending: None,
        }
    }

    fn probe(&self) -> SupervisorProbe {
        SupervisorProbe {
            gateway: Arc::clone(&self.gateway),
            sandbox: self.sandbox.clone(),
        }
    }

    pub async fn supervisor(&self) -> SupervisorReading {
        self.probe().read().await
    }

    async fn running(&self) -> bool {
        matches!(
            self.supervisor().await,
            SupervisorReading::Reported(SupervisorState::Running { .. })
        )
    }
}

enum EnvFailure {
    Gateway(GatewayError),
    File(EnvFileProblem),
}

impl From<GatewayError> for EnvFailure {
    fn from(error: GatewayError) -> EnvFailure {
        EnvFailure::Gateway(error)
    }
}

impl From<EnvReadError> for EnvFailure {
    fn from(error: EnvReadError) -> EnvFailure {
        match error {
            EnvReadError::Gateway(error) => EnvFailure::Gateway(error),
            EnvReadError::Missing => EnvFailure::File(EnvFileProblem::Missing),
            EnvReadError::Invalid(error) => EnvFailure::File(EnvFileProblem::Invalid(error)),
        }
    }
}

impl AttachError {
    fn env(sandbox: &SandboxName, failure: EnvFailure) -> AttachError {
        match failure {
            EnvFailure::Gateway(error) => AttachError::Gateway(error),
            EnvFailure::File(problem) => AttachError::EnvFile {
                sandbox: sandbox.clone(),
                problem,
            },
        }
    }
}

impl RestartError {
    fn env(sandbox: &SandboxName, failure: EnvFailure) -> RestartError {
        match failure {
            EnvFailure::Gateway(error) => RestartError::Gateway(error),
            EnvFailure::File(problem) => RestartError::EnvFile {
                sandbox: sandbox.clone(),
                problem,
            },
        }
    }
}

fn launch_of(labels: &jackin_core::Labels) -> Result<LaunchId, EnvFailure> {
    SandboxLabels::from_labels(labels)
        .map(|labels| labels.planned.launch)
        .map_err(|_| EnvFailure::File(EnvFileProblem::NoLaunchLabel))
}

fn not_found(sandbox: &SandboxName) -> GatewayError {
    GatewayError::NotFound {
        what: Missing::Sandbox(sandbox.clone()),
    }
}

impl Session {
    async fn open_until(
        &self,
        cancel: &CancellationToken,
        request: &TerminalRequest,
    ) -> Result<TerminalSession, GatewayError> {
        read_until_cancelled(
            cancel,
            GatewayOp::Terminal,
            self.deps.gateway.open_terminal(request),
        )
        .await
    }

    pub async fn attach(
        &self,
        request: AttachRequest,
        cancel: &CancellationToken,
    ) -> Result<AttachedTerminal, AttachError> {
        let sandbox = &request.sandbox;
        let view = self.locate(sandbox, cancel).await?.require(
            || AttachError::NotFound(sandbox.clone()),
            || AttachError::NotManaged(sandbox.clone()),
        )?;
        require_ready::<AttachError>(view.phase, sandbox)?;
        let names = self
            .agent_names_of(sandbox, &view, cancel)
            .await
            .map_err(AttachError::Gateway)?;
        if let Some(agent) = request.agent {
            if !names.iter().any(|name| name == agent.as_str()) {
                return Err(AttachError::NotFound(sandbox.clone()));
            }
            let is_lead = names.first().is_some_and(|lead| lead == agent.as_str());
            if !is_lead {
                return self
                    .attach_secondary(&view, agent, request.size, cancel)
                    .await;
            }
        }
        let session = self
            .deps
            .gateway
            .open_terminal(&TerminalRequest {
                sandbox: sandbox.clone(),
                kind: TerminalKind::MainProcess {
                    input: request.input,
                    repaint: RepaintPolicy::NudgeAfter(REPAINT_NUDGE),
                },
                size: request.size,
            })
            .await?;
        let supervisor = run_state(self.deps.gateway.as_ref(), sandbox).await?;
        Ok(AttachedTerminal {
            session,
            gateway: Arc::clone(&self.deps.gateway),
            sandbox: sandbox.clone(),
            supervisor,
            held: Vec::new(),
        })
    }

    async fn attach_secondary(
        &self,
        view: &SandboxView,
        agent: Agent,
        size: TermSize,
        cancel: &CancellationToken,
    ) -> Result<AttachedTerminal, AttachError> {
        let sandbox = &view.name;
        let gateway = self.deps.gateway.as_ref();
        let mut env = self.attach_env(view, cancel).await?;
        env.insert(
            EnvName::parse("HOME").map_err(|_| AttachError::NotFound(sandbox.clone()))?,
            format!("/sandbox/.jackin/homes/{}", agent.as_str()),
        );
        let request = TerminalRequest {
            sandbox: sandbox.clone(),
            kind: TerminalKind::Exec {
                argv: instance_argv(agent, &agent.spec().launch_argv()),
                workdir: jackin_core::SandboxPath::parse(&format!(
                    "/sandbox/work/{}/project",
                    agent.as_str()
                ))
                .map_err(|_| AttachError::NotFound(sandbox.clone()))?,
                env,
            },
            size,
        };
        let supervisor =
            read_until_cancelled(cancel, GatewayOp::ReadFiles, run_state(gateway, sandbox)).await?;
        let failed = |failure| AttachError::Secondary {
            sandbox: sandbox.clone(),
            agent,
            gateway: self.gateway_name().clone(),
            failure,
        };
        let stopped =
            self.kill_instance(view, agent, cancel)
                .await
                .map_err(|error| match error {
                    InstanceStopError::Before(cause) => AttachError::Gateway(cause),
                    InstanceStopError::Unconfirmed(cause) => {
                        failed(SecondaryAttachError::StopUnconfirmed(Box::new(cause)))
                    }
                    InstanceStopError::Cleanup(cause) => {
                        failed(SecondaryAttachError::RecordCleanup(Box::new(cause)))
                    }
                })?;
        let check_cancel = || {
            if cancel.is_cancelled() {
                return Err(match stopped {
                    InstanceStop::Unchanged => GatewayError::Cancelled {
                        op: GatewayOp::Terminal,
                        group: None,
                    }
                    .into(),
                    InstanceStop::Stopped => failed(SecondaryAttachError::CancelledAfterStop),
                });
            }
            Ok(())
        };
        let open_failed = |cause| match stopped {
            InstanceStop::Unchanged => AttachError::Gateway(cause),
            InstanceStop::Stopped => {
                failed(SecondaryAttachError::OpenFailedAfterStop(Box::new(cause)))
            }
        };
        check_cancel()?;
        if let Err(cause) = self.same_incarnation(sandbox, view, cancel).await {
            check_cancel()?;
            return Err(open_failed(cause));
        }
        check_cancel()?;
        let session = self
            .instance_deadline(GatewayOp::Terminal, gateway.open_terminal(&request))
            .await
            .map_err(|cause| {
                if cause.outcome_unknown() {
                    return failed(SecondaryAttachError::OpenUnconfirmed(Box::new(cause)));
                }
                open_failed(cause)
            })?;
        check_cancel()?;
        Ok(AttachedTerminal {
            session,
            gateway: Arc::clone(&self.deps.gateway),
            sandbox: sandbox.clone(),
            supervisor,
            held: Vec::new(),
        })
    }

    async fn attach_env(
        &self,
        view: &SandboxView,
        cancel: &CancellationToken,
    ) -> Result<BTreeMap<EnvName, String>, AttachError> {
        let values = async {
            let launch = launch_of(&view.labels)?;
            Ok(env_file(self.deps.gateway.as_ref(), &view.name, &launch).await?)
        };
        read_until_cancelled(cancel, GatewayOp::ReadFiles, values)
            .await
            .map_err(|failure| AttachError::env(&view.name, failure))
    }

    pub async fn open_shell(
        &self,
        request: ShellRequest,
        cancel: &CancellationToken,
    ) -> Result<TerminalSession, AttachError> {
        let sandbox = &request.sandbox;
        let view = self.locate(sandbox, cancel).await?.require(
            || AttachError::NotFound(sandbox.clone()),
            || AttachError::NotManaged(sandbox.clone()),
        )?;
        require_ready::<AttachError>(view.phase, sandbox)?;
        let gateway = self.deps.gateway.as_ref();
        let values = async {
            let launch = launch_of(&view.labels)?;
            Ok::<_, EnvFailure>(env_file(gateway, sandbox, &launch).await?)
        };
        let values = tokio::select! {
            biased;
            () = cancel.cancelled() => {
                return Err(GatewayError::Cancelled { op: GatewayOp::ReadFiles, group: None }.into());
            }
            values = values => values.map_err(|failure| AttachError::env(sandbox, failure))?,
        };
        if cancel.is_cancelled() {
            return Err(GatewayError::Cancelled {
                op: GatewayOp::Terminal,
                group: None,
            }
            .into());
        }
        Ok(self
            .open_until(
                cancel,
                &TerminalRequest {
                    sandbox: sandbox.clone(),
                    kind: TerminalKind::Shell {
                        workdir: sandbox_layout::PROJECT.clone(),
                        env: values,
                    },
                    size: request.size,
                },
            )
            .await?)
    }

    pub async fn supervisor_state(
        &self,
        sandbox: &SandboxName,
    ) -> Result<SupervisorReading, GatewayError> {
        run_state(self.deps.gateway.as_ref(), sandbox).await
    }

    pub async fn restart_agent(
        &self,
        sandbox: &SandboxName,
        events: LaunchEvents,
        cancel: CancellationToken,
    ) -> Result<AttachTarget, RestartError> {
        let gateway = self.deps.gateway.as_ref();
        let view = self.locate(sandbox, &cancel).await?.require(
            || RestartError::Gateway(not_found(sandbox)),
            || RestartError::NotManaged(sandbox.clone()),
        )?;
        require_ready::<RestartError>(view.phase, sandbox)?;
        let values = async {
            let launch = launch_of(&view.labels)?;
            Ok::<_, EnvFailure>(env_file(gateway, sandbox, &launch).await?)
        }
        .await
        .map_err(|failure| RestartError::env(sandbox, failure))?;
        let _held = self.restart_lock(sandbox).await?;
        self.restartable(sandbox).await?;
        self.restart_preflight(sandbox, &values, &events, &cancel)
            .await?;
        let attempt = self.restartable(sandbox).await?;
        self.request_attempt(sandbox, attempt, &events, &cancel)
            .await
    }

    async fn restart_lock(&self, sandbox: &SandboxName) -> Result<FileLock, RestartError> {
        let fs = self.deps.host.fs.as_ref();
        fs.create_dir_all(&self.deps.cache_dir).await?;
        let path = HostPath::resolve(
            Path::new(&format!("restart-{sandbox}.lock")),
            &self.deps.cache_dir,
        )?;
        match fs.lock(&path).await {
            Err(FsError::LockHeld(_)) => Err(RestartError::Busy {
                sandbox: sandbox.clone(),
            }),
            other => Ok(other?),
        }
    }

    async fn restartable(&self, sandbox: &SandboxName) -> Result<AttemptId, RestartError> {
        let state = match run_state(self.deps.gateway.as_ref(), sandbox).await? {
            SupervisorReading::Reported(state) => state,
            SupervisorReading::Unreadable | SupervisorReading::Invalid(_) => {
                return Err(RestartError::StateUnreadable {
                    sandbox: sandbox.clone(),
                });
            }
        };
        if let SupervisorState::Running { attempt } = state {
            return Err(RestartError::AlreadyRunning {
                sandbox: sandbox.clone(),
                attempt,
            });
        }
        state
            .next_attempt()
            .ok_or_else(|| RestartError::AttemptsExhausted {
                sandbox: sandbox.clone(),
            })
    }

    async fn restart_preflight(
        &self,
        sandbox: &SandboxName,
        values: &BTreeMap<EnvName, String>,
        events: &LaunchEvents,
        cancel: &CancellationToken,
    ) -> Result<(), RestartError> {
        let gateway = self.deps.gateway.as_ref();
        events
            .emit(LaunchEvent::StepStarted(LaunchStep::Hooks))
            .await;
        let hook = sandbox_layout::hook_path(HookKind::Preflight);
        let mut found = gateway
            .read_files(sandbox, std::slice::from_ref(&hook))
            .await?;
        let present = found.pop().flatten().is_some_and(|bytes| !bytes.is_empty());
        if present {
            hooks::run(
                HookRun {
                    gateway,
                    sandbox,
                    kind: HookKind::Preflight,
                    env: values,
                    touch_on_success: None,
                    timeout: self.deps.limits.hook,
                },
                cancel,
            )
            .await?;
        }
        events
            .emit(LaunchEvent::StepFinished {
                step: LaunchStep::Hooks,
                result: StepResult::Done,
            })
            .await;
        Ok(())
    }

    async fn request_attempt(
        &self,
        sandbox: &SandboxName,
        attempt: AttemptId,
        events: &LaunchEvents,
        cancel: &CancellationToken,
    ) -> Result<AttachTarget, RestartError> {
        let gateway = self.deps.gateway.as_ref();
        events
            .emit(LaunchEvent::StepStarted(LaunchStep::Start))
            .await;
        if cancel.is_cancelled() {
            return Err(GatewayError::Cancelled {
                op: GatewayOp::WriteFile,
                group: None,
            }
            .into());
        }
        gateway
            .write_file(
                sandbox,
                &SandboxFile {
                    path: sandbox_layout::RUN_START.clone(),
                    mode: REQUEST_MODE,
                    content: jackin_core::SecretBytes::new(
                        StartRequest(attempt).render().into_bytes(),
                    ),
                },
            )
            .await?;
        let limit = self.deps.limits.attempt;
        let mut waited = Duration::ZERO;
        loop {
            let seen_state = run_state(gateway, sandbox).await?;
            if let SupervisorReading::Reported(
                state @ (SupervisorState::Running { attempt: seen }
                | SupervisorState::Exited { attempt: seen, .. }),
            ) = seen_state
                && seen == attempt
            {
                events
                    .emit(LaunchEvent::StepFinished {
                        step: LaunchStep::Start,
                        result: StepResult::Done,
                    })
                    .await;
                return Ok(AttachTarget {
                    sandbox: sandbox.clone(),
                    state,
                });
            }
            if waited >= limit {
                return Err(RestartError::AttemptTimeout {
                    sandbox: sandbox.clone(),
                    attempt,
                });
            }
            tokio::select! {
                biased;
                () = cancel.cancelled() => {
                    return Err(GatewayError::Cancelled { op: GatewayOp::ReadFiles, group: None }.into());
                }
                () = self.deps.host.clock.sleep(POLL) => {}
            }
            waited += POLL;
        }
    }
}

fn require_ready<E>(phase: Phase, sandbox: &SandboxName) -> Result<(), E>
where
    E: From<GatewayError> + EndedMain,
{
    if phase.main_process_ended() {
        Err(E::ended(sandbox.clone(), phase))
    } else if phase == Phase::Ready {
        Ok(())
    } else {
        Err(GatewayError::NotReady { phase }.into())
    }
}

trait EndedMain {
    fn ended(sandbox: SandboxName, phase: Phase) -> Self;
}

impl EndedMain for AttachError {
    fn ended(sandbox: SandboxName, phase: Phase) -> AttachError {
        AttachError::MainProcessEnded { sandbox, phase }
    }
}

impl EndedMain for RestartError {
    fn ended(sandbox: SandboxName, phase: Phase) -> RestartError {
        RestartError::MainProcessEnded { sandbox, phase }
    }
}
