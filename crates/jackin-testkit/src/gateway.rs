// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod exec;
mod files;
pub(crate) use self::files::apply_host_edit;
mod handle;
mod main_process;
mod state;
mod terminal;

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::sync::{Arc, Mutex, MutexGuard};

use async_trait::async_trait;
use jackin_core::{
    AccountName, Agent, ChatgptAccountId, EnvName, FileMode, GatewayName, HostPath, ImageId,
    ImageTag, Labels, LaunchId, OpenShellWorkspace, Phase, PlannedLabels, PolicyLabel, ProfileId,
    ProjectRoot, ProviderName, RoleName, SandboxName, SandboxPath, Sha256, WorkspaceName,
};
use jackin_openshell::{
    ClosedPolicy, EndpointProtocol, Gateway, GatewayError, GatewayHealth, GatewayOp, MainProcess,
    Missing, NetworkRule, PolicyDocument, PolicyReport, PowerAction, ProfileSync, ProgramOutcome,
    ProgramRun, ProviderMaterial, ProviderProfile, ProviderSpec, ProviderState, RuleEndpoint,
    RuleKey, SandboxCreate, SandboxDetail, SandboxFile, SandboxQuery, SandboxView, TermSize,
    TerminalKind, TerminalRequest, TerminalSession, TlsMode, TransferOp,
};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

pub use handle::SandboxHandle;

use self::main_process::MainPlan;
use self::state::{
    Action, Armed, ArmedTrigger, Cell, Direction, Hold, HoldStatus, Registry, Sandbox, Settling,
};
use crate::engine::FakeEngine;
use crate::host::{FakeClock, Working};
use crate::supervisor::SupervisorModel;
use crate::sync::locked;

#[derive(Debug)]
pub struct FakeGateway {
    name: GatewayName,
    workspace: OpenShellWorkspace,
    registry: Mutex<Registry>,
    holds_changed: Notify,
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
pub struct ShellWitness {
    pub workdir: SandboxPath,
    pub env: BTreeMap<EnvName, String>,
    pub input: Vec<u8>,
    pub sizes: Vec<TermSize>,
    pub closed: bool,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UploadKind {
    Tree,
    File,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostEdit {
    Write { path: HostPath, bytes: Vec<u8> },
    Chmod { path: HostPath, mode: FileMode },
    Symlink { path: HostPath, target: HostPath },
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
    CorruptUpload,
    EditHostAfter {
        upload: UploadKind,
        edit: HostEdit,
    },
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
    HideUntilCreateConflict,
    StartIgnored,
    HoldAt {
        trigger: Trigger,
    },
    PutProviderFailsAfterWrite {
        error: GatewayError,
    },
    DeleteProviderFailsAfterWrite {
        error: GatewayError,
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
            holds_changed: Notify::new(),
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
            Fault::CorruptDownload => {
                registry.corrupt.insert(Direction::Download);
            }
            Fault::CorruptUpload => {
                registry.corrupt.insert(Direction::Upload);
            }
            Fault::EditHostAfter { upload, edit } => registry.host_edits.push((upload, edit)),
            Fault::ProvisionFor { polls } => registry.provision_polls = polls,
            Fault::ProvisionFails { creates } => registry.failing_creates = creates,
            Fault::StaleSupervisor { observations } => registry.stale_observations = observations,
            Fault::FailAt { trigger, error } => registry
                .triggers
                .push(ArmedTrigger::new(trigger, Action::Fail(error))),
            Fault::CancelAt { trigger, token } => registry
                .triggers
                .push(ArmedTrigger::new(trigger, Action::Cancel(token))),
            Fault::HideUntilCreateConflict => registry.hidden_until_conflict = true,
            Fault::StartIgnored => registry.start_ignored = true,
            Fault::HoldAt { trigger } => registry.holds.push(Hold::new(trigger)),
            Fault::PutProviderFailsAfterWrite { error } => {
                registry.put_fails_after_write = Some(error);
            }
            Fault::DeleteProviderFailsAfterWrite { error } => {
                registry.delete_fails_after_write = Some(error);
            }
        }
    }

    pub fn release(&self, trigger: &Trigger) -> bool {
        let released = self.registry().release(trigger);
        self.holds_changed.notify_waiters();
        released
    }

    pub fn calls(&self, op: GatewayOp) -> u32 {
        self.registry().calls.get(&op).copied().unwrap_or(0)
    }

    pub fn held(&self) -> Vec<Trigger> {
        self.registry().held()
    }

    pub async fn until_held(&self, trigger: &Trigger) -> bool {
        loop {
            let changed = self.holds_changed.notified();
            let mut changed = std::pin::pin!(changed);
            changed.as_mut().enable();
            let status = self.registry().hold_status(trigger);
            match status {
                HoldStatus::Reached => return true,
                HoldStatus::Absent => return false,
                HoldStatus::Armed => changed.await,
            }
        }
    }

    pub fn set_attachment_cap(&self, cap: usize) {
        self.registry().attachment_cap = cap;
    }

    async fn pause(
        &self,
        op: GatewayOp,
        special: Option<&Trigger>,
        cancel: Option<&CancellationToken>,
    ) -> Result<(), GatewayError> {
        let Some(gate) = self.registry().arrive(op, special) else {
            return Ok(());
        };
        self.holds_changed.notify_waiters();
        if let Some(token) = cancel {
            tokio::select! {
                () = gate.wait() => Ok(()),
                () = token.cancelled() => Err(GatewayError::Cancelled { op, group: None }),
            }
        } else {
            gate.wait().await;
            Ok(())
        }
    }

    pub fn force_phase(&self, name: &SandboxName, phase: Phase) {
        if let Some(cell) = self.registry().sandboxes.get(name) {
            let mut state = cell.state();
            state.phase = phase;
            state.settling = Settling::Done;
        }
    }

    pub fn insert_unmanaged(
        &self,
        name: &SandboxName,
        labels: Labels,
        phase: Phase,
    ) -> Result<(), GatewayError> {
        let spec = foreign_spec(name)?;
        let plan = MainPlan::parse(spec.main())?;
        let created_at = self.registry().now();
        let mut sandbox = Sandbox::new(
            spec,
            plan,
            0,
            None,
            SupervisorModel::Cooperative,
            created_at,
        );
        sandbox.shown = Some(labels);
        sandbox.phase = phase;
        sandbox.settling = Settling::Done;
        let dir = tempfile::Builder::new()
            .prefix("jackin-fake-sandbox-")
            .tempdir()
            .map_err(protocol)?;
        std::fs::create_dir(dir.path().join("sandbox")).map_err(protocol)?;
        let mut registry = self.registry();
        let Entry::Vacant(slot) = registry.sandboxes.entry(name.clone()) else {
            return Err(GatewayError::AlreadyExists {
                what: Missing::Sandbox(name.clone()),
            });
        };
        slot.insert(Arc::new(Cell {
            name: name.clone(),
            dir,
            closed: CancellationToken::new(),
            inner: Mutex::new(sandbox),
        }));
        Ok(())
    }

    pub fn add_live_rule(&self, name: &SandboxName, rule: NetworkRule) {
        if let Some(cell) = self.registry().sandboxes.get(name) {
            cell.state().live_rules.push(rule);
        }
    }

    pub fn replace_live_policy(
        &self,
        name: &SandboxName,
        policy: PolicyDocument,
    ) -> Result<(), GatewayError> {
        let cell = self
            .registry()
            .sandboxes
            .get(name)
            .cloned()
            .ok_or_else(|| GatewayError::NotFound {
                what: Missing::Sandbox(name.clone()),
            })?;
        cell.state().live_policy = Some(policy);
        Ok(())
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

    pub fn release_stall(&self, program: &SandboxPath) {
        self.registry().stalls.retain(|stalled| stalled != program);
    }

    pub fn add_image(&self, tag: &ImageTag) {
        self.registry().images.insert(tag.clone());
    }

    pub fn use_host_home(&self, home: &HostPath) {
        self.registry().host_home = Some(home.as_path().to_path_buf());
    }

    pub fn link_engine(&self, engine: Arc<FakeEngine>) {
        self.registry().engine = Some(engine);
    }

    pub fn link_clock(&self, clock: Arc<FakeClock>) {
        self.registry().clock = clock;
    }

    fn working(&self) -> Working {
        FakeClock::working(&self.registry().clock)
    }

    pub fn set_serving(&self, serving: bool) {
        self.registry().serving = serving;
    }

    pub fn set_version(&self, version: &str) {
        version.clone_into(&mut self.registry().version);
    }

    pub fn set_supervisor(&self, model: SupervisorModel) {
        self.registry().supervisor = model;
    }

    pub fn sandboxes(&self) -> Vec<SandboxHandle> {
        let registry = self.registry();
        registry
            .sandboxes
            .values()
            .map(|cell| SandboxHandle::new(Arc::clone(cell), Arc::clone(&registry.clock)))
            .collect()
    }

    pub fn sandbox(&self, name: &SandboxName) -> Option<SandboxHandle> {
        let registry = self.registry();
        registry
            .sandboxes
            .get(name)
            .map(|cell| SandboxHandle::new(Arc::clone(cell), Arc::clone(&registry.clock)))
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

    fn struck_at(
        &self,
        op: GatewayOp,
        special: Option<&Trigger>,
        name: &SandboxName,
    ) -> Result<Option<Arc<Cell>>, GatewayError> {
        let mut registry = self.registry();
        registry.strike_at(op, special)?;
        Ok(registry.sandboxes.get(name).cloned())
    }

    fn cell_at(
        &self,
        op: GatewayOp,
        special: Option<&Trigger>,
        name: &SandboxName,
    ) -> Result<Arc<Cell>, GatewayError> {
        self.struck_at(op, special, name)?
            .ok_or_else(|| GatewayError::NotFound {
                what: Missing::Sandbox(name.clone()),
            })
    }

    fn upload_cell(
        &self,
        op: GatewayOp,
        step: TransferOp,
        name: &SandboxName,
    ) -> Result<Arc<Cell>, GatewayError> {
        let cell = self
            .struck_at(op, None, name)?
            .ok_or_else(|| files::upload_absent(step))?;
        if cell.settle().accepts_exec() {
            Ok(cell)
        } else {
            Err(files::upload_not_ready(step))
        }
    }

    fn settled_at(
        &self,
        op: GatewayOp,
        special: Option<&Trigger>,
        name: &SandboxName,
    ) -> Result<(Arc<Cell>, Phase), GatewayError> {
        let cell = self.cell_at(op, special, name)?;
        let phase = cell.settle();
        Ok((cell, phase))
    }

    async fn edit_host(&self, upload: UploadKind) -> Result<(), GatewayError> {
        let edits: Vec<HostEdit> = {
            let mut registry = self.registry();
            let (taken, kept) = std::mem::take(&mut registry.host_edits)
                .into_iter()
                .partition(|(kind, _)| *kind == upload);
            registry.host_edits = kept;
            taken.into_iter().map(|(_, edit)| edit).collect()
        };
        for edit in edits {
            files::edit_host(&edit).await?;
        }
        Ok(())
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
        let (cell, phase) = self.settled_at(op, special, name)?;
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

const FOREIGN_POLICY: &str = "version: 1
filesystem_policy:
  include_workdir: true
  read_only:
    - /usr
  read_write:
    - /sandbox
landlock:
  compatibility: hard_requirement
process:
  run_as_user: sandbox
  run_as_group: sandbox
";

fn foreign_spec(name: &SandboxName) -> Result<SandboxCreate, GatewayError> {
    let planned = PlannedLabels {
        workspace: WorkspaceName::parse("foreign").map_err(protocol)?,
        role: RoleName::parse("foreign").map_err(protocol)?,
        agents: jackin_core::AgentInstances::single(jackin_core::AgentInstance::lead(
            Agent::Codex,
            AccountName::parse("foreign").map_err(protocol)?,
        )),
        policy: PolicyLabel::of("policy"),
        launch: LaunchId::from_entropy(&[0; 26]),
    };
    let image_id = ImageId::parse(&format!("sha256:{}", "0".repeat(64))).map_err(protocol)?;
    Ok(SandboxCreate::new(
        name.clone(),
        ImageTag::parse("jackin.invalid/foreign:0000000000000000").map_err(protocol)?,
        planned.complete(&image_id, jackin_core::DockerLabel::Off),
        Vec::new(),
        ClosedPolicy::parse(FOREIGN_POLICY).map_err(protocol)?,
        MainProcess {
            program: SandboxPath::parse("/bin/sh").map_err(protocol)?,
            args: vec!["-c".to_owned(), "sleep 3600".to_owned()],
        },
    ))
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
        tokio::task::yield_now().await;
        self.registry().strike(GatewayOp::Health)?;
        let registry = self.registry();
        Ok(GatewayHealth {
            version: registry.version.clone(),
            serving: registry.serving,
        })
    }

    async fn ensure_profile(&self, profile: &ProviderProfile) -> Result<ProfileSync, GatewayError> {
        tokio::task::yield_now().await;
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
        tokio::task::yield_now().await;
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
        tokio::task::yield_now().await;
        let mut registry = self.registry();
        registry.strike(GatewayOp::Provider)?;
        Ok(if registry.providers.contains_key(name) {
            ProviderState::Present
        } else {
            ProviderState::Absent
        })
    }

    async fn put_provider(&self, spec: &ProviderSpec) -> Result<(), GatewayError> {
        tokio::task::yield_now().await;
        self.pause(GatewayOp::Provider, None, None).await?;
        let mut registry = self.registry();
        registry.strike(GatewayOp::Provider)?;
        if !registry.profiles.contains_key(&spec.profile) {
            return Err(GatewayError::NotFound {
                what: Missing::Profile(spec.profile.clone()),
            });
        }
        let record = match &spec.material {
            ProviderMaterial::ChatgptLogin {
                access_token,
                refresh_token,
                client_id,
                chatgpt_account_id,
            } => ProviderRecord {
                profile: spec.profile.clone(),
                access_key: spec.keys.access_token.to_owned(),
                account_id_key: spec.keys.account_id.to_owned(),
                access_token: access_token.expose().to_owned(),
                refresh_token: refresh_token.expose().to_owned(),
                client_id: client_id.clone(),
                chatgpt_account_id: chatgpt_account_id.clone(),
            },
            ProviderMaterial::StaticKey { key } => ProviderRecord {
                profile: spec.profile.clone(),
                access_key: spec.keys.access_token.to_owned(),
                account_id_key: String::new(),
                access_token: key.expose().to_owned(),
                refresh_token: String::new(),
                client_id: String::new(),
                chatgpt_account_id: match ChatgptAccountId::parse(
                    "00000000-0000-4000-8000-000000000000",
                ) {
                    Ok(id) => id,
                    Err(_) => {
                        return Err(GatewayError::Protocol {
                            message: "static id".to_owned(),
                        });
                    }
                },
            },
        };
        registry.providers.insert(spec.name.clone(), record);
        match registry.put_fails_after_write.take() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    async fn delete_provider(&self, name: &ProviderName) -> Result<(), GatewayError> {
        tokio::task::yield_now().await;
        self.pause(GatewayOp::Provider, None, None).await?;
        let mut registry = self.registry();
        registry.strike(GatewayOp::Provider)?;
        let attached: Vec<String> = registry
            .sandboxes
            .values()
            .filter(|cell| cell.state().spec.providers().contains(name))
            .map(|cell| cell.name.to_string())
            .collect();
        if !attached.is_empty() {
            return Err(GatewayError::FailedPrecondition {
                message: format!(
                    "provider '{name}' is attached to sandbox(es): {}",
                    attached.join(", ")
                ),
            });
        }
        registry.providers.remove(name);
        match registry.delete_fails_after_write.take() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    async fn create_sandbox(&self, spec: &SandboxCreate) -> Result<SandboxView, GatewayError> {
        tokio::task::yield_now().await;
        self.pause(GatewayOp::Create, None, None).await?;
        let mut registry = self.registry();
        registry.strike(GatewayOp::Create)?;
        if registry.sandboxes.contains_key(spec.name()) {
            registry.hidden_until_conflict = false;
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
        let image_id = registry
            .engine
            .as_ref()
            .and_then(|engine| engine.images().get(spec.image()).cloned());
        let failure = if registry.failing_creates > 0 {
            registry.failing_creates -= 1;
            Some("provisioning failed (injected by the fake)".to_owned())
        } else if registry.images.contains(spec.image()) || image_id.is_some() {
            None
        } else {
            Some(format!("failed to resolve image '{}'", spec.image()))
        };
        let dir = tempfile::Builder::new()
            .prefix("jackin-fake-sandbox-")
            .tempdir()
            .map_err(protocol)?;
        std::fs::create_dir(dir.path().join("sandbox")).map_err(protocol)?;
        let mut sandbox = Sandbox::new(
            spec.clone(),
            plan,
            registry.provision_polls,
            failure,
            registry.supervisor,
            registry.now(),
        );
        sandbox.image_id = image_id;
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
        tokio::task::yield_now().await;
        self.pause(GatewayOp::Find, None, None).await?;
        let cells: Vec<Arc<Cell>> = {
            let mut registry = self.registry();
            registry.strike(GatewayOp::Find)?;
            match query {
                SandboxQuery::Name(name) => {
                    registry.sandboxes.get(name).cloned().into_iter().collect()
                }
                SandboxQuery::Labels(_) if registry.hidden_until_conflict => Vec::new(),
                SandboxQuery::Labels(selector) => {
                    let wanted = selector_pairs(&selector.render());
                    registry
                        .sandboxes
                        .values()
                        .filter(|cell| {
                            let labels = cell.state().all_labels();
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
        tokio::task::yield_now().await;
        self.pause(GatewayOp::Detail, None, None).await?;
        let cell = self.cell(GatewayOp::Detail, name)?;
        let view = observed(&cell);
        let (spec, ready, live, replaced) = {
            let state = cell.state();
            (
                state.spec.clone(),
                !matches!(
                    state.provisioning,
                    jackin_openshell::Provisioning::Failed { .. }
                ) && state.phase != Phase::Provisioning,
                state.live_rules.clone(),
                state.live_policy.clone(),
            )
        };
        let mut rules = self.generated_rules(&spec);
        rules.extend(live);
        let version = if rules.is_empty() { 1 } else { 2 };
        let mut document = spec.policy().clone();
        for rule in rules {
            document = document.with_rule(rule).map_err(protocol)?;
        }
        if let Some(replaced) = replaced {
            document = replaced;
        }
        Ok(SandboxDetail {
            view,
            providers: spec.providers().to_vec(),
            policy: PolicyReport {
                version,
                hash: Sha256::of(document.render().map_err(protocol)?.as_bytes()).to_string(),
                loaded: ready,
                document,
            },
        })
    }

    async fn sandbox_environment(
        &self,
        name: &SandboxName,
    ) -> Result<BTreeMap<EnvName, String>, GatewayError> {
        tokio::task::yield_now().await;
        let cell = self.cell(GatewayOp::Detail, name)?;
        let environment = cell.state().spec.environment();
        Ok(environment)
    }

    async fn set_power(
        &self,
        name: &SandboxName,
        action: PowerAction,
    ) -> Result<SandboxView, GatewayError> {
        tokio::task::yield_now().await;
        self.pause(GatewayOp::Power, None, None).await?;
        let cell = self.cell(GatewayOp::Power, name)?;
        let (stale, start_ignored) = {
            let registry = self.registry();
            (registry.stale_observations, registry.start_ignored)
        };
        let mut state = cell.state();
        state.observe(&cell.root());
        match (action, state.phase) {
            (PowerAction::Stop, Phase::Ready) => {
                state.phase = Phase::Stopped;
                state.main.exit(None);
                if let Some(supervisor) = &mut state.supervisor {
                    supervisor.stop();
                }
            }
            (PowerAction::Stop, Phase::Stopped) => {}
            (PowerAction::Stop, Phase::Error { exit_code: None }) => {
                return Err(protocol(
                    "stop sandbox failed: persist stop marker failed: No such file or directory (os error 2)",
                ));
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
            (PowerAction::Start, Phase::Stopped) if start_ignored => {}
            (PowerAction::Start, Phase::Stopped) => {
                state.phase = Phase::Starting;
                state.settling = Settling::Restarting { stale };
            }
            (PowerAction::Start, Phase::Error { exit_code: None }) => {
                return Err(GatewayError::FailedPrecondition {
                    message: "sandbox must be Stopped, Suspended, Completed, or a failed main-process Error to start (current phase: Error); stop the sandbox before retrying start".to_owned(),
                });
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
        tokio::task::yield_now().await;
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
        let _working = self.working();
        tokio::task::yield_now().await;
        let trigger = Trigger::Program(run.program.clone());
        self.pause(GatewayOp::Program, Some(&trigger), Some(cancel))
            .await?;
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
        let _working = self.working();
        tokio::task::yield_now().await;
        self.pause(GatewayOp::ReadFiles, None, None).await?;
        let cell = self.exec_cell(GatewayOp::ReadFiles, sandbox)?;
        let installed = self.registry().installed.clone();
        files::read_files(&cell.root(), &installed, paths).await
    }

    async fn write_file(
        &self,
        sandbox: &SandboxName,
        file: &SandboxFile,
    ) -> Result<(), GatewayError> {
        let _working = self.working();
        tokio::task::yield_now().await;
        let trigger = Trigger::Write(file.path.clone());
        self.pause(GatewayOp::WriteFile, Some(&trigger), None)
            .await?;
        let cell = self.exec_cell_at(GatewayOp::WriteFile, Some(&trigger), sandbox)?;
        files::write_file(&cell, file).await
    }

    async fn upload_tree(
        &self,
        sandbox: &SandboxName,
        source: &ProjectRoot,
        dest: &SandboxPath,
        cancel: &CancellationToken,
    ) -> Result<(), GatewayError> {
        let _working = self.working();
        tokio::task::yield_now().await;
        self.pause(GatewayOp::UploadTree, None, Some(cancel))
            .await?;
        let cell = self.upload_cell(GatewayOp::UploadTree, TransferOp::UploadFiltered, sandbox)?;
        let home = self.registry().host_home.clone();
        files::upload_tree(&cell.root(), home.as_deref(), source, dest, cancel).await?;
        self.edit_host(UploadKind::Tree).await
    }

    async fn upload_file(
        &self,
        sandbox: &SandboxName,
        source: &HostPath,
        dest: &SandboxPath,
        cancel: &CancellationToken,
    ) -> Result<(), GatewayError> {
        let _working = self.working();
        tokio::task::yield_now().await;
        self.pause(GatewayOp::UploadFile, None, Some(cancel))
            .await?;
        let cell = self.upload_cell(GatewayOp::UploadFile, TransferOp::UploadFile, sandbox)?;
        let corrupt = self.registry().corrupt.remove(&Direction::Upload);
        files::upload_file(&cell.root(), source, dest, corrupt, cancel).await?;
        self.edit_host(UploadKind::File).await
    }

    async fn download_file(
        &self,
        sandbox: &SandboxName,
        remote: &SandboxPath,
        local: &HostPath,
        cancel: &CancellationToken,
    ) -> Result<Sha256, GatewayError> {
        let _working = self.working();
        tokio::task::yield_now().await;
        self.pause(GatewayOp::Download, None, Some(cancel)).await?;
        let found = self.struck_at(GatewayOp::Download, None, sandbox)?;
        if cancel.is_cancelled() {
            return Err(GatewayError::Cancelled {
                op: GatewayOp::Download,
                group: None,
            });
        }
        let cell = found.ok_or_else(|| GatewayError::NotFound {
            what: Missing::Sandbox(sandbox.clone()),
        })?;
        let phase = cell.settle();
        if !phase.accepts_exec() {
            return Err(GatewayError::NotReady { phase });
        }
        let corrupt = self.registry().corrupt.remove(&Direction::Download);
        files::download_file(&cell.root(), remote, local, corrupt).await
    }

    async fn open_terminal(
        &self,
        request: &TerminalRequest,
    ) -> Result<TerminalSession, GatewayError> {
        tokio::task::yield_now().await;
        self.pause(GatewayOp::Terminal, None, None).await?;
        let (cell, phase) = self.settled_at(GatewayOp::Terminal, None, &request.sandbox)?;
        match &request.kind {
            TerminalKind::MainProcess { .. } => {
                if phase.accepts_exec() {
                    let cap = self.registry().attachment_cap;
                    terminal::attach_main(&cell, request.kind.clone(), request.size, cap)
                } else if matches!(
                    phase,
                    Phase::Completed { .. } | Phase::Error { exit_code: Some(_) }
                ) {
                    Ok(terminal::refuse_main(request.kind.clone()))
                } else {
                    Err(GatewayError::NotReady { phase })
                }
            }
            TerminalKind::Shell { workdir, .. } | TerminalKind::Exec { workdir, .. } => {
                if !phase.accepts_exec() {
                    return Err(GatewayError::NotReady { phase });
                }
                let host = exec::workdir_path(&cell, workdir.as_str()).map_err(protocol)?;
                terminal::spawn_shell(&cell, request.kind.clone(), host, request.size)
            }
        }
    }
}

fn already_running() -> GatewayError {
    GatewayError::FailedPrecondition {
        message: "start sandbox failed: VM sandbox is already running generation".to_owned(),
    }
}
