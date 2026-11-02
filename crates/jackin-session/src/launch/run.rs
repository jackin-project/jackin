// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::time::Duration;

use jackin_core::{
    Agent, FileMode, HookKind, LabelSelector, SandboxLabels, SandboxName, SandboxPath, SecretBytes,
    StartRequest, sandbox_layout,
};
use jackin_image::{BuiltImage, ImageSource, ensure_image};
use jackin_openshell::{
    GatewayError, GatewayOp, PowerAction, ProgramRun, ProgramStdin, ProjectUpload, SandboxFile,
    SandboxQuery, SandboxView,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::files::render_auth_json;
use crate::launch::decide::{Act, Boot, Decision, Done, Finish, HookPlan, Observed, Wait, decide};
use crate::launch::hooks::{self, HookRun};
use crate::launch::probe;
use crate::launch::tracker::Tracker;
use crate::launch::{
    LaunchError, LaunchEvent, LaunchEvents, LaunchFailure, LaunchOutcome, LaunchStep,
    RestoreCandidate, SkipReason,
};
use crate::plan::LaunchPlan;
use crate::service::Session;

const POLL: Duration = Duration::from_millis(500);
const EXEC_TIMEOUT: Duration = Duration::from_secs(30);
const IMAGE_PROGRESS: usize = 64;
const MARKER_MODE: FileMode = FileMode(0o600);

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
            Ok(_) | Err(_) => None,
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
            0 => {
                self.known = None;
                Ok(Observed::Absent)
            }
            1 => {
                let view = views.remove(0);
                self.known = Some(view.name.clone());
                self.settled(view, hint).await
            }
            _ => {
                self.known = None;
                Ok(Observed::Ambiguous(
                    views.into_iter().map(|view| view.name).collect(),
                ))
            }
        }
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
        .await
        .map_err(|error| failed(hint, error))?;
        Ok(Observed::Ready(view, probe))
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
        let gateway = self.session.deps.gateway.as_ref();
        let name = self.plan.draft().providers().first();
        let present = match name {
            Some(name) => gateway
                .provider_state(name)
                .await
                .map_err(|error| failed(LaunchStep::Provider, error))?,
            None => jackin_openshell::ProviderState::Present,
        };
        self.live()?;
        if present == jackin_openshell::ProviderState::Absent {
            let account = self.plan.account_entry();
            self.session.sync_account(account).await.map_err(|source| {
                failed(
                    LaunchStep::Provider,
                    LaunchError::ProviderUnavailable {
                        account: account.name().clone(),
                        source: Box::new(source),
                    },
                )
            })?;
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
            return Ok(());
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
        let upload = ProjectUpload {
            source: self.plan.project().clone(),
            dest: sandbox_layout::PROJECT.clone(),
        };
        let limit = self.session.deps.limits.upload;
        let sending = self
            .session
            .deps
            .gateway
            .upload_project(sandbox, &upload, self.cancel);
        match tokio::time::timeout(limit, sending).await {
            Ok(result) => result.map_err(|error| failed(STEP, error))?,
            Err(_) => {
                return Err(failed(
                    STEP,
                    GatewayError::Timeout {
                        op: GatewayOp::Upload,
                        after: limit,
                    },
                ));
            }
        }
        let marker = sandbox_layout::uploaded_marker(self.plan.launch_id());
        self.write(STEP, sandbox, &marker, MARKER_MODE, Vec::new())
            .await
    }

    async fn hooks(&mut self, sandbox: &SandboxName, plan: HookPlan) -> Result<(), Stop> {
        const STEP: LaunchStep = LaunchStep::Hooks;
        self.enter(STEP).await;
        if plan.setup_done {
            self.tracker.skip(SkipReason::SetupAlreadyDone);
        }
        if plan.setup || plan.preflight {
            let gateway = self.session.deps.gateway.as_ref();
            let env = hooks::read_env(gateway, sandbox, self.plan.launch_id())
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
