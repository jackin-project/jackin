// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::time::Duration;

use jackin_core::{
    Agent, FileMode, HookKind, HostPath, LabelSelector, SandboxLabels, SandboxName, SandboxPath,
    SecretBytes, StartRequest, sandbox_layout,
};
use jackin_image::{BuiltImage, ImageSource, ensure_image};
use jackin_openshell::{
    GatewayError, GatewayOp, Missing, PowerAction, ProgramRun, ProgramStdin, ProviderState,
    SandboxFile, SandboxQuery, SandboxView,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::accounts::AccountError;
use crate::files::render_auth_json;
use crate::launch::decide::{Act, Boot, Decision, Done, Finish, HookPlan, Observed, Wait, decide};
use crate::launch::hooks::{self, HookRun};
use crate::launch::probe;
use crate::launch::tracker::Tracker;
use crate::launch::{
    LaunchError, LaunchEvent, LaunchEvents, LaunchFailure, LaunchOutcome, LaunchStep,
    RestoreCandidate, SkipReason,
};
use crate::lock::LockError;
use crate::plan::LaunchPlan;
use crate::project;
use crate::service::{POLL, Session};

const EXEC_TIMEOUT: Duration = Duration::from_secs(30);
const IMAGE_PROGRESS: usize = 64;
const MARKER_MODE: FileMode = FileMode(0o600);

enum Uploaded {
    Done,
    ByPeer,
}

enum Stop {
    Cancelled,
    Failed(LaunchStep, LaunchError),
}

fn failed(step: LaunchStep, error: impl Into<LaunchError>) -> Stop {
    Stop::Failed(step, error.into())
}

struct Run<'a> {
    session: &'a Session,
    plan: &'a LaunchPlan,
    events: &'a LaunchEvents,
    cancel: &'a CancellationToken,
    done: Done,
    tracker: Tracker,
    known: Option<SandboxName>,
}

impl Session {
    pub async fn launch(
        &self,
        plan: LaunchPlan,
        events: LaunchEvents,
        cancel: CancellationToken,
    ) -> Result<LaunchOutcome, LaunchFailure> {
        Run {
            session: self,
            plan: &plan,
            events: &events,
            cancel: &cancel,
            done: Done::new(),
            tracker: Tracker::new(),
            known: None,
        }
        .execute()
        .await
    }
}

impl Run<'_> {
    async fn execute(mut self) -> Result<LaunchOutcome, LaunchFailure> {
        if self.cancel.is_cancelled() {
            return Ok(self.cancelled().await);
        }
        let deps = &self.session.deps;
        match project::precheck(
            deps,
            self.plan.project(),
            self.plan.launch_id(),
            self.cancel,
        )
        .await
        {
            Ok(()) => {}
            Err(_) if self.cancel.is_cancelled() => return Ok(self.cancelled().await),
            Err(error) => {
                return Err(LaunchFailure {
                    launch_id: self.plan.launch_id().clone(),
                    step: LaunchStep::Upload,
                    sandbox: None,
                    error: Box::new(error),
                });
            }
        }
        self.drive().await
    }

    async fn drive(&mut self) -> Result<LaunchOutcome, LaunchFailure> {
        loop {
            let observed = match self.observe().await {
                Ok(observed) => observed,
                Err(stop) => return self.stopped(stop).await,
            };
            if self.cancel.is_cancelled() {
                return Ok(self.cancelled().await);
            }
            self.done.absorb(&observed);
            let limits = self.session.deps.limits;
            match decide(self.plan, &observed, &self.done, &limits) {
                Decision::Finish(finish) => return Ok(self.finished(finish).await),
                Decision::Fail(step, error) => {
                    return self.stopped(Stop::Failed(step, error)).await;
                }
                Decision::Run(act) => {
                    if let Err(stop) = self.act(act).await {
                        return self.stopped(stop).await;
                    }
                }
            }
        }
    }

    async fn stopped(&mut self, stop: Stop) -> Result<LaunchOutcome, LaunchFailure> {
        match stop {
            Stop::Failed(step, error) if !self.cancel.is_cancelled() => {
                self.tracker
                    .enter(self.events, step, self.known.as_ref())
                    .await;
                Err(LaunchFailure {
                    launch_id: self.plan.launch_id().clone(),
                    step,
                    sandbox: self.known.clone(),
                    error: Box::new(error),
                })
            }
            Stop::Cancelled | Stop::Failed(..) => Ok(self.cancelled().await),
        }
    }

    async fn cancelled(&self) -> LaunchOutcome {
        let found = self
            .session
            .deps
            .gateway
            .find_sandboxes(&SandboxQuery::Labels(LabelSelector::launch(
                self.plan.launch_id(),
            )))
            .await;
        let sandbox = match found {
            Ok(views) if views.len() == 1 => views.first().map(|view| view.name.clone()),
            Ok(_) => None,
            Err(_) => self.known.clone(),
        };
        LaunchOutcome::Cancelled(RestoreCandidate {
            launch_id: self.plan.launch_id().clone(),
            sandbox,
        })
    }

    async fn finished(&mut self, finish: Finish) -> LaunchOutcome {
        self.tracker
            .finish(self.events, finish.running, self.known.as_ref())
            .await;
        LaunchOutcome::Ready(finish.target)
    }

    async fn enter(&mut self, step: LaunchStep) {
        self.tracker
            .enter(self.events, step, self.known.as_ref())
            .await;
    }

    fn live(&self) -> Result<(), Stop> {
        if self.cancel.is_cancelled() {
            Err(Stop::Cancelled)
        } else {
            Ok(())
        }
    }

    async fn observe(&mut self) -> Result<Observed, Stop> {
        let hint = self.tracker.current().unwrap_or(LaunchStep::Sandbox);
        let gateway = self.session.deps.gateway.as_ref();
        let mut views = gateway
            .find_sandboxes(&SandboxQuery::Labels(LabelSelector::launch(
                self.plan.launch_id(),
            )))
            .await
            .map_err(|error| failed(hint, error))?;
        match views.len() {
            0 => Ok(self.vanished_sandbox()),
            1 => {
                let view = views.remove(0);
                self.known = Some(view.name.clone());
                match self.settled(view, hint).await? {
                    Observed::Absent => Ok(self.vanished_sandbox()),
                    observed => Ok(observed),
                }
            }
            _ => {
                self.known = None;
                Ok(Observed::Ambiguous(
                    views.into_iter().map(|view| view.name).collect(),
                ))
            }
        }
    }

    fn vanished_sandbox(&mut self) -> Observed {
        if self.known.take().is_some() {
            self.done.forget_sandbox();
        }
        Observed::Absent
    }

    async fn settled(&self, view: SandboxView, hint: LaunchStep) -> Result<Observed, Stop> {
        if !view.phase.accepts_exec() {
            return Ok(Observed::Settling(view));
        }
        let probe = probe::read(
            self.session.deps.gateway.as_ref(),
            &view.name,
            self.plan.launch_id(),
        )
        .await;
        match probe {
            Ok(probe) => Ok(Observed::Ready(view, probe)),
            Err(LaunchError::Gateway(GatewayError::NotFound {
                what: Missing::Sandbox(gone),
            })) if gone == view.name => Ok(Observed::Absent),
            Err(error) => Err(failed(hint, error)),
        }
    }

    async fn act(&mut self, act: Act) -> Result<(), Stop> {
        match act {
            Act::Image => self.image().await,
            Act::Provider => self.provider().await,
            Act::Create(image) => self.create(&image).await,
            Act::Replace(sandbox) => self.replace(&sandbox).await,
            Act::Power(sandbox) => self.power(&sandbox).await,
            Act::Wait(kind) => self.wait(kind).await,
            Act::Credentials(sandbox) => self.credentials(&sandbox).await,
            Act::Upload(sandbox) => self.upload(&sandbox).await,
            Act::Hooks(sandbox, plan) => self.hooks(&sandbox, plan).await,
            Act::Start(sandbox, attempt) => self.start(&sandbox, attempt).await,
        }
    }

    async fn image(&mut self) -> Result<(), Stop> {
        self.enter(LaunchStep::Image).await;
        let (events, plan, cancel) = (self.events, self.plan, self.cancel);
        let engine = self.session.deps.engine.as_ref();
        let (sender, mut receiver) = mpsc::channel(IMAGE_PROGRESS);
        let work = async {
            let result = ensure_image(engine, plan.image_request(), &sender, cancel).await;
            drop(sender);
            result
        };
        let forward = async {
            while let Some(event) = receiver.recv().await {
                events.emit(LaunchEvent::Image(event)).await;
            }
        };
        let (result, ()) = tokio::join!(work, forward);
        let built = result.map_err(|error| failed(LaunchStep::Image, error))?;
        if built.source == ImageSource::Cache {
            self.tracker.skip(SkipReason::ImageCached);
        }
        self.done.image = Some(built);
        Ok(())
    }

    async fn provider(&mut self) -> Result<(), Stop> {
        self.enter(LaunchStep::Provider).await;
        let name = self.plan.draft().provider();
        let state = self
            .session
            .deps
            .gateway
            .provider_state(name)
            .await
            .map_err(|error| failed(LaunchStep::Provider, error))?;
        self.live()?;
        if state == ProviderState::Absent {
            let account = self.plan.account_entry();
            let synced = self
                .session
                .sync_if_absent(account, name, self.cancel)
                .await
                .map_err(|source| match source {
                    AccountError::Lock(LockError::Cancelled) => Stop::Cancelled,
                    other => failed(
                        LaunchStep::Provider,
                        LaunchError::ProviderUnavailable {
                            account: account.name().clone(),
                            source: Box::new(other),
                        },
                    ),
                })?;
            if !synced {
                self.tracker.skip(SkipReason::ProviderPresent);
            }
        } else {
            self.tracker.skip(SkipReason::ProviderPresent);
        }
        self.done.steps.insert(LaunchStep::Provider);
        Ok(())
    }

    async fn create(&mut self, image: &BuiltImage) -> Result<(), Stop> {
        self.enter(LaunchStep::Sandbox).await;
        let create = self.plan.draft().complete(image);
        match self.session.deps.gateway.create_sandbox(&create).await {
            Ok(view) => {
                self.done.created = self.done.created.saturating_add(1);
                self.known = Some(view.name);
                Ok(())
            }
            Err(GatewayError::AlreadyExists { .. }) => self.join(create.name()).await,
            Err(error) => Err(failed(LaunchStep::Sandbox, error)),
        }
    }

    async fn join(&mut self, name: &SandboxName) -> Result<(), Stop> {
        let found = self
            .session
            .deps
            .gateway
            .find_sandboxes(&SandboxQuery::Name(name.clone()))
            .await
            .map_err(|error| failed(LaunchStep::Sandbox, error))?;
        let Some(view) = found.first() else {
            return self.vanished(name).await;
        };
        let other = SandboxLabels::from_labels(&view.labels)
            .ok()
            .map(|labels| labels.planned.launch);
        if other.as_ref() == Some(self.plan.launch_id()) {
            self.known = Some(view.name.clone());
            Ok(())
        } else {
            Err(failed(
                LaunchStep::Sandbox,
                LaunchError::NameCollision {
                    name: name.clone(),
                    other,
                },
            ))
        }
    }

    async fn vanished(&mut self, name: &SandboxName) -> Result<(), Stop> {
        self.done.created = self.done.created.saturating_add(1);
        if self.done.created >= self.session.deps.limits.create_attempts.get() {
            return Err(failed(
                LaunchStep::Sandbox,
                LaunchError::NameCollision {
                    name: name.clone(),
                    other: None,
                },
            ));
        }
        tokio::select! {
            biased;
            () = self.cancel.cancelled() => Err(Stop::Cancelled),
            () = self.session.deps.host.clock.sleep(POLL) => Ok(()),
        }
    }

    async fn replace(&mut self, sandbox: &SandboxName) -> Result<(), Stop> {
        self.enter(LaunchStep::Sandbox).await;
        self.session
            .deps
            .gateway
            .delete_sandbox(sandbox)
            .await
            .map_err(|error| failed(LaunchStep::Sandbox, error))
    }

    async fn power(&mut self, sandbox: &SandboxName) -> Result<(), Stop> {
        self.enter(LaunchStep::Ready).await;
        self.session
            .deps
            .gateway
            .set_power(sandbox, PowerAction::Start)
            .await
            .map_err(|error| failed(LaunchStep::Ready, error))?;
        self.done.boot = Boot::Powered;
        Ok(())
    }

    async fn wait(&mut self, kind: Wait) -> Result<(), Stop> {
        self.enter(match kind {
            Wait::Ready => LaunchStep::Ready,
            Wait::Supervisor => LaunchStep::Supervisor,
            Wait::Attempt => LaunchStep::Start,
        })
        .await;
        let clock = &self.session.deps.host.clock;
        tokio::select! {
            biased;
            () = self.cancel.cancelled() => return Err(Stop::Cancelled),
            () = clock.sleep(POLL) => {}
        }
        self.done.wait(kind, POLL);
        Ok(())
    }

    async fn write(
        &self,
        step: LaunchStep,
        sandbox: &SandboxName,
        path: &SandboxPath,
        mode: FileMode,
        content: Vec<u8>,
    ) -> Result<(), Stop> {
        self.live()?;
        self.session
            .deps
            .gateway
            .write_file(
                sandbox,
                &SandboxFile {
                    path: path.clone(),
                    mode,
                    content: SecretBytes::new(content),
                },
            )
            .await
            .map_err(|error| failed(step, error))
    }

    async fn credentials(&mut self, sandbox: &SandboxName) -> Result<(), Stop> {
        const STEP: LaunchStep = LaunchStep::Credentials;
        self.enter(STEP).await;
        let spec = Agent::Codex.spec();
        let key = spec.openshell.keys.access_token;
        let outcome = self
            .session
            .deps
            .gateway
            .run_program(
                &ProgramRun {
                    sandbox: sandbox.clone(),
                    program: sandbox_layout::PRINTENV.clone(),
                    args: vec![key.to_owned()],
                    workdir: None,
                    env: BTreeMap::new(),
                    stdin: ProgramStdin::Closed,
                    touch_on_success: None,
                    timeout: EXEC_TIMEOUT,
                },
                self.cancel,
            )
            .await
            .map_err(|error| failed(STEP, error))?;
        let placeholder = String::from_utf8_lossy(&outcome.stdout)
            .trim_end_matches(['\n', '\r'])
            .to_owned();
        if outcome.exit_code != 0 || placeholder.is_empty() {
            return Err(failed(
                STEP,
                LaunchError::PlaceholderMissing {
                    sandbox: sandbox.clone(),
                    key: key.to_owned(),
                },
            ));
        }
        let auth = render_auth_json(
            &placeholder,
            self.plan.chatgpt_account(),
            self.session.deps.host.clock.now(),
        );
        self.write(
            STEP,
            sandbox,
            &spec.auth_file.path,
            spec.auth_file.mode,
            auth.into_bytes(),
        )
        .await?;
        let files = self.plan.files();
        for file in [&files.codex_config, &files.env] {
            self.write(
                STEP,
                sandbox,
                &file.path,
                file.mode,
                file.text.clone().into_bytes(),
            )
            .await?;
        }
        self.done.steps.insert(STEP);
        Ok(())
    }

    async fn upload(&mut self, sandbox: &SandboxName) -> Result<(), Stop> {
        const STEP: LaunchStep = LaunchStep::Upload;
        self.enter(STEP).await;
        let deps = &self.session.deps;
        let launch = self.plan.launch_id();
        let _held = project::hold(deps, launch, self.cancel)
            .await
            .map_err(|error| failed(STEP, error))?;
        let dir = project::staging_dir(deps, launch).map_err(|error| failed(STEP, error))?;
        let limit = deps.limits.upload;
        let timed = tokio::time::timeout(limit, self.upload_once(sandbox, &dir)).await;
        project::release(deps, &dir).await;
        match timed {
            Ok(Ok(Uploaded::Done)) => Ok(()),
            Ok(Ok(Uploaded::ByPeer)) => {
                self.tracker.skip(SkipReason::AlreadyUploaded);
                Ok(())
            }
            Ok(Err(stop)) => Err(stop),
            Err(_) => Err(failed(
                STEP,
                GatewayError::Timeout {
                    op: GatewayOp::UploadTree,
                    after: limit,
                },
            )),
        }
    }

    async fn upload_once(&self, sandbox: &SandboxName, dir: &HostPath) -> Result<Uploaded, Stop> {
        const STEP: LaunchStep = LaunchStep::Upload;
        let deps = &self.session.deps;
        let launch = self.plan.launch_id();
        let marker = sandbox_layout::uploaded_marker(launch);
        let found = deps
            .gateway
            .read_files(sandbox, std::slice::from_ref(&marker))
            .await
            .map_err(|error| failed(STEP, error))?;
        if found.first().is_some_and(Option::is_some) {
            return Ok(Uploaded::ByPeer);
        }
        let staged = project::stage(deps, self.plan.project(), dir, self.cancel)
            .await
            .map_err(|error| failed(STEP, error))?;
        staged
            .upload(deps, self.plan.project(), sandbox, launch, self.cancel)
            .await
            .map_err(|error| failed(STEP, error))?;
        self.write(STEP, sandbox, &marker, MARKER_MODE, Vec::new())
            .await?;
        Ok(Uploaded::Done)
    }

    async fn hooks(&mut self, sandbox: &SandboxName, plan: HookPlan) -> Result<(), Stop> {
        const STEP: LaunchStep = LaunchStep::Hooks;
        self.enter(STEP).await;
        if plan.setup_done {
            self.tracker.skip(SkipReason::SetupAlreadyDone);
        }
        if plan.setup || plan.preflight {
            let gateway = self.session.deps.gateway.as_ref();
            let env = crate::state::env_file(gateway, sandbox, self.plan.launch_id())
                .await
                .map_err(|error| failed(STEP, error))?;
            let timeout = self.session.deps.limits.hook;
            for (wanted, kind) in [
                (plan.setup, HookKind::Setup),
                (plan.preflight, HookKind::Preflight),
            ] {
                if !wanted {
                    continue;
                }
                self.live()?;
                let touch_on_success = (kind == HookKind::Setup)
                    .then(|| sandbox_layout::setup_done_marker(self.plan.launch_id()));
                hooks::run(
                    HookRun {
                        gateway,
                        sandbox,
                        kind,
                        env: &env,
                        touch_on_success,
                        timeout,
                    },
                    self.cancel,
                )
                .await
                .map_err(|error| failed(STEP, error))?;
            }
        }
        self.done.steps.insert(STEP);
        Ok(())
    }

    async fn start(
        &mut self,
        sandbox: &SandboxName,
        attempt: jackin_core::AttemptId,
    ) -> Result<(), Stop> {
        self.enter(LaunchStep::Start).await;
        self.write(
            LaunchStep::Start,
            sandbox,
            &sandbox_layout::RUN_START,
            MARKER_MODE,
            StartRequest(attempt).render().into_bytes(),
        )
        .await?;
        self.done.started = Some(attempt);
        Ok(())
    }
}
