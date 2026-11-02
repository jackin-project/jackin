// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod exec;
mod files;
mod handle;
mod main_process;
mod state;
mod terminal;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};

use async_trait::async_trait;
use jackin_core::{
    ChatgptAccountId, EnvName, GatewayName, HostPath, ImageTag, OpenShellWorkspace, Phase,
    ProfileId, ProviderName, SandboxName, SandboxPath,
};
use jackin_openshell::{
    EndpointProtocol, Gateway, GatewayError, GatewayHealth, GatewayOp, Missing, NetworkRule,
    PolicyReport, PowerAction, ProfileSync, ProgramOutcome, ProgramRun, ProjectUpload,
    ProviderMaterial, ProviderProfile, ProviderSpec, ProviderState, RuleEndpoint, RuleKey,
    SandboxCreate, SandboxDetail, SandboxFile, SandboxQuery, SandboxView, TerminalKind,
    TerminalRequest, TerminalSession, TlsMode,
};
use tokio_util::sync::CancellationToken;

pub use handle::SandboxHandle;

use self::main_process::MainPlan;
use self::state::{Action, Armed, ArmedTrigger, Cell, Registry, Sandbox, Settling};
use crate::supervisor::SupervisorModel;
use crate::sync::locked;

#[derive(Debug)]
pub struct FakeGateway {
    name: GatewayName,
    workspace: OpenShellWorkspace,
    registry: Mutex<Registry>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgramScript {
    pub exit_code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunRecord {
    pub args: Vec<String>,
    pub workdir: Option<SandboxPath>,
    pub env: BTreeMap<EnvName, String>,
    pub exit_code: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Trigger {
    Op(GatewayOp),
    Write(SandboxPath),
    Program(SandboxPath),
}

#[derive(Clone, PartialEq, Eq)]
pub struct ProviderRecord {
    pub profile: ProfileId,
    pub access_key: String,
    pub account_id_key: String,
    pub access_token: String,
    pub refresh_token: String,
    pub client_id: String,
    pub chatgpt_account_id: ChatgptAccountId,
}

impl std::fmt::Debug for ProviderRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderRecord")
            .field("profile", &self.profile)
            .field("access_key", &self.access_key)
            .field("account_id_key", &self.account_id_key)
            .field("chatgpt_account_id", &self.chatgpt_account_id)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub enum Fault {
    FailNext {
        op: GatewayOp,
        error: GatewayError,
    },
    FailAfter {
        op: GatewayOp,
        calls: u32,
        error: GatewayError,
    },
    StallProgram {
        program: SandboxPath,
    },
    CorruptDownload,
    ProvisionFor {
        polls: u32,
    },
    ProvisionFails {
        creates: u32,
    },
    StaleSupervisor {
        observations: u32,
    },
    FailAt {
        trigger: Trigger,
        error: GatewayError,
    },
    CancelAt {
        trigger: Trigger,
        token: CancellationToken,
    },
}

fn protocol(message: impl ToString) -> GatewayError {
    GatewayError::Protocol {
        message: message.to_string(),
    }
}

impl FakeGateway {
    pub fn new(name: GatewayName, workspace: OpenShellWorkspace) -> FakeGateway {
        FakeGateway {
            name,
            workspace,
            registry: Mutex::new(Registry::new()),
        }
    }

    pub fn workspace(&self) -> &OpenShellWorkspace {
        &self.workspace
    }

    pub fn inject(&self, fault: Fault) {
        let mut registry = self.registry();
        match fault {
            Fault::FailNext { op, error } => registry.faults.push(Armed::new(op, 0, error)),
            Fault::FailAfter { op, calls, error } => {
                registry.faults.push(Armed::new(op, calls, error));
            }
            Fault::StallProgram { program } => registry.stalls.push(program),
            Fault::CorruptDownload => registry.corrupt_download = true,
            Fault::ProvisionFor { polls } => registry.provision_polls = polls,
            Fault::ProvisionFails { creates } => registry.failing_creates = creates,
            Fault::StaleSupervisor { observations } => registry.stale_observations = observations,
            Fault::FailAt { trigger, error } => registry
                .triggers
                .push(ArmedTrigger::new(trigger, Action::Fail(error))),
            Fault::CancelAt { trigger, token } => registry
                .triggers
                .push(ArmedTrigger::new(trigger, Action::Cancel(token))),
        }
    }

    pub fn force_phase(&self, name: &SandboxName, phase: Phase) {
        if let Some(cell) = self.registry().sandboxes.get(name) {
            let mut state = cell.state();
            state.phase = phase;
            state.settling = Settling::Done;
        }
    }

    pub fn install_file(&self, path: &SandboxPath, bytes: Vec<u8>) {
        self.registry().installed.insert(path.clone(), bytes);
    }

    pub fn provider(&self, name: &ProviderName) -> Option<ProviderRecord> {
        self.registry().providers.get(name).cloned()
    }

    pub fn script_program(&self, program: &SandboxPath, script: ProgramScript) {
        self.registry().scripts.insert(program.clone(), script);
    }

    pub fn add_image(&self, tag: &ImageTag) {
        self.registry().images.insert(tag.clone());
    }

    pub fn set_supervisor(&self, model: SupervisorModel) {
        self.registry().supervisor = model;
    }

    pub fn sandboxes(&self) -> Vec<SandboxHandle> {
        self.registry()
            .sandboxes
            .values()
            .map(|cell| SandboxHandle::new(Arc::clone(cell)))
            .collect()
    }

    pub fn sandbox(&self, name: &SandboxName) -> Option<SandboxHandle> {
        self.registry()
            .sandboxes
            .get(name)
            .map(|cell| SandboxHandle::new(Arc::clone(cell)))
    }

    pub fn providers(&self) -> Vec<ProviderName> {
        self.registry().providers.keys().cloned().collect()
    }

    pub fn profiles(&self) -> Vec<ProfileId> {
        self.registry().profiles.keys().cloned().collect()
    }

    fn registry(&self) -> MutexGuard<'_, Registry> {
        locked(&self.registry)
    }

    fn cell(&self, op: GatewayOp, name: &SandboxName) -> Result<Arc<Cell>, GatewayError> {
        self.cell_at(op, None, name)
    }

    fn cell_at(
        &self,
        op: GatewayOp,
        special: Option<&Trigger>,
        name: &SandboxName,
    ) -> Result<Arc<Cell>, GatewayError> {
        let mut registry = self.registry();
        registry.strike_at(op, special)?;
        registry
            .sandboxes
            .get(name)
            .cloned()
            .ok_or_else(|| GatewayError::NotFound {
                what: Missing::Sandbox(name.clone()),
            })
    }

    fn exec_cell(&self, op: GatewayOp, name: &SandboxName) -> Result<Arc<Cell>, GatewayError> {
        self.exec_cell_at(op, None, name)
    }

    fn exec_cell_at(
        &self,
        op: GatewayOp,
        special: Option<&Trigger>,
        name: &SandboxName,
    ) -> Result<Arc<Cell>, GatewayError> {
        let cell = self.cell_at(op, special, name)?;
        let phase = cell.state().phase;
        if phase.accepts_exec() {
            Ok(cell)
        } else {
            Err(GatewayError::NotReady { phase })
        }
    }

    fn generated_rules(&self, create: &SandboxCreate) -> Vec<NetworkRule> {
        let registry = self.registry();
        create
            .providers()
            .iter()
            .filter_map(|provider| {
                let profile = registry
                    .providers
                    .get(provider)
                    .and_then(|record| registry.profiles.get(&record.profile))?;
                Some(provider_rule(provider, profile))
            })
            .collect()
    }
}

fn observed(cell: &Cell) -> SandboxView {
    let mut state = cell.state();
    state.observe(&cell.root());
    state.view()
}

fn provider_rule(provider: &ProviderName, profile: &ProviderProfile) -> NetworkRule {
    NetworkRule {
        key: RuleKey::provider_rule(provider),
        endpoints: profile
            .endpoints
            .iter()
            .map(|endpoint| RuleEndpoint {
                host: endpoint.host.clone(),
                port: endpoint.port,
                protocol: EndpointProtocol::Tcp,
                tls: TlsMode::Default,
            })
            .collect(),
        binaries: profile.binaries.clone(),
    }
}

fn selector_pairs(rendered: &str) -> Vec<(String, String)> {
    rendered
        .split(',')
        .filter_map(|pair| pair.split_once('='))
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect()
}

#[async_trait]
impl Gateway for FakeGateway {
    fn name(&self) -> &GatewayName {
        &self.name
    }

    async fn health(&self) -> Result<GatewayHealth, GatewayError> {
        self.registry().strike(GatewayOp::Health)?;
        Ok(GatewayHealth {
            version: "0.1.6".to_owned(),
            serving: true,
        })
    }

    async fn ensure_profile(&self, profile: &ProviderProfile) -> Result<ProfileSync, GatewayError> {
        let mut registry = self.registry();
        registry.strike(GatewayOp::Profile)?;
        let sync = match registry.profiles.get(&profile.id) {
            None => ProfileSync::Imported,
            Some(stored) if stored == profile => ProfileSync::Unchanged,
            Some(_) => ProfileSync::Updated,
        };
        if sync != ProfileSync::Unchanged {
            registry
                .profiles
                .insert(profile.id.clone(), profile.clone());
        }
        Ok(sync)
    }

    async fn delete_profile(&self, id: &ProfileId) -> Result<(), GatewayError> {
        let mut registry = self.registry();
        registry.strike(GatewayOp::Profile)?;
        if registry.providers.values().any(|used| used.profile == *id) {
            return Err(GatewayError::FailedPrecondition {
                message: format!("profile '{id}' is used by a provider"),
            });
        }
        registry.profiles.remove(id);
        Ok(())
    }

    async fn provider_state(&self, name: &ProviderName) -> Result<ProviderState, GatewayError> {
        let mut registry = self.registry();
        registry.strike(GatewayOp::Provider)?;
        Ok(if registry.providers.contains_key(name) {
            ProviderState::Present
        } else {
            ProviderState::Absent
        })
    }

    async fn put_provider(&self, spec: &ProviderSpec) -> Result<(), GatewayError> {
        let mut registry = self.registry();
        registry.strike(GatewayOp::Provider)?;
        if !registry.profiles.contains_key(&spec.profile) {
            return Err(GatewayError::NotFound {
                what: Missing::Profile(spec.profile.clone()),
            });
        }
        let ProviderMaterial::ChatgptLogin {
            access_token,
            refresh_token,
            client_id,
            chatgpt_account_id,
        } = &spec.material;
        registry.providers.insert(
            spec.name.clone(),
            ProviderRecord {
                profile: spec.profile.clone(),
                access_key: spec.keys.access_token.to_owned(),
                account_id_key: spec.keys.account_id.to_owned(),
                access_token: access_token.expose().to_owned(),
                refresh_token: refresh_token.expose().to_owned(),
                client_id: client_id.clone(),
                chatgpt_account_id: chatgpt_account_id.clone(),
            },
        );
        Ok(())
    }

    async fn delete_provider(&self, name: &ProviderName) -> Result<(), GatewayError> {
        let mut registry = self.registry();
        registry.strike(GatewayOp::Provider)?;
        registry.providers.remove(name);
        Ok(())
    }

    async fn create_sandbox(&self, spec: &SandboxCreate) -> Result<SandboxView, GatewayError> {
        let mut registry = self.registry();
        registry.strike(GatewayOp::Create)?;
        if registry.sandboxes.contains_key(spec.name()) {
            return Err(GatewayError::AlreadyExists {
                what: Missing::Sandbox(spec.name().clone()),
            });
        }
        if let Some(missing) = spec
            .providers()
            .iter()
            .find(|provider| !registry.providers.contains_key(provider))
        {
            return Err(GatewayError::FailedPrecondition {
                message: format!("provider '{missing}' not found"),
            });
        }
        let plan = MainPlan::parse(spec.main())?;
        let failure = if registry.failing_creates > 0 {
            registry.failing_creates -= 1;
            Some("provisioning failed (injected by the fake)".to_owned())
        } else if registry.images.contains(spec.image()) {
            None
        } else {
            Some(format!("failed to resolve image '{}'", spec.image()))
        };
        let dir = tempfile::Builder::new()
            .prefix("jackin-fake-sandbox-")
            .tempdir()
            .map_err(protocol)?;
        std::fs::create_dir(dir.path().join("sandbox")).map_err(protocol)?;
        let sandbox = Sandbox::new(
            spec.clone(),
            plan,
            registry.provision_polls,
            failure,
            registry.supervisor,
        );
        let view = sandbox.view();
        registry.sandboxes.insert(
            spec.name().clone(),
            Arc::new(Cell {
                name: spec.name().clone(),
                dir,
                closed: CancellationToken::new(),
                inner: Mutex::new(sandbox),
            }),
        );
        Ok(view)
    }

    async fn find_sandboxes(&self, query: &SandboxQuery) -> Result<Vec<SandboxView>, GatewayError> {
        let cells: Vec<Arc<Cell>> = {
            let mut registry = self.registry();
            registry.strike(GatewayOp::Find)?;
            match query {
                SandboxQuery::Name(name) => {
                    registry.sandboxes.get(name).cloned().into_iter().collect()
                }
                SandboxQuery::Labels(selector) => {
                    let wanted = selector_pairs(&selector.render());
                    registry
                        .sandboxes
                        .values()
                        .filter(|cell| {
                            let labels = cell.state().labels().to_labels();
                            wanted.iter().all(|(key, value)| {
                                labels
                                    .iter()
                                    .any(|(k, v)| k.to_string() == *key && v.to_string() == *value)
                            })
                        })
                        .cloned()
                        .collect()
                }
            }
        };
        Ok(cells.iter().map(|cell| observed(cell)).collect())
    }

    async fn sandbox_detail(&self, name: &SandboxName) -> Result<SandboxDetail, GatewayError> {
        let cell = self.cell(GatewayOp::Detail, name)?;
        let view = observed(&cell);
        let (spec, ready) = {
            let state = cell.state();
            (
                state.spec.clone(),
                !matches!(
                    state.provisioning,
                    jackin_openshell::Provisioning::Failed { .. }
                ) && state.phase != Phase::Provisioning,
            )
        };
        let rules = self.generated_rules(&spec);
        let version = if rules.is_empty() { 1 } else { 2 };
        let mut document = spec.policy().clone();
        for rule in rules {
            document = document.with_rule(rule).map_err(protocol)?;
        }
        Ok(SandboxDetail {
            view,
            providers: spec.providers().to_vec(),
            policy: PolicyReport {
                version,
                hash: jackin_core::Sha256::of(document.render().map_err(protocol)?.as_bytes())
                    .to_string(),
                loaded: ready,
                document,
            },
        })
    }

    async fn set_power(
        &self,
        name: &SandboxName,
        action: PowerAction,
    ) -> Result<SandboxView, GatewayError> {
        let cell = self.cell(GatewayOp::Power, name)?;
        let stale = self.registry().stale_observations;
        let mut state = cell.state();
        match (action, state.phase) {
            (PowerAction::Stop, Phase::Ready) => {
                state.phase = Phase::Stopped;
                state.main.exit(None);
                if let Some(supervisor) = &mut state.supervisor {
                    supervisor.stop();
                }
            }
            (PowerAction::Stop, Phase::Completed { .. } | Phase::Error { .. }) => {
                return Err(GatewayError::MainProcessFailed {
                    sandbox: name.clone(),
                });
            }
            (PowerAction::Stop, other) => {
                return Err(GatewayError::FailedPrecondition {
                    message: format!(
                        "sandbox must be Ready, Suspended, Error, or in an automatic restart to stop (current phase: {other:?})"
                    ),
                });
            }
            (PowerAction::Start, Phase::Stopped) => {
                state.phase = Phase::Starting;
                state.settling = Settling::Restarting { stale };
            }
            (PowerAction::Start, Phase::Completed { .. } | Phase::Error { .. }) => {
                state.phase = Phase::Starting;
                state.settling = Settling::Done;
                return Err(already_running());
            }
            (PowerAction::Start, _) => return Err(already_running()),
        }
        Ok(state.view())
    }

    async fn delete_sandbox(&self, name: &SandboxName) -> Result<(), GatewayError> {
        let removed = {
            let mut registry = self.registry();
            registry.strike(GatewayOp::Delete)?;
            registry.sandboxes.remove(name)
        };
        if let Some(cell) = removed {
            cell.closed.cancel();
            cell.state().main.exit(None);
        }
        Ok(())
    }

    async fn run_program(
        &self,
        run: &ProgramRun,
        cancel: &CancellationToken,
    ) -> Result<ProgramOutcome, GatewayError> {
        let trigger = Trigger::Program(run.program.clone());
        let cell = self.exec_cell_at(GatewayOp::Program, Some(&trigger), &run.sandbox)?;
        let (script, stalled) = {
            let registry = self.registry();
            (
                registry.scripts.get(&run.program).cloned(),
                registry.stalls.contains(&run.program),
            )
        };
        exec::run_program(&cell, script, stalled, run, cancel).await
    }

    async fn read_files(
        &self,
        sandbox: &SandboxName,
        paths: &[SandboxPath],
    ) -> Result<Vec<Option<Vec<u8>>>, GatewayError> {
        let cell = self.exec_cell(GatewayOp::ReadFiles, sandbox)?;
        let installed = self.registry().installed.clone();
        Ok(files::read_files(&cell.root(), &installed, paths).await)
    }

    async fn write_file(
        &self,
        sandbox: &SandboxName,
        file: &SandboxFile,
    ) -> Result<(), GatewayError> {
        let trigger = Trigger::Write(file.path.clone());
        let cell = self.exec_cell_at(GatewayOp::WriteFile, Some(&trigger), sandbox)?;
        files::write_file(&cell, file).await
    }

    async fn upload_project(
        &self,
        sandbox: &SandboxName,
        upload: &ProjectUpload,
        cancel: &CancellationToken,
    ) -> Result<(), GatewayError> {
        let cell = self.exec_cell(GatewayOp::Upload, sandbox)?;
        files::upload_project(&cell.root(), upload, cancel).await
    }

    async fn download_file(
        &self,
        sandbox: &SandboxName,
        remote: &SandboxPath,
        local: &HostPath,
        _cancel: &CancellationToken,
    ) -> Result<(), GatewayError> {
        let cell = self.exec_cell(GatewayOp::Download, sandbox)?;
        let corrupt = std::mem::take(&mut self.registry().corrupt_download);
        files::download_file(&cell.root(), remote, local, corrupt).await
    }

    async fn open_terminal(
        &self,
        request: &TerminalRequest,
    ) -> Result<TerminalSession, GatewayError> {
        let cell = self.exec_cell(GatewayOp::Terminal, &request.sandbox)?;
        match &request.kind {
            TerminalKind::MainProcess { input, .. } => Ok(terminal::attach_main(
                &cell,
                request.kind.clone(),
                *input,
                request.size,
            )),
            TerminalKind::Shell { workdir, env } => {
                let host = exec::host_path(&cell.root(), workdir.as_str()).ok_or_else(|| {
                    GatewayError::InvalidArgument {
                        field: "workdir".to_owned(),
                        message: "the fake maps only /sandbox".to_owned(),
                    }
                })?;
                terminal::spawn_shell(&cell, request.kind.clone(), host, env)
            }
        }
    }
}

fn already_running() -> GatewayError {
    GatewayError::FailedPrecondition {
        message: "start sandbox failed: VM sandbox is already running generation".to_owned(),
    }
}
