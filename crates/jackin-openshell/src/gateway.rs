// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fmt::Debug;
use std::time::Duration;

use async_trait::async_trait;
use jackin_core::{
    ChatgptAccountId, CredentialKeys, EnvName, FileMode, GatewayName, HostPath, ImageTag,
    LabelSelector, Labels, LocalImageRef, Phase, ProfileId, ProjectRoot, ProviderName,
    SandboxLabels, SandboxName, SandboxPath, Secret, SecretBytes, Sha256, TerminalText, Timestamp,
};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::error::GatewayError;
use crate::policy::{ClosedPolicy, PolicyDocument, ProviderProfile};
use crate::terminal::{TerminalRequest, TerminalSession};

#[async_trait]
pub trait Gateway: Send + Sync + Debug {
    fn name(&self) -> &GatewayName;

    async fn health(&self) -> Result<GatewayHealth, GatewayError>;

    async fn ensure_profile(&self, profile: &ProviderProfile) -> Result<ProfileSync, GatewayError>;
    async fn delete_profile(&self, id: &ProfileId) -> Result<(), GatewayError>;
    async fn provider_state(&self, name: &ProviderName) -> Result<ProviderState, GatewayError>;
    async fn put_provider(
        &self,
        spec: &ProviderSpec,
    ) -> Result<ProviderWriteEffect, ProviderWriteError>;
    async fn delete_provider(&self, name: &ProviderName) -> Result<(), GatewayError>;

    async fn create_sandbox(&self, spec: &SandboxCreate) -> Result<SandboxView, GatewayError>;
    async fn create_contract_sandbox(
        &self,
        spec: &GatewayContractSandboxCreate,
    ) -> Result<SandboxView, GatewayError>;
    async fn find_sandboxes(
        &self,
        query: &SandboxQuery,
        cancel: &CancellationToken,
    ) -> Result<Vec<SandboxView>, GatewayError>;
    async fn sandbox_detail(&self, name: &SandboxName) -> Result<SandboxDetail, GatewayError>;
    async fn sandbox_environment(
        &self,
        name: &SandboxName,
    ) -> Result<BTreeMap<EnvName, String>, GatewayError>;
    async fn set_power(
        &self,
        name: &SandboxName,
        action: PowerAction,
        cancel: &CancellationToken,
    ) -> Result<SandboxView, GatewayError>;
    async fn delete_sandbox(&self, name: &SandboxName) -> Result<(), GatewayError>;

    async fn run_program(
        &self,
        run: &ProgramRun,
        cancel: &CancellationToken,
    ) -> Result<ProgramOutcome, GatewayError>;
    async fn read_files(
        &self,
        sandbox: &SandboxName,
        paths: &[SandboxPath],
        cancel: &CancellationToken,
    ) -> Result<Vec<Option<Vec<u8>>>, GatewayError>;
    async fn write_file(
        &self,
        sandbox: &SandboxName,
        file: &SandboxFile,
        cancel: &CancellationToken,
    ) -> Result<(), GatewayError>;

    async fn upload_tree(
        &self,
        sandbox: &SandboxName,
        source: &ProjectRoot,
        dest: &SandboxPath,
        cancel: &CancellationToken,
    ) -> Result<(), GatewayError>;
    async fn upload_file(
        &self,
        sandbox: &SandboxName,
        source: &HostPath,
        dest: &SandboxPath,
        cancel: &CancellationToken,
    ) -> Result<(), GatewayError>;
    async fn download_file(
        &self,
        sandbox: &SandboxName,
        remote: &SandboxPath,
        local: &HostPath,
        cancel: &CancellationToken,
    ) -> Result<Sha256, GatewayError>;

    async fn open_terminal(
        &self,
        request: &TerminalRequest,
    ) -> Result<TerminalSession, GatewayError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GatewayHealth {
    pub version: String,
    pub serving: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProfileSync {
    Imported,
    Unchanged,
    Updated,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProviderState {
    Absent,
    Unrefreshed,
    Present,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProviderWriteEffect {
    NoWrite,
    Created,
    Existing,
    Unknown,
}

#[derive(Debug, Error)]
#[error("provider write failed")]
pub struct ProviderWriteError {
    #[source]
    source: GatewayError,
    effect: ProviderWriteEffect,
}

impl ProviderWriteError {
    pub fn new(source: GatewayError, effect: ProviderWriteEffect) -> ProviderWriteError {
        ProviderWriteError { source, effect }
    }

    pub fn effect(&self) -> ProviderWriteEffect {
        self.effect
    }

    pub fn into_parts(self) -> (GatewayError, ProviderWriteEffect) {
        (self.source, self.effect)
    }
}

impl ProviderState {
    pub fn exists(self) -> bool {
        self != ProviderState::Absent
    }
}

#[derive(Debug)]
pub struct ProviderSpec {
    pub name: ProviderName,
    pub profile: ProfileId,
    pub keys: CredentialKeys,
    pub material: ProviderMaterial,
}

#[derive(Debug)]
pub enum ProviderMaterial {
    StaticKey {
        key: Secret,
    },
    ChatgptLogin {
        access_token: Secret,
        refresh_token: Secret,
        client_id: String,
        chatgpt_account_id: ChatgptAccountId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MainProcess {
    pub program: SandboxPath,
    pub args: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct SandboxCreate {
    name: SandboxName,
    image: LocalImageRef,
    labels: SandboxLabels,
    providers: Vec<ProviderName>,
    policy: ClosedPolicy,
    main: MainProcess,
}

const LAUNCH_ENV: &str = "JACKIN_LAUNCH_ID";

impl SandboxCreate {
    pub fn new(
        name: SandboxName,
        image: LocalImageRef,
        labels: SandboxLabels,
        providers: Vec<ProviderName>,
        policy: ClosedPolicy,
        main: MainProcess,
    ) -> SandboxCreate {
        SandboxCreate {
            name,
            image,
            labels,
            providers,
            policy,
            main,
        }
    }

    pub fn name(&self) -> &SandboxName {
        &self.name
    }

    pub fn image(&self) -> &LocalImageRef {
        &self.image
    }

    pub fn labels(&self) -> &SandboxLabels {
        &self.labels
    }

    pub fn providers(&self) -> &[ProviderName] {
        &self.providers
    }

    pub fn policy(&self) -> &PolicyDocument {
        self.policy.document()
    }

    pub fn main(&self) -> &MainProcess {
        &self.main
    }

    pub fn environment(&self) -> BTreeMap<EnvName, String> {
        EnvName::parse(LAUNCH_ENV)
            .map(|name| (name, self.labels.planned.launch.to_string()))
            .into_iter()
            .collect()
    }
}

#[derive(Clone, Debug)]
pub struct GatewayContractSandboxCreate {
    name: SandboxName,
    image: ImageTag,
    labels: SandboxLabels,
    providers: Vec<ProviderName>,
    policy: ClosedPolicy,
    main: MainProcess,
}

impl GatewayContractSandboxCreate {
    pub fn new(
        name: SandboxName,
        image: ImageTag,
        labels: SandboxLabels,
        providers: Vec<ProviderName>,
        policy: ClosedPolicy,
        main: MainProcess,
    ) -> GatewayContractSandboxCreate {
        GatewayContractSandboxCreate {
            name,
            image,
            labels,
            providers,
            policy,
            main,
        }
    }

    pub fn name(&self) -> &SandboxName {
        &self.name
    }

    pub fn image(&self) -> &ImageTag {
        &self.image
    }

    pub fn labels(&self) -> &SandboxLabels {
        &self.labels
    }

    pub fn providers(&self) -> &[ProviderName] {
        &self.providers
    }

    pub fn policy(&self) -> &PolicyDocument {
        self.policy.document()
    }

    pub fn main(&self) -> &MainProcess {
        &self.main
    }

    pub fn environment(&self) -> BTreeMap<EnvName, String> {
        EnvName::parse(LAUNCH_ENV)
            .map(|name| (name, self.labels.planned.launch.to_string()))
            .into_iter()
            .collect()
    }
}

#[derive(Clone, Debug)]
pub struct SandboxView {
    pub name: SandboxName,
    pub phase: Phase,
    pub labels: Labels,
    pub provisioning: Provisioning,
    pub created_at: Option<Timestamp>,
}

#[derive(Clone, Debug)]
pub enum Provisioning {
    Normal,
    Failed { message: TerminalText },
}

#[derive(Clone, Debug)]
pub enum SandboxQuery {
    Name(SandboxName),
    Labels(LabelSelector),
}

#[derive(Clone, Debug)]
pub struct SandboxDetail {
    pub view: SandboxView,
    pub providers: Vec<ProviderName>,
    pub policy: PolicyReport,
}

#[derive(Clone, Debug)]
pub struct PolicyReport {
    pub version: u32,
    pub hash: String,
    pub loaded: bool,
    pub document: PolicyDocument,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PowerAction {
    Start,
    Stop,
}

#[derive(Debug)]
pub struct ProgramRun {
    pub sandbox: SandboxName,
    pub program: SandboxPath,
    pub args: Vec<String>,
    pub workdir: Option<SandboxPath>,
    pub env: BTreeMap<EnvName, String>,
    pub stdin: ProgramStdin,
    pub touch_on_success: Option<SandboxPath>,
    pub timeout: Duration,
}

impl ProgramRun {
    pub fn c_locale() -> BTreeMap<EnvName, String> {
        EnvName::parse("LC_ALL")
            .map(|name| BTreeMap::from([(name, "C".to_owned())]))
            .unwrap_or_default()
    }
}

#[derive(Debug)]
pub enum ProgramStdin {
    Closed,
    Data(SecretBytes),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgramOutcome {
    pub exit_code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[derive(Debug)]
pub struct SandboxFile {
    pub path: SandboxPath,
    pub mode: FileMode,
    pub content: SecretBytes,
}
