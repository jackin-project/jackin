// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use jackin_core::{
    AttemptId, Clock, Config, FileMode, FileSystem, HookKind, HostPath, LabelSelector, LaunchId,
    ProcessRunner, ProjectRoot, RoleName, SandboxName, SandboxPath, SecretBytes, SupervisorState,
    sandbox_layout,
};
use jackin_image::{BuildStage, BuiltImage, ImageEngine};
use jackin_openshell::{Gateway, SandboxFile, SandboxQuery};
use jackin_session::{
    AttachTarget, HostPorts, LaunchEvent, LaunchFailure, LaunchLimits, LaunchOutcome, LaunchPlan,
    LaunchRequest, LineDelivery, LoadedRole, Session, SessionDeps, launch_events, plan_launch,
};
use jackin_testkit::{FakeGateway, ProgramScript, RepoFixture, RoleFixture, SandboxHandle, World};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

pub(crate) type TestResult = Result<(), Box<dyn Error>>;

pub(crate) const ACCESS: &str = "ACCESS-SECRET-7f3a91";
pub(crate) const REFRESH: &str = "REFRESH-SECRET-c20e44";
pub(crate) const ACCOUNT_ID: &str = "3f2b8c1e-5d4a-4e7b-9c6d-1a2b3c4d5e6f";
pub(crate) const PLACEHOLDER: &str = "openshell:resolve:env:CODEX_AUTH_ACCESS_TOKEN";
const GUARD: Duration = Duration::from_secs(10);

pub(crate) async fn guard(work: impl Future<Output = TestResult>) -> TestResult {
    tokio::time::timeout(GUARD, work)
        .await
        .map_err(|_| "the test hung for 10 s")?
}

pub(crate) fn launch_id(byte: u8) -> LaunchId {
    LaunchId::from_entropy(&[byte; 26])
}

pub(crate) fn path(text: &str) -> Result<SandboxPath, Box<dyn Error>> {
    Ok(SandboxPath::parse(text)?)
}

pub(crate) fn limits(ready_secs: u64, supervisor_secs: u64) -> LaunchLimits {
    LaunchLimits {
        ready: Duration::from_secs(ready_secs),
        supervisor: Duration::from_secs(supervisor_secs),
        attempt: Duration::from_secs(20),
        hook: Duration::from_secs(600),
        upload: Duration::from_secs(600),
        create_attempts: std::num::NonZeroU8::MIN.saturating_add(2),
    }
}

pub(crate) fn login_json() -> Value {
    json!({
        "auth_mode": "chatgpt",
        "OPENAI_API_KEY": null,
        "tokens": {
            "id_token": "header.payload.signature",
            "access_token": ACCESS,
            "refresh_token": REFRESH,
            "oauth_provenance": {
                "issuer": "https://auth.openai.com",
                "client_id": "app_EMoamEEZ73f0CkXaXp7hrann",
            },
            "login_owner_id": "abcdefghijklmnopqrstuvwxyz0123456789ABCDEFG",
            "account_id": ACCOUNT_ID,
        },
        "last_refresh": "2026-11-02T10:11:12.123456789Z",
    })
}

pub(crate) fn default_role() -> Result<RoleFixture, Box<dyn Error>> {
    Ok(RoleFixture::new("sentinel")?
        .env("GREETING", Some("hello"), Some("Greeting"))
        .hook(HookKind::Setup, b"#!/bin/sh\n")
        .hook(HookKind::Preflight, b"#!/bin/sh\n"))
}

pub(crate) struct Spec {
    pub(crate) role: RoleFixture,
    pub(crate) limits: LaunchLimits,
    pub(crate) login: bool,
    pub(crate) printenv_exit: i32,
}

impl Spec {
    pub(crate) fn new() -> Result<Spec, Box<dyn Error>> {
        Ok(Spec {
            role: default_role()?,
            limits: limits(30, 20),
            login: true,
            printenv_exit: 0,
        })
    }

    pub(crate) async fn rig(self) -> Result<Rig, Box<dyn Error>> {
        Rig::build(self).await
    }
}

pub(crate) struct Rig {
    pub(crate) world: World,
    pub(crate) repo: RepoFixture,
    _role: RoleFixture,
    pub(crate) loaded: LoadedRole,
    pub(crate) config: Config,
    pub(crate) project: ProjectRoot,
    pub(crate) home: HostPath,
    _home_dir: TempDir,
}

pub(crate) struct Outcome {
    pub(crate) result: Result<LaunchOutcome, LaunchFailure>,
    pub(crate) events: Vec<LaunchEvent>,
}

fn script(exit_code: i32, stdout: &[u8]) -> ProgramScript {
    ProgramScript {
        exit_code,
        stdout: stdout.to_vec(),
        stderr: Vec::new(),
    }
}

impl Rig {
    pub(crate) async fn new() -> Result<Rig, Box<dyn Error>> {
        Spec::new()?.rig().await
    }

    async fn build(spec: Spec) -> Result<Rig, Box<dyn Error>> {
        let world = World::builder().limits(spec.limits).build()?;
        let repo = RepoFixture::new().await?;
        repo.file("src/main.rs", b"fn main() {}\n").await?;
        repo.file("README.md", b"readme\n").await?;
        repo.ignore("target/\n").await?;
        repo.commit().await?;
        repo.file("notes.txt", b"notes\n").await?;
        repo.file("target/out.bin", b"binary\n").await?;
        let role = spec.role;
        role.commit().await?;
        let loaded = world.roles.load_path(role.root()).await?;
        let home_dir = tempfile::tempdir()?;
        let home = HostPath::parse(&std::fs::canonicalize(home_dir.path())?)?;
        if spec.login {
            tokio::fs::write(
                home.as_path().join("auth.json"),
                login_json().to_string().as_bytes(),
            )
            .await?;
        }
        let config = Config::parse(&format!(
            "default_gateway = \"fake-gateway\"\n\n[gateways.fake-gateway]\nopenshell_workspace = \"default\"\nbuilder = \"jackin-builder\"\nplatform = \"linux/arm64\"\n\n[accounts.work]\nprovider = \"codex\"\ncredential = {{ source = \"codex-login\", home = \"{}\", chatgpt_account_id = \"{ACCOUNT_ID}\" }}\n\n[roles.sentinel]\nsource = {{ path = \"{}\" }}\n\n[workspaces.platform-backend]\npath = \"{}\"\n",
            home.as_path().display(),
            role.root().as_path().display(),
            repo.root().as_path().display(),
        ))?;
        let project = repo.project().await?;
        let gateway = &world.gateway;
        gateway.script_program(
            &sandbox_layout::PRINTENV,
            script(spec.printenv_exit, format!("{PLACEHOLDER}\n").as_bytes()),
        );
        for kind in [HookKind::Setup, HookKind::Preflight] {
            let hook = sandbox_layout::hook_path(kind);
            if loaded.manifest().hook(kind).is_some() {
                gateway.install_file(&hook, b"#!/bin/sh\n".to_vec());
                gateway.script_program(&hook, script(0, b""));
            }
        }
        Ok(Rig {
            world,
            repo,
            _role: role,
            loaded,
            config,
            project,
            home,
            _home_dir: home_dir,
        })
    }

    pub(crate) fn gateway(&self) -> &Arc<FakeGateway> {
        &self.world.gateway
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
        self.run(self.request(id)?, CancellationToken::new()).await
    }

    pub(crate) async fn run(
        &self,
        request: LaunchRequest,
        cancel: CancellationToken,
    ) -> Result<Outcome, Box<dyn Error>> {
        self.run_on(&self.world.session, request, cancel).await
    }

    pub(crate) async fn run_on(
        &self,
        session: &Session,
        request: LaunchRequest,
        cancel: CancellationToken,
    ) -> Result<Outcome, Box<dyn Error>> {
        let plan = plan_launch(&self.config, &self.loaded, request)?;
        let (events, mut feed) = launch_events(LineDelivery::Reliable);
        let collect = async {
            let mut seen = Vec::new();
            while let Some(event) = feed.recv().await {
                seen.push(event);
            }
            seen
        };
        let (result, events) = tokio::join!(session.launch(plan, events, cancel), collect);
        Ok(Outcome { result, events })
    }

    pub(crate) fn session_with(&self, limits: LaunchLimits) -> Session {
        let world = &self.world;
        let gateway: Arc<dyn Gateway> = Arc::<FakeGateway>::clone(&world.gateway);
        let engine: Arc<dyn ImageEngine> = Arc::<jackin_testkit::FakeEngine>::clone(&world.engine);
        let clock: Arc<dyn Clock> = Arc::<jackin_testkit::FakeClock>::clone(&world.clock);
        let runner: Arc<dyn ProcessRunner> =
            Arc::<jackin_testkit::TestRunner>::clone(&world.runner);
        let fs: Arc<dyn FileSystem> = Arc::clone(&world.fs);
        Session::new(SessionDeps {
            gateway,
            engine,
            host: HostPorts { clock, fs, runner },
            cache_dir: world.cache_dir().clone(),
            limits,
        })
    }

    pub(crate) async fn sandbox(&self, id: u8) -> Result<Option<SandboxHandle>, Box<dyn Error>> {
        let found = self
            .world
            .gateway
            .find_sandboxes(&SandboxQuery::Labels(LabelSelector::launch(&launch_id(id))))
            .await?;
        Ok(found
            .first()
            .and_then(|view| self.world.gateway.sandbox(&view.name)))
    }

    pub(crate) async fn handle(&self, id: u8) -> Result<SandboxHandle, Box<dyn Error>> {
        self.sandbox(id)
            .await?
            .ok_or_else(|| "the launch has no sandbox".into())
    }

    pub(crate) fn stages(&self) -> Vec<BuildStage> {
        self.world
            .engine
            .builds()
            .iter()
            .map(|build| build.stage)
            .collect()
    }

    pub(crate) fn final_image(&self) -> Result<BuiltImage, Box<dyn Error>> {
        let build = self
            .world
            .engine
            .builds()
            .into_iter()
            .rev()
            .find(|build| build.stage == BuildStage::Derived)
            .ok_or("no final image was built")?;
        let id = self
            .world
            .engine
            .images()
            .get(&build.tag)
            .cloned()
            .ok_or("the final image is not stored")?;
        Ok(BuiltImage {
            tag: build.tag,
            id,
            source: jackin_image::ImageSource::Built,
        })
    }
}

pub(crate) fn ready(outcome: Outcome) -> Result<AttachTarget, Box<dyn Error>> {
    match outcome.result {
        Ok(LaunchOutcome::Ready(target)) => Ok(target),
        other => Err(format!("expected a ready launch, saw {other:?}").into()),
    }
}

pub(crate) fn failure(outcome: Outcome) -> Result<LaunchFailure, Box<dyn Error>> {
    match outcome.result {
        Err(failure) => Ok(failure),
        other => Err(format!("expected a failed launch, saw {other:?}").into()),
    }
}

pub(crate) fn running(number: u64) -> Result<SupervisorState, Box<dyn Error>> {
    Ok(SupervisorState::Running {
        attempt: AttemptId::try_from(number)?,
    })
}

pub(crate) fn sandbox_name(id: u8, rig: &Rig) -> Result<SandboxName, Box<dyn Error>> {
    Ok(rig.plan(id)?.sandbox_name().clone())
}

pub(crate) fn has_file(handle: &SandboxHandle, target: &SandboxPath) -> bool {
    handle.file(target).is_some()
}

pub(crate) fn under_project(handle: &SandboxHandle) -> bool {
    handle
        .files()
        .keys()
        .any(|file| file.as_str().starts_with("/sandbox/project"))
}

pub(crate) fn converged(rig: &Rig, handle: &SandboxHandle, id: u8) -> TestResult {
    assert_eq!(handle.supervisor(), Some(running(1)?));
    assert_eq!(handle.file(&sandbox_layout::RUN_START), Some(b"1".to_vec()));
    assert!(has_file(
        handle,
        &sandbox_layout::uploaded_marker(&launch_id(id))
    ));
    assert!(has_file(
        handle,
        &sandbox_layout::setup_done_marker(&launch_id(id))
    ));
    let stages = rig.stages();
    for stage in [BuildStage::Construct, BuildStage::Derived] {
        assert_eq!(
            stages.iter().filter(|s| **s == stage).count(),
            1,
            "{stages:?}"
        );
    }
    Ok(())
}

pub(crate) async fn write(
    rig: &Rig,
    sandbox: &SandboxName,
    target: &SandboxPath,
    content: &str,
) -> TestResult {
    rig.world
        .gateway
        .write_file(
            sandbox,
            &SandboxFile {
                path: target.clone(),
                mode: FileMode(0o600),
                content: SecretBytes::new(content.as_bytes().to_vec()),
            },
        )
        .await?;
    Ok(())
}

pub(crate) fn unreachable() -> jackin_openshell::GatewayError {
    jackin_openshell::GatewayError::Protocol {
        message: "injected".to_owned(),
    }
}
