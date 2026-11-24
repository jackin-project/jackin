// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use jackin_core::{
    EnvName, GatewayName, HostPath, ProfileId, ProjectRoot, ProviderName, SandboxName, SandboxPath,
    Sha256,
};
use jackin_openshell::{
    Gateway, GatewayError, GatewayHealth, PowerAction, ProfileSync, ProgramOutcome, ProgramRun,
    ProviderProfile, ProviderSpec, ProviderState, SandboxCreate, SandboxDetail, SandboxFile,
    SandboxQuery, SandboxView, TerminalRequest, TerminalSession,
};
use jackin_testkit::FakeGateway;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

#[derive(Debug)]
pub(crate) struct Probe {
    inner: Arc<FakeGateway>,
    active: Mutex<BTreeMap<SandboxName, usize>>,
    peak: AtomicUsize,
    stalled: AtomicBool,
    listing: AtomicBool,
    only: Mutex<Vec<SandboxName>>,
    entered: Notify,
    pause: Mutex<Duration>,
}

impl Probe {
    pub(crate) fn over(inner: &Arc<FakeGateway>) -> Arc<Probe> {
        Arc::new(Probe {
            inner: Arc::clone(inner),
            active: Mutex::new(BTreeMap::new()),
            peak: AtomicUsize::new(0),
            stalled: AtomicBool::new(false),
            listing: AtomicBool::new(false),
            only: Mutex::new(Vec::new()),
            entered: Notify::new(),
            pause: Mutex::new(Duration::ZERO),
        })
    }

    pub(crate) fn stall(&self) {
        self.stalled.store(true, Ordering::SeqCst);
    }

    pub(crate) fn stall_listing(&self) {
        self.listing.store(true, Ordering::SeqCst);
    }

    pub(crate) fn stall_only(&self, sandboxes: &[SandboxName]) {
        *self.only.lock().unwrap_or_else(PoisonError::into_inner) = sandboxes.to_vec();
    }

    pub(crate) async fn until_stalled(&self) {
        self.entered.notified().await;
    }

    pub(crate) fn pause(&self, pause: Duration) {
        *self.pause.lock().unwrap_or_else(PoisonError::into_inner) = pause;
    }

    pub(crate) fn peak(&self) -> usize {
        self.peak.load(Ordering::SeqCst)
    }

    async fn observed<T>(&self, sandbox: &SandboxName, call: impl Future<Output = T>) -> T {
        {
            let mut active = self.active.lock().unwrap_or_else(PoisonError::into_inner);
            *active.entry(sandbox.clone()).or_default() += 1;
            self.peak.fetch_max(active.len(), Ordering::SeqCst);
        }
        let _leave = Leave {
            probe: self,
            sandbox: sandbox.clone(),
        };
        let pause = *self.pause.lock().unwrap_or_else(PoisonError::into_inner);
        let only = self
            .only
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if self.stalled.load(Ordering::SeqCst) || only.contains(sandbox) {
            self.entered.notify_one();
            std::future::pending::<()>().await;
        }
        if !pause.is_zero() {
            tokio::time::sleep(pause).await;
        }
        call.await
    }
}

struct Leave<'a> {
    probe: &'a Probe,
    sandbox: SandboxName,
}

impl Drop for Leave<'_> {
    fn drop(&mut self) {
        let mut active = self
            .probe
            .active
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(count) = active.get_mut(&self.sandbox) {
            *count -= 1;
            if *count == 0 {
                active.remove(&self.sandbox);
            }
        }
    }
}

#[async_trait]
impl Gateway for Probe {
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
        if self.listing.load(Ordering::SeqCst) {
            self.entered.notify_one();
            std::future::pending::<()>().await;
        }
        self.inner.find_sandboxes(query).await
    }

    async fn sandbox_detail(&self, name: &SandboxName) -> Result<SandboxDetail, GatewayError> {
        self.observed(name, self.inner.sandbox_detail(name)).await
    }

    async fn sandbox_environment(
        &self,
        name: &SandboxName,
    ) -> Result<BTreeMap<EnvName, String>, GatewayError> {
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
        self.observed(&run.sandbox, self.inner.run_program(run, cancel))
            .await
    }

    async fn read_files(
        &self,
        sandbox: &SandboxName,
        paths: &[SandboxPath],
    ) -> Result<Vec<Option<Vec<u8>>>, GatewayError> {
        self.observed(sandbox, self.inner.read_files(sandbox, paths))
            .await
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
        self.observed(
            sandbox,
            self.inner.download_file(sandbox, remote, local, cancel),
        )
        .await
    }

    async fn open_terminal(
        &self,
        request: &TerminalRequest,
    ) -> Result<TerminalSession, GatewayError> {
        self.inner.open_terminal(request).await
    }
}
