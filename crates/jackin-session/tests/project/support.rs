// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use jackin_core::{
    Clock, Config, HostPath, LabelSelector, LaunchId, LineSink, ProcessError, ProcessOutput,
    ProcessRunner, ProcessSpec, ProjectRoot, RoleName, SandboxPath, SecretOutput, StdFileSystem,
    sandbox_layout,
};
use jackin_image::ImageEngine;
use jackin_openshell::{Gateway, SandboxQuery};
use jackin_session::{
    HostPorts, LaunchError, LaunchFailure, LaunchOutcome, LaunchPlan, LaunchRequest, LineDelivery,
    LoadedRole, Session, SessionDeps, TreeMismatch, UnsupportedProject, launch_events, plan_launch,
};
use jackin_testkit::{
    FakeClock, FakeEngine, FakeGateway, RepoFixture, RoleFixture, SandboxHandle, TestRunner, World,
};
use serde_json::json;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

pub(crate) type TestResult = Result<(), Box<dyn Error>>;

pub(crate) const PLACEHOLDER: &str = "openshell:resolve:env:CODEX_AUTH_ACCESS_TOKEN";
const GUARD: Duration = Duration::from_secs(60);
const ACCOUNT_ID: &str = "3f2b8c1e-5d4a-4e7b-9c6d-1a2b3c4d5e6f";

pub(crate) async fn guard(work: impl Future<Output = TestResult>) -> TestResult {
    tokio::time::timeout(GUARD, work)
        .await
        .map_err(|_| "the test hung for 60 s")?
}

#[derive(Clone, Copy)]
pub(crate) enum Point {
    AfterOpen,
    AfterBundle(usize),
}

type Action = Box<dyn FnOnce() -> std::io::Result<()> + Send>;

pub(crate) struct Tamper {
    inner: Arc<TestRunner>,
    point: Point,
    action: Mutex<Option<Action>>,
    outcome: Mutex<Option<Result<(), String>>>,
    opened: AtomicBool,
    bundles: AtomicUsize,
}

impl std::fmt::Debug for Tamper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tamper").finish_non_exhaustive()
    }
}

impl Tamper {
    pub(crate) fn new(
        rig: &Rig,
        point: Point,
        action: impl FnOnce() -> std::io::Result<()> + Send + 'static,
    ) -> Arc<Tamper> {
        Arc::new(Tamper {
            inner: Arc::clone(&rig.world.runner),
            point,
            action: Mutex::new(Some(Box::new(action))),
            outcome: Mutex::new(None),
            opened: AtomicBool::new(false),
            bundles: AtomicUsize::new(0),
        })
    }

    pub(crate) fn done(&self) -> TestResult {
        match self
            .outcome
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
            Some(Ok(())) => Ok(()),
            Some(Err(error)) => Err(error.into()),
            None => Err("the tampering never ran".into()),
        }
    }

    fn act(&self) {
        let taken = self
            .action
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(action) = taken {
            *self.outcome.lock().unwrap_or_else(PoisonError::into_inner) =
                Some(action().map_err(|error| error.to_string()));
        }
    }
}

#[async_trait]
impl ProcessRunner for Tamper {
    async fn run(
        &self,
        spec: &ProcessSpec,
        lines: Option<&LineSink>,
        cancel: &CancellationToken,
    ) -> Result<ProcessOutput, ProcessError> {
        let names = |word: &str| spec.args.iter().any(|arg| arg == word);
        if names("--git-common-dir") {
            self.opened.store(true, Ordering::SeqCst);
        } else if matches!(self.point, Point::AfterOpen) && self.opened.load(Ordering::SeqCst) {
            self.act();
        }
        let output = self.inner.run(spec, lines, cancel).await;
        if let Point::AfterBundle(occurrence) = self.point
            && names("bundle")
            && names("create")
            && self.bundles.fetch_add(1, Ordering::SeqCst) + 1 == occurrence
        {
            self.act();
        }
        output
    }

    async fn run_secret(
        &self,
        spec: &ProcessSpec,
        cancel: &CancellationToken,
    ) -> Result<SecretOutput, ProcessError> {
        self.inner.run_secret(spec, cancel).await
    }
}

pub(crate) fn launch_id(byte: u8) -> LaunchId {
    LaunchId::from_entropy(&[byte; 26])
}

pub(crate) fn path(text: &str) -> Result<SandboxPath, Box<dyn Error>> {
    Ok(SandboxPath::parse(text)?)
}

pub(crate) struct Outcome {
    pub(crate) result: Result<LaunchOutcome, LaunchFailure>,
}

pub(crate) struct Rig {
    pub(crate) world: World,
    pub(crate) repo: RepoFixture,
    _role: RoleFixture,
    loaded: LoadedRole,
    config: Config,
    project: ProjectRoot,
    _home_dir: TempDir,
}

impl Rig {
    pub(crate) async fn new(repo: RepoFixture) -> Result<Rig, Box<dyn Error>> {
        let at = repo.root().clone();
        Rig::new_at(repo, &at).await
    }

    pub(crate) async fn new_at(repo: RepoFixture, at: &HostPath) -> Result<Rig, Box<dyn Error>> {
        let world = World::builder().build()?;
        let role = RoleFixture::new("sentinel")?;
        role.commit().await?;
        let loaded = world
            .roles
            .load_path(role.root(), &CancellationToken::new())
            .await?;
        let home_dir = tempfile::tempdir()?;
        let home = HostPath::parse(&std::fs::canonicalize(home_dir.path())?)?;
        let login = json!({
            "auth_mode": "chatgpt",
            "OPENAI_API_KEY": null,
            "tokens": {
                "id_token": "header.payload.signature",
                "access_token": "ACCESS-SECRET-7f3a91",
                "refresh_token": "REFRESH-SECRET-c20e44",
                "oauth_provenance": {
                    "issuer": "https://auth.openai.com",
                    "client_id": "app_EMoamEEZ73f0CkXaXp7hrann",
                },
                "login_owner_id": "abcdefghijklmnopqrstuvwxyz0123456789ABCDEFG",
                "account_id": ACCOUNT_ID,
            },
            "last_refresh": "2026-11-02T10:11:12.123456789Z",
        });
        tokio::fs::write(home.as_path().join("auth.json"), login.to_string()).await?;
        let config = Config::parse(&format!(
            "default_gateway = \"fake-gateway\"\n\n[gateways.fake-gateway]\nopenshell_workspace = \"default\"\nbuilder = \"jackin-builder\"\nplatform = \"linux/arm64\"\n\n[accounts.work]\nprovider = \"codex\"\ncredential = {{ source = \"codex-login\", home = \"{}\", chatgpt_account_id = \"{ACCOUNT_ID}\" }}\n\n[roles.sentinel]\nsource = {{ path = \"{}\" }}\n\n[workspaces.platform-backend]\npath = \"{}\"\n",
            home.as_path().display(),
            role.root().as_path().display(),
            at.as_path().display(),
        ))?;
        world.seed_config(&config).await?;
        let project = ProjectRoot::open(&StdFileSystem::default(), at).await?;
        world.gateway.script_program(
            &sandbox_layout::PRINTENV,
            jackin_testkit::ProgramScript {
                exit_code: 0,
                stdout: format!("{PLACEHOLDER}\n").into_bytes(),
                stderr: Vec::new(),
            },
        );
        Ok(Rig {
            world,
            repo,
            _role: role,
            loaded,
            config,
            project,
            _home_dir: home_dir,
        })
    }

    pub(crate) fn project(&self) -> &ProjectRoot {
        &self.project
    }

    pub(crate) fn request(&self, id: u8) -> Result<LaunchRequest, Box<dyn Error>> {
        Ok(LaunchRequest::new(
            self.world.gateway.name().clone(),
            self.project.clone(),
            RoleName::parse("sentinel")?,
            launch_id(id),
        ))
    }

    pub(crate) fn plan(&self, id: u8) -> Result<LaunchPlan, Box<dyn Error>> {
        Ok(plan_launch(&self.config, &self.loaded, self.request(id)?)?)
    }

    pub(crate) async fn launch(&self, id: u8) -> Result<Outcome, Box<dyn Error>> {
        self.launch_with(id, CancellationToken::new()).await
    }

    pub(crate) async fn launch_with(
        &self,
        id: u8,
        cancel: CancellationToken,
    ) -> Result<Outcome, Box<dyn Error>> {
        let plan = self.plan(id)?;
        let (events, mut feed) = launch_events(LineDelivery::Reliable);
        let drain = async { while feed.recv().await.is_some() {} };
        let (result, ()) = tokio::join!(self.world.session.launch(plan, events, cancel), drain);
        Ok(Outcome { result })
    }

    pub(crate) async fn launch_over(
        &self,
        id: u8,
        runner: Arc<dyn ProcessRunner>,
        cancel: CancellationToken,
    ) -> Result<Outcome, Box<dyn Error>> {
        let world = &self.world;
        let gateway: Arc<dyn Gateway> = Arc::<FakeGateway>::clone(&world.gateway);
        let engine: Arc<dyn ImageEngine> = Arc::<FakeEngine>::clone(&world.engine);
        let clock: Arc<dyn Clock> = Arc::<FakeClock>::clone(&world.clock);
        let session = Session::new(SessionDeps {
            gateway,
            engine,
            host: HostPorts {
                clock,
                fs: Arc::clone(&world.fs),
                runner,
            },
            cache_dir: world.cache_dir().clone(),
            config: world.config.clone(),
            limits: jackin_testkit::default_limits(),
        });
        let plan = self.plan(id)?;
        let (events, mut feed) = launch_events(LineDelivery::Reliable);
        let drain = async { while feed.recv().await.is_some() {} };
        let (result, ()) = tokio::join!(session.launch(plan, events, cancel), drain);
        Ok(Outcome { result })
    }

    pub(crate) async fn handle(&self, id: u8) -> Result<SandboxHandle, Box<dyn Error>> {
        let found = self
            .world
            .gateway
            .find_sandboxes(&SandboxQuery::Labels(LabelSelector::launch(&launch_id(id))))
            .await?;
        found
            .first()
            .and_then(|view| self.world.gateway.sandbox(&view.name))
            .ok_or_else(|| "the launch has no sandbox".into())
    }
}

pub(crate) fn ready(outcome: Outcome) -> TestResult {
    match outcome.result {
        Ok(LaunchOutcome::Ready(_)) => Ok(()),
        other => Err(format!("expected a ready launch, saw {other:?}").into()),
    }
}

pub(crate) fn failure(outcome: Outcome) -> Result<LaunchFailure, Box<dyn Error>> {
    match outcome.result {
        Err(failure) => Ok(failure),
        other => Err(format!("expected a failed launch, saw {other:?}").into()),
    }
}

pub(crate) fn unsupported(outcome: Outcome) -> Result<UnsupportedProject, Box<dyn Error>> {
    match *failure(outcome)?.error {
        LaunchError::Unsupported(refusal) => Ok(refusal),
        other => Err(format!("expected a refusal, saw {other:?}").into()),
    }
}

pub(crate) fn mismatch(outcome: Outcome) -> Result<TreeMismatch, Box<dyn Error>> {
    match *failure(outcome)?.error {
        LaunchError::ProjectMismatch(found) => Ok(*found),
        other => Err(format!("expected a ProjectMismatch, saw {other:?}").into()),
    }
}

pub(crate) fn tree_of(path: &Path) -> Result<Vec<String>, Box<dyn Error>> {
    let mut names = Vec::new();
    if !path.exists() {
        return Ok(names);
    }
    let mut pending = vec![path.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let relative = entry.path().strip_prefix(path)?.display().to_string();
            if entry.file_type()?.is_dir() {
                pending.push(entry.path());
            }
            names.push(relative);
        }
    }
    names.sort();
    Ok(names)
}

pub(crate) fn assert_nothing_created(rig: &Rig) -> TestResult {
    assert!(rig.world.gateway.sandboxes().is_empty());
    assert!(rig.world.gateway.providers().is_empty());
    assert!(rig.world.gateway.profiles().is_empty());
    assert!(rig.world.engine.builds().is_empty());
    assert!(rig.world.engine.images().is_empty());
    let cache = tree_of(rig.world.cache_dir().as_path())?;
    assert_eq!(
        cache.iter().filter(|name| name.contains("upload-")).count(),
        0,
        "{cache:?}"
    );
    Ok(())
}

pub(crate) fn global_config(rig: &Rig, text: &str) -> TestResult {
    std::fs::write(rig.world.home().as_path().join(".gitconfig"), text)?;
    Ok(())
}
