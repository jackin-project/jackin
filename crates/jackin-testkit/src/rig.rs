// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use jackin_core::{
    Clock, Config, EngineCert, EnvName, FileSystem, HookKind, HostPath, LabelSelector, LaunchId,
    PlainEnvValue, ProcessRunner, ProjectRoot, RoleName, SandboxName, sandbox_layout,
};
use jackin_image::{BuildStage, BuiltImage, ImageEngine, ImageSource};
use jackin_openshell::{Gateway, SandboxQuery};
use jackin_session::{
    HostPorts, LaunchEvent, LaunchFailure, LaunchLimits, LaunchOutcome, LaunchPlan, LaunchRequest,
    LineDelivery, LoadedRole, Session, SessionDeps, launch_events, plan_launch,
};
use rcgen::{BasicConstraints, CertificateParams, IsCa, KeyPair};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

use crate::engine::FakeEngine;
use crate::fixtures::{FixtureError, RepoFixture, RoleFixture};
use crate::gateway::{FakeGateway, ProgramScript, SandboxHandle};
use crate::host::{FakeClock, TestRunner};
use crate::world::{World, default_limits};

pub type TestResult = Result<(), FixtureError>;

const GUARD: Duration = Duration::from_secs(170);

pub async fn guard<T>(
    work: impl Future<Output = Result<T, FixtureError>>,
) -> Result<T, FixtureError> {
    guard_within(GUARD, work).await
}

pub async fn guard_within<T>(
    limit: Duration,
    work: impl Future<Output = Result<T, FixtureError>>,
) -> Result<T, FixtureError> {
    tokio::time::timeout(limit, work)
        .await
        .map_err(|_| format!("the test hung for {} s", limit.as_secs()))?
}

pub const ACCESS: &str = "ACCESS-SECRET-7f3a91";
pub const REFRESH: &str = "REFRESH-SECRET-c20e44";
pub const ACCOUNT_ID: &str = "3f2b8c1e-5d4a-4e7b-9c6d-1a2b3c4d5e6f";
pub const PLACEHOLDER: &str = "openshell:resolve:env:CODEX_AUTH_ACCESS_TOKEN";

#[derive(Debug)]
pub struct Rig {
    pub world: World,
    pub repo: RepoFixture,
    pub config: Config,
    pub project: ProjectRoot,
    pub loaded: LoadedRole,
    pub home: HostPath,
    limits: LaunchLimits,
    _role: RoleFixture,
    _home: TempDir,
}

#[derive(Debug)]
pub struct Outcome {
    pub result: Result<LaunchOutcome, LaunchFailure>,
    pub events: Vec<LaunchEvent>,
}

pub const ENGINE_PORT: u16 = 2376;
pub const KEY_MARKER: &str = "KEYMARKERc2VjcmV0LWVuZ2luZS1rZXk";

pub fn engine_pem(cert: EngineCert) -> String {
    let (label, body) = match cert {
        EngineCert::Ca => ("CERTIFICATE", "Q0FDQ0FDQ0FDQ0FDQ0FDQ0FDQ0FDQ0E="),
        EngineCert::Cert => ("CERTIFICATE", "Q0VSVENFUlRDRVJUQ0VSVENFUlRDRVI="),
        EngineCert::Key => ("PRIVATE KEY", KEY_MARKER),
    };
    format!("-----BEGIN {label}-----\n{body}\n-----END {label}-----\n")
}

pub struct EngineCertificateSet {
    pub ca: String,
    pub cert: String,
    pub key: String,
}

impl std::fmt::Debug for EngineCertificateSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("EngineCertificateSet(<redacted>)")
    }
}

impl EngineCertificateSet {
    pub fn generate() -> Result<EngineCertificateSet, FixtureError> {
        let key = KeyPair::generate()?;
        let mut params = CertificateParams::new(vec!["localhost".to_owned()])?;
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        let certificate = params.self_signed(&key)?;
        let pem = certificate.pem();
        Ok(EngineCertificateSet {
            ca: pem.clone(),
            cert: pem,
            key: key.serialize_pem(),
        })
    }

    pub fn certificate_for_key(key_pem: &str) -> Result<String, FixtureError> {
        let key = KeyPair::from_pem(key_pem)?;
        let params = CertificateParams::new(vec!["rotated.local".to_owned()])?;
        Ok(params.self_signed(&key)?.pem())
    }

    pub fn pem(&self, cert: EngineCert) -> &str {
        match cert {
            EngineCert::Ca => &self.ca,
            EngineCert::Cert => &self.cert,
            EngineCert::Key => &self.key,
        }
    }
}

#[derive(Debug)]
pub struct RigBuilder {
    engine: Option<u16>,
    workspace_docker: bool,
    role_docker: bool,
    limits: LaunchLimits,
    repo: Option<RepoFixture>,
    role: Option<RoleFixture>,
    login: bool,
    printenv_exit: i32,
    preflight_exit: i32,
}

pub fn launch_id(byte: u8) -> LaunchId {
    LaunchId::from_entropy(&[byte; 26])
}

fn script(exit_code: i32, stdout: &[u8]) -> ProgramScript {
    ProgramScript {
        exit_code,
        stdout: stdout.to_vec(),
        stderr: Vec::new(),
    }
}

fn login_text() -> String {
    format!(
        "{{\"auth_mode\":\"chatgpt\",\"OPENAI_API_KEY\":null,\"tokens\":{{\"id_token\":\"header.payload.signature\",\"access_token\":\"{ACCESS}\",\"refresh_token\":\"{REFRESH}\",\"oauth_provenance\":{{\"issuer\":\"https://auth.openai.com\",\"client_id\":\"app_EMoamEEZ73f0CkXaXp7hrann\"}},\"login_owner_id\":\"abcdefghijklmnopqrstuvwxyz0123456789ABCDEFG\",\"account_id\":\"{ACCOUNT_ID}\"}},\"last_refresh\":\"2026-11-02T10:11:12.123456789Z\"}}"
    )
}

fn default_role() -> Result<RoleFixture, FixtureError> {
    Ok(RoleFixture::new("sentinel")?
        .env("GREETING", Some("hello"), Some("Greeting"))
        .hook(HookKind::Setup, b"#!/bin/sh\n")
        .hook(HookKind::Preflight, b"#!/bin/sh\n"))
}

async fn committed_repo() -> Result<RepoFixture, FixtureError> {
    let repo = RepoFixture::new().await?;
    repo.file("src/main.rs", b"fn main() {}\n").await?;
    repo.file("README.md", b"readme\n").await?;
    repo.ignore("target/\n").await?;
    repo.commit().await?;
    Ok(repo)
}

impl Rig {
    pub fn builder() -> RigBuilder {
        RigBuilder {
            engine: None,
            workspace_docker: false,
            role_docker: false,
            limits: default_limits(),
            repo: None,
            role: None,
            login: true,
            printenv_exit: 0,
            preflight_exit: 0,
        }
    }

    pub async fn new() -> Result<Rig, FixtureError> {
        Rig::builder().build().await
    }

    pub fn gateway(&self) -> &Arc<FakeGateway> {
        &self.world.gateway
    }

    pub fn request(&self, id: u8) -> Result<LaunchRequest, FixtureError> {
        Ok(LaunchRequest::new(
            self.world.gateway.name().clone(),
            self.project.clone(),
            RoleName::parse("sentinel")?,
            launch_id(id),
        ))
    }

    pub fn plan(&self, id: u8) -> Result<LaunchPlan, FixtureError> {
        Ok(plan_launch(&self.config, &self.loaded, self.request(id)?)?)
    }

    pub fn engine_certs(&self) -> Result<HostPath, FixtureError> {
        Ok(HostPath::parse(&self.home.as_path().join("engine-certs"))?)
    }

    pub async fn write_engine_certs(&self) -> Result<(), FixtureError> {
        let directory = self.engine_certs()?;
        tokio::fs::create_dir_all(directory.as_path()).await?;
        let certificates = EngineCertificateSet::generate()?;
        for cert in EngineCert::ALL {
            tokio::fs::write(
                directory.as_path().join(cert.file_name()),
                certificates.pem(cert),
            )
            .await?;
        }
        Ok(())
    }

    pub fn sandbox_name(&self, id: u8) -> Result<SandboxName, FixtureError> {
        Ok(self.plan(id)?.sandbox_name().clone())
    }

    pub async fn launch(&self, id: u8) -> Result<LaunchOutcome, FixtureError> {
        self.launch_with(id, CancellationToken::new())
            .await?
            .map_err(|failure| format!("the launch failed: {failure:?}").into())
    }

    pub async fn launch_with(
        &self,
        id: u8,
        cancel: CancellationToken,
    ) -> Result<Result<LaunchOutcome, LaunchFailure>, FixtureError> {
        Ok(self.run(self.request(id)?, cancel).await?.result)
    }

    pub async fn launch_with_env(
        &self,
        id: u8,
        env: BTreeMap<EnvName, String>,
    ) -> Result<LaunchOutcome, FixtureError> {
        let env = env
            .into_iter()
            .map(|(name, value)| Ok((name, PlainEnvValue::parse(&value)?)))
            .collect::<Result<BTreeMap<_, _>, FixtureError>>()?;
        self.run(self.request(id)?.env(env), CancellationToken::new())
            .await?
            .result
            .map_err(|failure| format!("the launch failed: {failure:?}").into())
    }

    pub async fn launch_observed(&self, id: u8) -> Result<Outcome, FixtureError> {
        self.run(self.request(id)?, CancellationToken::new()).await
    }

    pub async fn run(
        &self,
        request: LaunchRequest,
        cancel: CancellationToken,
    ) -> Result<Outcome, FixtureError> {
        self.run_on(&self.world.session, request, cancel).await
    }

    pub async fn run_on(
        &self,
        session: &Session,
        request: LaunchRequest,
        cancel: CancellationToken,
    ) -> Result<Outcome, FixtureError> {
        let plan = plan_launch(&self.config, &self.loaded, request)?;
        Ok(self.run_planned(session, plan, cancel).await)
    }

    pub async fn run_planned(
        &self,
        session: &Session,
        plan: LaunchPlan,
        cancel: CancellationToken,
    ) -> Outcome {
        let (events, mut feed) = launch_events(LineDelivery::Reliable);
        let collect = async {
            let mut seen = Vec::new();
            while let Some(event) = feed.recv().await {
                seen.push(event);
            }
            seen
        };
        let (result, events) = tokio::join!(session.launch(plan, events, cancel), collect);
        Outcome { result, events }
    }

    pub fn session_with(&self, limits: LaunchLimits) -> Session {
        self.session_of(limits, Arc::<TestRunner>::clone(&self.world.runner))
    }

    pub fn session_over(&self, runner: Arc<dyn ProcessRunner>) -> Session {
        self.session_of(self.limits, runner)
    }

    fn session_of(&self, limits: LaunchLimits, runner: Arc<dyn ProcessRunner>) -> Session {
        let world = &self.world;
        let gateway: Arc<dyn Gateway> = Arc::<FakeGateway>::clone(&world.gateway);
        let engine: Arc<dyn ImageEngine> = Arc::<FakeEngine>::clone(&world.engine);
        let clock: Arc<dyn Clock> = Arc::<FakeClock>::clone(&world.clock);
        let fs: Arc<dyn FileSystem> = Arc::clone(&world.fs);
        Session::new(SessionDeps {
            gateway,
            engine,
            host: HostPorts { clock, fs, runner },
            cache_dir: world.cache_dir().clone(),
            config: world.config.clone(),
            limits,
        })
    }

    pub async fn sandbox(&self, id: u8) -> Result<Option<SandboxHandle>, FixtureError> {
        let found = self
            .world
            .gateway
            .find_sandboxes(
                &SandboxQuery::Labels(LabelSelector::launch(&launch_id(id))),
                &self.world.cancel,
            )
            .await?;
        Ok(found
            .first()
            .and_then(|view| self.world.gateway.sandbox(&view.name)))
    }

    pub async fn handle(&self, id: u8) -> Result<SandboxHandle, FixtureError> {
        self.sandbox(id)
            .await?
            .ok_or_else(|| "the launch has no sandbox".into())
    }

    pub fn stages(&self) -> Vec<BuildStage> {
        self.world
            .engine
            .builds()
            .iter()
            .map(|build| build.stage)
            .collect()
    }

    pub fn final_image(&self) -> Result<BuiltImage, FixtureError> {
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
            source: ImageSource::Built,
        })
    }
}

impl RigBuilder {
    #[must_use]
    pub fn limits(mut self, limits: LaunchLimits) -> RigBuilder {
        self.limits = limits;
        self
    }

    #[must_use]
    pub fn repo(mut self, repo: RepoFixture) -> RigBuilder {
        self.repo = Some(repo);
        self
    }

    #[must_use]
    pub fn role(mut self, role: RoleFixture) -> RigBuilder {
        self.role = Some(role);
        self
    }

    #[must_use]
    pub fn engine(mut self, port: u16) -> RigBuilder {
        self.engine = Some(port);
        self
    }

    #[must_use]
    pub fn workspace_docker(mut self, enabled: bool) -> RigBuilder {
        self.workspace_docker = enabled;
        self
    }

    #[must_use]
    pub fn role_docker(mut self, enabled: bool) -> RigBuilder {
        self.role_docker = enabled;
        self
    }

    #[must_use]
    pub fn login(mut self, login: bool) -> RigBuilder {
        self.login = login;
        self
    }

    #[must_use]
    pub fn printenv_exit(mut self, code: i32) -> RigBuilder {
        self.printenv_exit = code;
        self
    }

    #[must_use]
    pub fn preflight_exit(mut self, code: i32) -> RigBuilder {
        self.preflight_exit = code;
        self
    }

    pub async fn build(self) -> Result<Rig, FixtureError> {
        let limits = self.limits;
        let world = World::builder().limits(limits).build()?;
        let repo = match self.repo {
            Some(repo) => repo,
            None => committed_repo().await?,
        };
        let role = match self.role {
            Some(role) => role,
            None => default_role()?,
        }
        .docker(self.role_docker);
        role.commit().await?;
        let loaded = world
            .roles
            .load_path(role.root(), &CancellationToken::new())
            .await?;
        let home_dir = tempfile::tempdir()?;
        let home = HostPath::parse(&std::fs::canonicalize(home_dir.path())?)?;
        if self.login {
            tokio::fs::write(home.as_path().join("auth.json"), login_text()).await?;
        }
        let engine = self.engine.map_or_else(String::new, |port| {
            format!(
                "\n[gateways.fake-gateway.docker_engine]\nport = {port}\nclient_certs = \"{}/engine-certs\"\n",
                home.as_path().display()
            )
        });
        let workspace_docker = if self.workspace_docker {
            "docker = true\n"
        } else {
            ""
        };
        let config = Config::parse(&format!(
            "default_gateway = \"fake-gateway\"\n\n[gateways.fake-gateway]\nopenshell_workspace = \"default\"\nbuilder = \"jackin-builder\"\nplatform = \"linux/arm64\"\n{engine}\n[accounts.work]\nprovider = \"codex\"\ncredential = {{ source = \"codex-login\", home = \"{}\", chatgpt_account_id = \"{ACCOUNT_ID}\" }}\n\n[roles.sentinel]\nsource = {{ path = \"{}\" }}\n\n[workspaces.platform-backend]\npath = \"{}\"\n{workspace_docker}",
            home.as_path().display(),
            role.root().as_path().display(),
            repo.root().as_path().display(),
        ))?;
        world.seed_config(&config).await?;
        world.seed_workspaces(&config).await?;
        let project = repo.project().await?;
        let gateway = &world.gateway;
        gateway.script_program(
            &sandbox_layout::PRINTENV,
            script(self.printenv_exit, format!("{PLACEHOLDER}\n").as_bytes()),
        );
        for (kind, exit) in [
            (HookKind::Setup, 0),
            (HookKind::Preflight, self.preflight_exit),
        ] {
            let hook = sandbox_layout::hook_path(kind);
            if loaded.manifest().hook(kind).is_some() {
                gateway.install_file(&hook, b"#!/bin/sh\n".to_vec());
                gateway.script_program(&hook, script(exit, b""));
            }
        }
        Ok(Rig {
            world,
            repo,
            config,
            project,
            loaded,
            home,
            limits,
            _role: role,
            _home: home_dir,
        })
    }
}
