// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use jackin_core::{
    EnvName, GatewayName, HostPath, OpenShellWorkspace, ProcessRunner, ProfileId, ProjectRoot,
    ProviderName, SandboxName, SandboxPath, Sha256,
};
use openshell_sdk::OpenShellClient;
use openshell_sdk::raw::AuthedGrpcClient;
use openshell_sdk::raw::proto::{WorkspaceSelector, workspace_selector};
use tokio_util::sync::CancellationToken;

use crate::discovery::ResolvedGateway;
use crate::error::{GatewayError, GatewayOp};
use crate::gateway::{
    Gateway, GatewayHealth, PowerAction, ProfileSync, ProgramOutcome, ProgramRun, ProviderSpec,
    ProviderState, SandboxCreate, SandboxDetail, SandboxFile, SandboxQuery, SandboxView,
};
use crate::policy::ProviderProfile;
use crate::terminal::{TerminalRequest, TerminalSession};

mod connect;
mod main_pty;
mod program;
mod provider;
mod sandbox;
mod shell;
mod status;

#[derive(Clone, Debug)]
pub struct GrpcGatewayConfig {
    pub gateway: ResolvedGateway,
    pub workspace: OpenShellWorkspace,
    pub runner: Arc<dyn ProcessRunner>,
    pub transfer_timeout: Duration,
    pub call_timeout: Duration,
}

#[derive(Clone)]
pub struct GrpcGateway {
    inner: Arc<Inner>,
}

pub(crate) struct Inner {
    pub(crate) name: GatewayName,
    pub(crate) endpoint: String,
    pub(crate) workspace: OpenShellWorkspace,
    pub(crate) client: OpenShellClient,
    pub(crate) runner: Arc<dyn ProcessRunner>,
    pub(crate) transfer_timeout: Duration,
    pub(crate) call_timeout: Duration,
}

impl fmt::Debug for GrpcGateway {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GrpcGateway")
            .field("name", &self.inner.name)
            .field("endpoint", &self.inner.endpoint)
            .field("workspace", &self.inner.workspace)
            .finish_non_exhaustive()
    }
}

impl Inner {
    pub(crate) fn grpc(&self) -> AuthedGrpcClient {
        self.client.raw_grpc()
    }

    pub(crate) fn scope(&self) -> WorkspaceSelector {
        workspace_selector(self.workspace.as_str())
    }

    pub(crate) async fn unary<T>(
        &self,
        op: GatewayOp,
        missing: Option<crate::error::Missing>,
        call: impl Future<Output = Result<tonic::Response<T>, tonic::Status>>,
    ) -> Result<T, GatewayError> {
        match tokio::time::timeout(self.call_timeout, call).await {
            Err(_) => Err(GatewayError::Timeout {
                op,
                after: self.call_timeout,
            }),
            Ok(Ok(response)) => Ok(response.into_inner()),
            Ok(Err(status)) => Err(self.map_status(op, missing, &status)),
        }
    }
}

#[async_trait]
impl Gateway for GrpcGateway {
    fn name(&self) -> &GatewayName {
        &self.inner.name
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
        self.inner.run_program(run, cancel).await
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
        crate::transfer::upload_tree(&self.inner, sandbox, source, dest, cancel).await
    }

    async fn upload_file(
        &self,
        sandbox: &SandboxName,
        source: &HostPath,
        dest: &SandboxPath,
        cancel: &CancellationToken,
    ) -> Result<(), GatewayError> {
        crate::transfer::upload_file(&self.inner, sandbox, source, dest, cancel).await
    }

    async fn download_file(
        &self,
        sandbox: &SandboxName,
        remote: &SandboxPath,
        local: &HostPath,
        cancel: &CancellationToken,
    ) -> Result<Sha256, GatewayError> {
        crate::transfer::download_file(&self.inner, sandbox, remote, local, cancel).await
    }

    async fn open_terminal(
        &self,
        request: &TerminalRequest,
    ) -> Result<TerminalSession, GatewayError> {
        self.inner.open_terminal(request).await
    }
}
