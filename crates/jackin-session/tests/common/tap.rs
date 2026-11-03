// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::os::unix::fs::PermissionsExt;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use jackin_core::{
    Clock, FileMode, FileSystem, GatewayName, HostPath, ProcessRunner, ProfileId, ProjectRoot,
    ProviderName, SandboxName, SandboxPath, SecretBytes, Sha256,
};
use jackin_image::ImageEngine;
use jackin_openshell::{
    Gateway, GatewayError, GatewayHealth, GatewayOp, PowerAction, ProfileSync, ProgramOutcome,
    ProgramRun, ProviderProfile, ProviderSpec, ProviderState, SandboxCreate, SandboxDetail,
    SandboxFile, SandboxQuery, SandboxView, TerminalRequest, TerminalSession,
};
use jackin_session::{HostPorts, LaunchLimits, Session, SessionDeps};
use jackin_testkit::{FakeClock, FakeEngine, FakeGateway, Rig, TestRunner};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

const RM: &str = "/usr/bin/rm";

#[derive(Clone, Debug)]
pub(crate) struct Gate {
    pub(crate) entered: Arc<Notify>,
    pub(crate) release: Arc<Notify>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RmRun {
    pub(crate) timeout: Duration,
    pub(crate) cancelled: bool,
}

#[derive(Debug, Default)]
struct Hooks {
    downloads: Vec<Vec<u8>>,
    after: Vec<(SandboxPath, SandboxFile)>,
    gates: Vec<(SandboxPath, Gate)>,
    rm_clock: Option<Arc<dyn Clock>>,
    rm_runs: Vec<RmRun>,
    download_dir_modes: Vec<u32>,
}

#[derive(Debug)]
pub(crate) struct Tap {
    inner: Arc<FakeGateway>,
    hooks: Mutex<Hooks>,
}

impl Tap {
    pub(crate) fn new(rig: &Rig) -> Arc<Tap> {
        Arc::new(Tap {
            inner: Arc::clone(&rig.world.gateway),
            hooks: Mutex::new(Hooks::default()),
        })
    }

    fn hooks(&self) -> MutexGuard<'_, Hooks> {
        self.hooks.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn session(self: &Arc<Tap>, rig: &Rig, limits: LaunchLimits) -> Session {
        let runner: Arc<dyn ProcessRunner> = Arc::<TestRunner>::clone(&rig.world.runner);
        self.session_over(rig, limits, runner)
    }

    pub(crate) fn session_over(
        self: &Arc<Tap>,
        rig: &Rig,
        limits: LaunchLimits,
        runner: Arc<dyn ProcessRunner>,
    ) -> Session {
        let fs: Arc<dyn FileSystem> = Arc::clone(&rig.world.fs);
        self.session_over_fs(rig, limits, runner, fs)
    }

    pub(crate) fn session_over_fs(
        self: &Arc<Tap>,
        rig: &Rig,
        limits: LaunchLimits,
        runner: Arc<dyn ProcessRunner>,
        fs: Arc<dyn FileSystem>,
    ) -> Session {
        let world = &rig.world;
        let gateway: Arc<dyn Gateway> = Arc::<Tap>::clone(self);
        let engine: Arc<dyn ImageEngine> = Arc::<FakeEngine>::clone(&world.engine);
        let clock: Arc<dyn Clock> = Arc::<FakeClock>::clone(&world.clock);
        Session::new(SessionDeps {
            gateway,
            engine,
            host: HostPorts { clock, fs, runner },
            cache_dir: world.cache_dir().clone(),
            config: world.config.clone(),
            limits,
        })
    }

    pub(crate) fn replace_next_download(&self, bytes: Vec<u8>) {
        self.hooks().downloads.push(bytes);
    }

    pub(crate) fn after_program(&self, program: &SandboxPath, then_write: SandboxFile) {
        self.hooks().after.push((program.clone(), then_write));
    }

    pub(crate) fn gate_program(&self, program: &SandboxPath) -> Gate {
        let gate = Gate {
            entered: Arc::new(Notify::new()),
            release: Arc::new(Notify::new()),
        };
        self.hooks().gates.push((program.clone(), gate.clone()));
        gate
    }

    pub(crate) fn stall_rm(&self, clock: Arc<dyn Clock>) {
        self.hooks().rm_clock = Some(clock);
    }

    pub(crate) fn download_dir_modes(&self) -> Vec<u32> {
        self.hooks().download_dir_modes.clone()
    }

    pub(crate) fn rm_runs(&self) -> Vec<RmRun> {
        self.hooks().rm_runs.clone()
    }
}

pub(crate) fn file(path: &SandboxPath, bytes: &[u8]) -> SandboxFile {
    SandboxFile {
        path: path.clone(),
        mode: FileMode(0o600),
        content: SecretBytes::new(bytes.to_vec()),
    }
}

#[async_trait]
impl Gateway for Tap {
    fn name(&self) -> &GatewayName {
        self.inner.name()
    }

    async fn health(&self) -> Result<GatewayHealth, GatewayError> {
        self.inner.health().await
    }

    async fn ensure_profile(&self, profile: &ProviderProfile) -> Result<ProfileSync, GatewayError> {
        self.inner.ensure_profile(profile).await
    }

    async fn delete_profile(&self, id: &ProfileId) -> Result<(), GatewayError> {
        self.inner.delete_profile(id).await
    }

    async fn provider_state(&self, name: &ProviderName) -> Result<ProviderState, GatewayError> {
        self.inner.provider_state(name).await
    }

    async fn put_provider(&self, spec: &ProviderSpec) -> Result<(), GatewayError> {
        self.inner.put_provider(spec).await
    }

    async fn delete_provider(&self, name: &ProviderName) -> Result<(), GatewayError> {
        self.inner.delete_provider(name).await
    }

    async fn create_sandbox(&self, spec: &SandboxCreate) -> Result<SandboxView, GatewayError> {
        self.inner.create_sandbox(spec).await
    }

    async fn find_sandboxes(&self, query: &SandboxQuery) -> Result<Vec<SandboxView>, GatewayError> {
        self.inner.find_sandboxes(query).await
    }

    async fn sandbox_detail(&self, name: &SandboxName) -> Result<SandboxDetail, GatewayError> {
        self.inner.sandbox_detail(name).await
    }

    async fn sandbox_environment(
        &self,
        name: &SandboxName,
    ) -> Result<std::collections::BTreeMap<jackin_core::EnvName, String>, GatewayError> {
        self.inner.sandbox_environment(name).await
    }

    async fn set_power(
        &self,
        name: &SandboxName,
        action: PowerAction,
    ) -> Result<SandboxView, GatewayError> {
        self.inner.set_power(name, action).await
    }

    async fn delete_sandbox(&self, name: &SandboxName) -> Result<(), GatewayError> {
        self.inner.delete_sandbox(name).await
    }

    async fn run_program(
        &self,
        run: &ProgramRun,
        cancel: &CancellationToken,
    ) -> Result<ProgramOutcome, GatewayError> {
        if run.program.as_str() == RM {
            let clock = self.hooks().rm_clock.clone();
            if let Some(clock) = clock {
                self.hooks().rm_runs.push(RmRun {
                    timeout: run.timeout,
                    cancelled: cancel.is_cancelled(),
                });
                clock.sleep(run.timeout).await;
                return Err(GatewayError::Timeout {
                    op: GatewayOp::Program,
                    after: run.timeout,
                    group: None,
                });
            }
        }
        let gate = {
            let mut hooks = self.hooks();
            hooks
                .gates
                .iter()
                .position(|(program, _)| *program == run.program)
                .map(|at| hooks.gates.remove(at).1)
        };
        if let Some(gate) = gate {
            gate.entered.notify_one();
            gate.release.notified().await;
        }
        let outcome = self.inner.run_program(run, cancel).await;
        let follow = {
            let mut hooks = self.hooks();
            hooks
                .after
                .iter()
                .position(|(program, _)| *program == run.program)
                .map(|at| hooks.after.remove(at).1)
        };
        if let Some(file) = follow {
            self.inner.write_file(&run.sandbox, &file).await?;
        }
        outcome
    }

    async fn read_files(
        &self,
        sandbox: &SandboxName,
        paths: &[SandboxPath],
    ) -> Result<Vec<Option<Vec<u8>>>, GatewayError> {
        self.inner.read_files(sandbox, paths).await
    }

    async fn write_file(
        &self,
        sandbox: &SandboxName,
        file: &SandboxFile,
    ) -> Result<(), GatewayError> {
        self.inner.write_file(sandbox, file).await
    }

    async fn upload_tree(
        &self,
        sandbox: &SandboxName,
        source: &ProjectRoot,
        dest: &SandboxPath,
        cancel: &CancellationToken,
    ) -> Result<(), GatewayError> {
        self.inner.upload_tree(sandbox, source, dest, cancel).await
    }

    async fn upload_file(
        &self,
        sandbox: &SandboxName,
        source: &HostPath,
        dest: &SandboxPath,
        cancel: &CancellationToken,
    ) -> Result<(), GatewayError> {
        self.inner.upload_file(sandbox, source, dest, cancel).await
    }

    async fn download_file(
        &self,
        sandbox: &SandboxName,
        remote: &SandboxPath,
        local: &HostPath,
        cancel: &CancellationToken,
    ) -> Result<Sha256, GatewayError> {
        let replacement = {
            let mut hooks = self.hooks();
            if let Some(mode) = local
                .as_path()
                .parent()
                .and_then(|dir| std::fs::metadata(dir).ok())
                .map(|meta| meta.permissions().mode() & 0o777)
            {
                hooks.download_dir_modes.push(mode);
            }
            if hooks.downloads.is_empty() {
                None
            } else {
                Some(hooks.downloads.remove(0))
            }
        };
        if let Some(bytes) = replacement {
            self.inner
                .write_file(sandbox, &file(remote, &bytes))
                .await?;
        }
        self.inner
            .download_file(sandbox, remote, local, cancel)
            .await
    }

    async fn open_terminal(
        &self,
        request: &TerminalRequest,
    ) -> Result<TerminalSession, GatewayError> {
        self.inner.open_terminal(request).await
    }
}
