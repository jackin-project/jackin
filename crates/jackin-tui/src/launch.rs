// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};

use jackin_console::backend::{Backend, OnePassword};
use jackin_console::domain::agent::Agent;
use jackin_console::domain::docker::DockerCapability;
use jackin_console::domain::instance::Instance;
use jackin_console::domain::launch::{LaunchContent, LaunchFailure, NextStep, RefusalReason};
use jackin_console::launch::{
    EnvAnswer, EnvQuestion, FailedLaunch, LaunchEvent, LaunchFeed, LaunchTarget, ReadyInstance,
    TypedKey,
};
use jackin_console::world::{Msg, World};
use jackin_core::{
    AccountName, Config, ConfigEdit, ConfigError, EntropyError, EnvName, GatewayName, HostPath,
    IdError, LaunchId, PathError, ProjectRoot, RoleName, SandboxName, Workspace, WorkspaceName,
};
use jackin_image::ImageEvent;
use jackin_openshell::GatewayError;
use jackin_session::{
    AccountError, LaunchError, LaunchEvent as SessionEvent, LaunchFailure as SessionFailure,
    LaunchNextStep, LaunchOutcome, LaunchPlan, LaunchRequest, LaunchStep, LaunchWarning,
    LineDelivery, PlanError, Prompting, RestoreCandidate, RoleError, Session, StepResult,
    WorkspaceLookup, adhoc_workspace_name, error_chain, launch_events, lookup_workspace,
    plan_launch,
};
use junie_tui::widgets::steps::StepState;
use thiserror::Error;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio_util::sync::CancellationToken;

use crate::backend::JackinBackend;
use crate::crashed::STOPPED;
use crate::fleet::ObservedSandbox;
use crate::project::{Origins, docker_risks};
use crate::sessions::Access;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Stage {
    Image,
    Provider,
    Sandbox,
    Credentials,
    Upload,
    Hooks,
    Codex,
}

impl Stage {
    fn index(self) -> usize {
        self as usize
    }

    fn last(self) -> LaunchStep {
        match self {
            Stage::Image => LaunchStep::Image,
            Stage::Provider => LaunchStep::Provider,
            Stage::Sandbox => LaunchStep::Ready,
            Stage::Credentials => LaunchStep::Credentials,
            Stage::Upload => LaunchStep::Upload,
            Stage::Hooks => LaunchStep::Hooks,
            Stage::Codex => LaunchStep::Start,
        }
    }
}

fn stage_of(step: LaunchStep) -> Stage {
    match step {
        LaunchStep::Image => Stage::Image,
        LaunchStep::Provider => Stage::Provider,
        LaunchStep::Sandbox | LaunchStep::Ready => Stage::Sandbox,
        LaunchStep::Credentials => Stage::Credentials,
        LaunchStep::Upload => Stage::Upload,
        LaunchStep::Supervisor | LaunchStep::Hooks => Stage::Hooks,
        LaunchStep::Start => Stage::Codex,
    }
}

fn plan_stage(error: &PlanError) -> Stage {
    match error {
        PlanError::Selection(_) | PlanError::Unsupported { .. } | PlanError::EnvFile(_) => {
            Stage::Credentials
        }
        PlanError::Docker(_) | PlanError::Policy(_) | PlanError::Closure(_) => Stage::Sandbox,
        PlanError::Config(_)
        | PlanError::Project(_)
        | PlanError::Image(_)
        | PlanError::EnvNeeded(_)
        | PlanError::EnvUndeclared(_)
        | PlanError::CodexConfig(_)
        | PlanError::RoleMismatch { .. }
        | PlanError::RoleNotAllowed { .. } => Stage::Image,
    }
}

fn split(text: &str) -> (String, Vec<String>) {
    let mut lines = text.lines();
    let first = lines.next().unwrap_or_default().to_owned();
    (first, lines.map(str::to_owned).collect())
}

fn plain_failure(stage: Stage, error: &dyn Error) -> LaunchFailure {
    let (summary, detail) = split(&error_chain(error));
    LaunchFailure {
        stage: stage.index(),
        summary,
        next_step: NextStep::None,
        detail,
    }
}

pub(crate) fn gateway_failure(base: &LaunchFailure, error: &GatewayError) -> LaunchFailure {
    LaunchFailure {
        detail: vec![error.to_string()],
        ..base.clone()
    }
}

fn session_failure(failure: &SessionFailure) -> LaunchFailure {
    let (summary, detail) = split(&failure.error.text());
    LaunchFailure {
        stage: stage_of(failure.step).index(),
        summary,
        next_step: next_step(failure),
        detail,
    }
}

fn next_step(failure: &SessionFailure) -> NextStep {
    let credential = match &*failure.error {
        LaunchError::Credential(_) => true,
        LaunchError::ProviderUnavailable { source, .. } => {
            matches!(**source, AccountError::Credential(_))
        }
        _ => false,
    };
    if credential {
        return NextStep::FixCredential;
    }
    match failure.next_step() {
        Some(LaunchNextStep::Retry { sandbox, launch }) => NextStep::Retry {
            sandbox: sandbox.to_string(),
            launch: launch.to_string(),
        },
        None => NextStep::None,
    }
}

#[derive(Debug, Error)]
enum Unlaunchable {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Id(#[from] IdError),
    #[error(transparent)]
    Path(#[from] PathError),
    #[error("the workspace is no longer in the config")]
    WorkspaceGone,
}

#[derive(Debug)]
enum Stop {
    Refused(RefusalReason),
    Failed(Box<LaunchFailure>),
    Cancelled,
}

impl Stop {
    fn failed(failure: LaunchFailure) -> Stop {
        Stop::Failed(Box::new(failure))
    }

    fn at(stage: Stage, error: &dyn Error) -> Stop {
        Stop::failed(plain_failure(stage, error))
    }
}

#[derive(Debug)]
enum Place {
    Workspace(String),
    Gone,
    Directory(String),
}

#[derive(Debug)]
struct Wanted {
    place: Place,
    role: String,
    account: Option<String>,
    agent: Agent,
}

#[derive(Debug)]
struct Rail {
    agent: Agent,
    started: BTreeMap<Stage, i64>,
    ran: BTreeSet<Stage>,
}

impl Rail {
    fn new(agent: Agent) -> Rail {
        Rail {
            agent,
            started: BTreeMap::new(),
            ran: BTreeSet::new(),
        }
    }

    fn map(
        &mut self,
        content: &LaunchContent,
        event: SessionEvent,
        account: &AccountName,
        now_ms: i64,
    ) -> Vec<LaunchEvent> {
        match event {
            SessionEvent::Warning(LaunchWarning::DockerCapability) => {
                vec![LaunchEvent::Warning(DockerCapability {
                    risks: docker_risks(),
                })]
            }
            SessionEvent::StepStarted(step) => {
                let stage = stage_of(step);
                if self.started.contains_key(&stage) {
                    return Vec::new();
                }
                self.started.insert(stage, now_ms);
                vec![
                    LaunchEvent::StageChanged(stage.index(), StepState::Running),
                    LaunchEvent::Activity(
                        content.stages.get(stage.index()).activity_for(self.agent),
                    ),
                ]
            }
            SessionEvent::StepFinished { step, result } => {
                let stage = stage_of(step);
                if result == StepResult::Done {
                    self.ran.insert(stage);
                }
                if step != stage.last() {
                    return Vec::new();
                }
                if !self.ran.contains(&stage) {
                    return vec![LaunchEvent::StageChanged(stage.index(), StepState::Skipped)];
                }
                let began = self.started.get(&stage).copied().unwrap_or(now_ms);
                let mut events = vec![
                    LaunchEvent::StageChanged(stage.index(), StepState::Done),
                    LaunchEvent::StageTimed {
                        stage: stage.index(),
                        elapsed_ms: u64::try_from(now_ms.saturating_sub(began)).unwrap_or(0),
                    },
                ];
                if stage == Stage::Credentials {
                    events.push(LaunchEvent::CredentialsResolved {
                        origin: account.to_string(),
                        validation: String::new(),
                    });
                }
                events
            }
            SessionEvent::Image(ImageEvent::Line { text, .. }) => {
                vec![LaunchEvent::BuildLine(text.as_str().to_owned())]
            }
            SessionEvent::Image(_) => Vec::new(),
            SessionEvent::SandboxKnown(name) => vec![LaunchEvent::ContainerReady(name.to_string())],
        }
    }
}

struct Prepared {
    plan: LaunchPlan,
    session: Session,
    project: ProjectRoot,
    config: Config,
    name: WorkspaceName,
    gateway: GatewayName,
}

struct Run {
    backend: JackinBackend,
    content: Arc<LaunchContent>,
    wanted: Wanted,
    launch: LaunchId,
    out: mpsc::Sender<LaunchEvent>,
    stage: Arc<AtomicUsize>,
    answers: UnboundedReceiver<Vec<EnvAnswer>>,
    cancel: CancellationToken,
}

fn announce(out: &mpsc::Sender<LaunchEvent>, failure: LaunchFailure) {
    drop(out.send(LaunchEvent::StageChanged(failure.stage, StepState::Failed)));
    drop(out.send(LaunchEvent::Failed(Box::new(FailedLaunch {
        failure,
        instance: None,
    }))));
}

fn question(found: &jackin_session::EnvQuestion) -> EnvQuestion {
    EnvQuestion {
        name: found.name.to_string(),
        prompt: found
            .prompt
            .clone()
            .unwrap_or_else(|| found.name.to_string()),
        default: found.default.clone(),
    }
}

fn resolve(
    config: &Config,
    place: &Place,
) -> Result<(Config, WorkspaceName, Workspace), Unlaunchable> {
    match place {
        Place::Gone => Err(Unlaunchable::WorkspaceGone),
        Place::Workspace(name) => {
            let name = WorkspaceName::parse(name)?;
            let workspace = config
                .workspace(&name)
                .cloned()
                .ok_or(Unlaunchable::WorkspaceGone)?;
            Ok((config.clone(), name, workspace))
        }
        Place::Directory(text) => {
            let path = HostPath::parse(std::path::Path::new(text))?;
            match lookup_workspace(config, &path) {
                WorkspaceLookup::Saved { name, workspace } => Ok((config.clone(), name, workspace)),
                WorkspaceLookup::Unsaved { path } => {
                    let name = adhoc_workspace_name(config, &path)?;
                    let workspace = Workspace::new(path, false);
                    let mut adhoc = config.clone();
                    adhoc.apply(ConfigEdit::AddWorkspace {
                        name: name.clone(),
                        workspace: workspace.clone(),
                    })?;
                    Ok((adhoc, name, workspace))
                }
            }
        }
    }
}

impl Run {
    fn send(&self, event: LaunchEvent) {
        if let LaunchEvent::StageChanged(stage, StepState::Running) = &event {
            self.stage.store(*stage, Ordering::Relaxed);
        }
        drop(self.out.send(event));
    }

    fn now_ms(&self) -> i64 {
        self.backend.inputs.wall.epoch_millis()
    }

    async fn run(mut self) {
        if self.wanted.agent != Agent::Codex {
            self.send(LaunchEvent::Refused(RefusalReason::AgentNotLaunchable {
                agent: self.wanted.agent,
                launchable: Agent::Codex,
            }));
            return;
        }
        match self.prepare().await {
            Ok(prepared) => self.launch(prepared).await,
            Err(Stop::Refused(reason)) => self.send(LaunchEvent::Refused(reason)),
            Err(Stop::Failed(failure)) => self.report(*failure),
            Err(Stop::Cancelled) => {}
        }
    }

    async fn ask(
        &mut self,
        found: &[jackin_session::EnvQuestion],
    ) -> Result<BTreeMap<EnvName, String>, Stop> {
        self.send(LaunchEvent::NeedsInput(
            found.iter().map(question).collect(),
        ));
        let answers = tokio::select! {
            biased;
            () = self.cancel.cancelled() => return Err(Stop::Cancelled),
            answers = self.answers.recv() => answers.ok_or(Stop::Cancelled)?,
        };
        Ok(answers
            .into_iter()
            .filter_map(|answer| Some((EnvName::parse(&answer.name).ok()?, answer.value)))
            .collect())
    }

    async fn prepare(&mut self) -> Result<Prepared, Stop> {
        let inputs = Arc::clone(&self.backend.inputs);
        let saved = inputs
            .config
            .load()
            .await
            .map_err(|error| Stop::at(Stage::Image, &error))?;
        let (config, name, workspace) =
            resolve(&saved, &self.wanted.place).map_err(|error| Stop::at(Stage::Image, &error))?;
        let Some(gateway) = inputs.sessions.default_name().cloned() else {
            let error = saved
                .resolve_gateway(None)
                .err()
                .unwrap_or(ConfigError::NoGateway);
            return Err(Stop::failed(LaunchFailure {
                next_step: NextStep::RunDoctor,
                ..plain_failure(Stage::Image, &error)
            }));
        };
        let session = match inputs.sessions.access(&gateway) {
            Access::Up(session) => *session,
            Access::Down { error, .. } => {
                return Err(Stop::failed(gateway_failure(
                    &self.content.network_failure,
                    &error,
                )));
            }
            Access::NotChosen => {
                return Err(Stop::at(
                    Stage::Sandbox,
                    &ConfigError::UnknownGateway(gateway),
                ));
            }
        };
        let project = ProjectRoot::open(inputs.host.fs.as_ref(), &workspace.path)
            .await
            .map_err(|error| Stop::at(Stage::Image, &error))?;
        let role_name =
            RoleName::parse(&self.wanted.role).map_err(|error| Stop::at(Stage::Image, &error))?;
        let role = match inputs
            .roles
            .load(&config, Some(&role_name), &workspace, &self.cancel)
            .await
        {
            Ok(role) => role,
            Err(RoleError::NotAllowed(role)) => {
                return Err(Stop::Refused(RefusalReason::RoleNotAllowed {
                    workspace: name.to_string(),
                    role: role.to_string(),
                }));
            }
            Err(RoleError::Cancelled) => return Err(Stop::Cancelled),
            Err(error) => return Err(Stop::at(Stage::Image, &error)),
        };
        let account = self
            .wanted
            .account
            .as_deref()
            .map(AccountName::parse)
            .transpose()
            .map_err(|error| Stop::at(Stage::Credentials, &error))?;
        let mut values = BTreeMap::new();
        loop {
            let request = LaunchRequest::new(
                gateway.clone(),
                project.clone(),
                role.name().clone(),
                self.launch.clone(),
            )
            .account(account.clone())
            .prompting(Prompting::Interactive)
            .env(values.clone());
            match plan_launch(&config, &role, request) {
                Ok(plan) => {
                    return Ok(Prepared {
                        plan,
                        session,
                        project,
                        config,
                        name,
                        gateway,
                    });
                }
                Err(PlanError::EnvNeeded(found)) => values.extend(self.ask(&found).await?),
                Err(PlanError::RoleNotAllowed { workspace, role }) => {
                    return Err(Stop::Refused(RefusalReason::RoleNotAllowed {
                        workspace: workspace.to_string(),
                        role: role.to_string(),
                    }));
                }
                Err(error) => return Err(Stop::at(plan_stage(&error), &error)),
            }
        }
    }

    async fn launch(self, prepared: Prepared) {
        let Prepared {
            plan,
            session,
            project,
            config,
            name,
            gateway,
        } = prepared;
        let account = plan.account().account.clone();
        let (events, mut feed) = launch_events(LineDelivery::BestEffort);
        let mut rail = Rail::new(self.wanted.agent);
        let mapping = async {
            while let Some(event) = feed.recv().await {
                for mapped in rail.map(&self.content, event, &account, self.now_ms()) {
                    self.send(mapped);
                }
            }
        };
        let (outcome, ()) =
            tokio::join!(session.launch(plan, events, self.cancel.clone()), mapping);
        match outcome {
            Ok(LaunchOutcome::Ready(target)) => {
                match self
                    .ready(
                        &session,
                        &config,
                        &project,
                        &name,
                        &gateway,
                        &target.sandbox,
                    )
                    .await
                {
                    Ok(instance) => self.send(LaunchEvent::Ready(Box::new(ReadyInstance {
                        instance,
                        run_id: self.launch.to_string(),
                    }))),
                    Err(error) => {
                        let failure = plain_failure(Stage::Codex, &error);
                        self.fail(failure);
                    }
                }
            }
            Ok(LaunchOutcome::Cancelled(left)) => {
                self.cancelled(&session, &config, &gateway, left).await;
            }
            Err(failure) => {
                if let Some(sandbox) = &failure.sandbox {
                    self.send(LaunchEvent::ContainerReady(sandbox.to_string()));
                }
                self.fail(session_failure(&failure));
            }
        }
    }

    fn report(&self, failure: LaunchFailure) {
        announce(&self.out, failure);
    }

    fn fail(&self, failure: LaunchFailure) {
        self.report(failure);
        self.backend.refresh();
    }

    async fn cancelled(
        &self,
        session: &Session,
        config: &Config,
        gateway: &GatewayName,
        left: RestoreCandidate,
    ) {
        let Some(sandbox) = left.sandbox else {
            return;
        };
        match self
            .record(session, config, gateway, &sandbox, &Origins::new())
            .await
        {
            Ok(instance) => self.backend.bridge.send(Msg::LaunchCancelled {
                instance: Box::new(instance),
            }),
            Err(_) => self.backend.refresh(),
        }
    }

    async fn record(
        &self,
        session: &Session,
        config: &Config,
        gateway: &GatewayName,
        sandbox: &SandboxName,
        origins: &Origins,
    ) -> Result<Instance, jackin_session::LifecycleError> {
        let inspected = session.inspect(sandbox).await?;
        let work = session.work_state(config, sandbox).await;
        let observed = ObservedSandbox {
            gateway: gateway.clone(),
            summary: inspected.summary,
            supervisor: Some(inspected.supervisor),
            work: Some(work),
        };
        Ok(self.backend.inputs.instance_of(config, &observed, origins))
    }

    async fn ready(
        &self,
        session: &Session,
        config: &Config,
        project: &ProjectRoot,
        name: &WorkspaceName,
        gateway: &GatewayName,
        sandbox: &SandboxName,
    ) -> Result<Instance, jackin_session::LifecycleError> {
        let origins: Origins = session
            .project_origin(project, &self.cancel)
            .await
            .ok()
            .flatten()
            .map(|origin| (name.clone(), origin))
            .into_iter()
            .collect();
        self.record(session, config, gateway, sandbox, &origins)
            .await
    }
}

pub(crate) struct SessionLaunch {
    run_id: String,
    events: mpsc::Receiver<LaunchEvent>,
    answers: UnboundedSender<Vec<EnvAnswer>>,
    cancel: CancellationToken,
}

impl fmt::Debug for SessionLaunch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SessionLaunch")
            .field("run_id", &self.run_id)
            .finish_non_exhaustive()
    }
}

impl LaunchFeed for SessionLaunch {
    fn run_id(&self) -> &str {
        &self.run_id
    }

    fn poll(&mut self, _world: &World) -> Vec<LaunchEvent> {
        self.events.try_iter().collect()
    }

    fn retry_credentials(&mut self, _op: &mut dyn OnePassword) {}

    fn use_plain_text(&mut self) -> Option<TypedKey> {
        None
    }

    fn answer_env(&mut self, answers: Vec<EnvAnswer>) {
        drop(self.answers.send(answers));
    }

    fn cancel(&mut self, _world: &World) -> Option<Instance> {
        self.cancel.cancel();
        None
    }
}

fn place(world: &World, target: &LaunchTarget) -> Place {
    match target.workspace {
        Some(id) => world
            .workspace(id)
            .map_or(Place::Gone, |found| Place::Workspace(found.name.clone())),
        None => Place::Directory(world.cwd.clone()),
    }
}

pub(crate) fn start(
    backend: &JackinBackend,
    world: &World,
    target: &LaunchTarget,
) -> Box<dyn LaunchFeed> {
    let (out, events) = mpsc::channel();
    let (answer, answers) = unbounded_channel();
    let cancel = backend.inputs.cancel.child_token();
    let launch = match LaunchId::generate() {
        Ok(launch) => launch,
        Err(error) => return unlaunched(world, &error, out, events, answer, cancel),
    };
    let stage = Arc::new(AtomicUsize::new(Stage::Image.index()));
    let crashed = (out.clone(), Arc::clone(&stage), backend.clone());
    let run = Run {
        backend: backend.clone(),
        content: backend.launch_content(),
        wanted: Wanted {
            place: place(world, target),
            role: target.role.clone(),
            account: target.account.clone(),
            agent: target.agent,
        },
        launch: launch.clone(),
        out,
        stage,
        answers,
        cancel: cancel.clone(),
    };
    backend.bridge.detach(run.run(), move || {
        let (out, stage, backend) = crashed;
        announce(
            &out,
            LaunchFailure {
                stage: stage.load(Ordering::Relaxed),
                summary: STOPPED.to_owned(),
                next_step: NextStep::None,
                detail: Vec::new(),
            },
        );
        backend.refresh();
    });
    Box::new(SessionLaunch {
        run_id: launch.to_string(),
        events,
        answers: answer,
        cancel,
    })
}

fn unlaunched(
    world: &World,
    error: &EntropyError,
    out: mpsc::Sender<LaunchEvent>,
    events: mpsc::Receiver<LaunchEvent>,
    answers: UnboundedSender<Vec<EnvAnswer>>,
    cancel: CancellationToken,
) -> Box<dyn LaunchFeed> {
    let failure = plain_failure(Stage::Image, error);
    announce(&out, failure);
    Box::new(SessionLaunch {
        run_id: world.now_secs().to_string(),
        events,
        answers,
        cancel,
    })
}
