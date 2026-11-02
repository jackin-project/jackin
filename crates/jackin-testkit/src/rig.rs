// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_core::{
    Config, HookKind, HostPath, LabelSelector, LaunchId, ProjectRoot, RoleName, SandboxName,
    sandbox_layout,
};
use jackin_openshell::{Gateway, SandboxQuery};
use jackin_session::{
    LaunchFailure, LaunchLimits, LaunchOutcome, LaunchPlan, LaunchRequest, LineDelivery,
    LoadedRole, launch_events, plan_launch,
};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

use crate::fixtures::{FixtureError, RepoFixture, RoleFixture};
use crate::gateway::{ProgramScript, SandboxHandle};
use crate::world::{World, default_limits};

pub type TestResult = Result<(), FixtureError>;

const GUARD: Duration = Duration::from_secs(10);

pub async fn guard(work: impl Future<Output = TestResult>) -> TestResult {
    tokio::time::timeout(GUARD, work)
        .await
        .map_err(|_| "the test hung for 10 s")?
}

const ACCOUNT_ID: &str = "3f2b8c1e-5d4a-4e7b-9c6d-1a2b3c4d5e6f";
const PLACEHOLDER: &str = "openshell:resolve:env:CODEX_AUTH_ACCESS_TOKEN";

#[derive(Debug)]
pub struct Rig {
    pub world: World,
    pub repo: RepoFixture,
    pub config: Config,
    pub project: ProjectRoot,
    loaded: LoadedRole,
    _role: RoleFixture,
    _home: TempDir,
}

#[derive(Debug)]
pub struct RigBuilder {
    limits: LaunchLimits,
    repo: Option<RepoFixture>,
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
        "{{\"auth_mode\":\"chatgpt\",\"OPENAI_API_KEY\":null,\"tokens\":{{\"id_token\":\"header.payload.signature\",\"access_token\":\"ACCESS-SECRET-7f3a91\",\"refresh_token\":\"REFRESH-SECRET-c20e44\",\"oauth_provenance\":{{\"issuer\":\"https://auth.openai.com\",\"client_id\":\"app_EMoamEEZ73f0CkXaXp7hrann\"}},\"login_owner_id\":\"abcdefghijklmnopqrstuvwxyz0123456789ABCDEFG\",\"account_id\":\"{ACCOUNT_ID}\"}},\"last_refresh\":\"2026-11-02T10:11:12.123456789Z\"}}"
    )
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
            limits: default_limits(),
            repo: None,
            printenv_exit: 0,
            preflight_exit: 0,
        }
    }

    pub async fn new() -> Result<Rig, FixtureError> {
        Rig::builder().build().await
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
        let plan = self.plan(id)?;
        let (events, mut feed) = launch_events(LineDelivery::Reliable);
        let drain = async { while feed.recv().await.is_some() {} };
        let (result, ()) = tokio::join!(self.world.session.launch(plan, events, cancel), drain);
        Ok(result)
    }

    pub async fn handle(&self, id: u8) -> Result<SandboxHandle, FixtureError> {
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
        let world = World::builder().limits(self.limits).build()?;
        let repo = match self.repo {
            Some(repo) => repo,
            None => committed_repo().await?,
        };
        let role = RoleFixture::new("sentinel")?
            .env("GREETING", Some("hello"), Some("Greeting"))
            .hook(HookKind::Setup, b"#!/bin/sh\n")
            .hook(HookKind::Preflight, b"#!/bin/sh\n");
        role.commit().await?;
        let loaded = world.roles.load_path(role.root()).await?;
        let home_dir = tempfile::tempdir()?;
        let home = HostPath::parse(&std::fs::canonicalize(home_dir.path())?)?;
        tokio::fs::write(home.as_path().join("auth.json"), login_text()).await?;
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
            script(self.printenv_exit, format!("{PLACEHOLDER}\n").as_bytes()),
        );
        for (kind, exit) in [
            (HookKind::Setup, 0),
            (HookKind::Preflight, self.preflight_exit),
        ] {
            let hook = sandbox_layout::hook_path(kind);
            gateway.install_file(&hook, b"#!/bin/sh\n".to_vec());
            gateway.script_program(&hook, script(exit, b""));
        }
        Ok(Rig {
            world,
            repo,
            config,
            project,
            loaded,
            _role: role,
            _home: home_dir,
        })
    }
}
