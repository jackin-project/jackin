// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::Duration;

use jackin_console::backend::Ticket;
use jackin_console::domain::account::{AccountId, AccountRegistry};
use jackin_console::domain::agent::{Agent, Provider};
use jackin_console::domain::instance::InstanceId;
use jackin_console::domain::role::{FindingArea, RoleFinding, RoleManifestDoc};
use jackin_console::domain::vocabulary::{Rejection, SaveField};
use jackin_console::domain::workspace::{MountSource, Workspace, WorkspaceId};
use jackin_console::panes::{FailNext, LinkFault};
use jackin_console::world::{GlobalConfig, Msg, SaveFailure, SaveSuccess, SaveWarning, Snapshot};
use jackin_core::{
    AccountName, Agent as CoreAgent, Config, ConfigEdit, ConfigError, EnvName, FileKind, HostPath,
    ModelName, PathError, RepoPath, RoleManifest, RoleName, RoleSource, RoleUrl, WorkspaceName,
    read_credential_file,
};
use jackin_core::{
    EnvSpec, FileLock, ManifestError, Poll, PollError, PollPlan, Sha256, SupervisorReading,
    SupervisorState, Workspace as CoreWorkspace, hold_lock, poll_until,
};
use jackin_core::{lock_path, plain};
use jackin_image::Rebuild;
use jackin_openshell::{GatewayError, PowerAction};
use jackin_session::{
    AccountPlan, BuildRequest, Gateways, LineDelivery, MANIFEST_FILE, PurgeMode, RestartError,
    RoleError, Session, Source, WorkspaceHold, build_observed, error_chain, launch_events,
    plan_account_edits_for_agents, read_chatgpt_login,
};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::bridge::Settled;
use crate::changes::read_changes;
use crate::observe::{Inputs, Observation, locked};
use crate::persist::{self, Folder, PersistError, Resolved, sentence};
use crate::project::manifest_doc;
use crate::sessions::{Access, Home};

#[derive(Debug, Error)]
enum Refused {
    #[error("{}", sentence(.0))]
    Rejected(Rejection),
    #[error("{}", plain(.0))]
    Text(String),
    #[error("cancelled while starting Codex")]
    StartCancelled,
    #[error("Codex in {0} did not reach its start state")]
    StartTimedOut(jackin_core::SandboxName),
    #[error("Codex is already running in {0}")]
    AgentRunning(jackin_core::SandboxName),
    #[error("the configuration path has no directory")]
    NoConfigDir,
    #[error("cancelled while waiting for the role manifest")]
    ManifestCancelled,
    #[error("another jackin process is saving this role manifest")]
    ManifestBusy,
    #[error("{}", .0.iter().map(|error| text(error)).collect::<Vec<_>>().join("; "))]
    RoleErrors(Vec<RoleError>),
    #[error(transparent)]
    Persist(#[from] PersistError),
    #[error(transparent)]
    Config(#[from] ConfigError),
}

const SUPERVISOR_WAIT: PollPlan = PollPlan {
    interval: Duration::from_millis(250),
    timeout: Duration::from_secs(30),
};
const LOCK_KEY_LEN: usize = 32;
const MANIFEST_LOCK: PollPlan = PollPlan {
    interval: Duration::from_millis(50),
    timeout: Duration::from_secs(30),
};

fn text(error: &dyn std::error::Error) -> String {
    error_chain(error)
}

fn save_failure(error: &dyn std::error::Error) -> SaveFailure {
    SaveFailure {
        error: text(error),
        next: None,
    }
}

fn core_agent(agent: Agent) -> Result<CoreAgent, PersistError> {
    crate::project::core_agent(agent).ok_or_else(|| Rejection::CannotSave(SaveField::Agent).into())
}

fn finding(error: &RoleError, doc: &RoleManifestDoc) -> RoleFinding {
    let area_of = |path: &str| {
        if path == doc.dockerfile {
            FindingArea::Dockerfile
        } else if path == doc.policy {
            FindingArea::Policy
        } else if [&doc.hook_setup, &doc.hook_preflight]
            .into_iter()
            .flatten()
            .any(|hook| hook == path)
        {
            FindingArea::Hook
        } else {
            FindingArea::Manifest
        }
    };
    let area = match error {
        RoleError::Dockerfile(_) => FindingArea::Dockerfile,
        RoleError::Policy(_) | RoleError::ProviderHostClosure(_) | RoleError::EngineHost(_) => {
            FindingArea::Policy
        }
        RoleError::FileMissing { path }
        | RoleError::FileNotRegular { path }
        | RoleError::EscapesRepository { path }
        | RoleError::FileTooLarge { path, .. } => area_of(path.as_str()),
        _ => FindingArea::Manifest,
    };
    RoleFinding {
        area,
        key: None,
        message: text(error),
        fix: None,
    }
}

pub(crate) enum LocalKind {
    File(HostPath),
    Folder(HostPath),
}

impl LocalKind {
    pub(crate) fn source(&self, login: bool) -> Source<'_> {
        match self {
            LocalKind::File(path) => Source::File { path },
            LocalKind::Folder(path) => Source::Folder { path, login },
        }
    }
}

impl Inputs {
    async fn saved_snapshot(&self) -> Option<Snapshot> {
        let config = self.config.load().await.ok()?;
        Some(self.current(&config).await.snapshot)
    }

    async fn failed(&self, reply: Msg) -> Settled {
        Settled::reverted(reply, self.saved_snapshot().await)
    }

    pub(crate) async fn save_result(
        &self,
        result: Result<Config, ConfigError>,
    ) -> Result<(Config, Option<SaveWarning>), ConfigError> {
        match result {
            Ok(config) => Ok((config, None)),
            Err(error) if error.committed() => {
                Ok((self.config.load().await?, Some(SaveWarning::DirectorySync)))
            }
            Err(error) => Err(error),
        }
    }

    async fn applied(
        &self,
        config: &Config,
        edits: Vec<ConfigEdit>,
    ) -> Result<(Config, Option<SaveWarning>), Refused> {
        if edits.is_empty() {
            return Ok((config.clone(), None));
        }
        Ok(self.save_result(self.config.edit(edits).await).await?)
    }

    async fn snapshot_of(&self, config: &Config) -> Box<Snapshot> {
        let Observation { snapshot, .. } = self.current(config).await;
        Box::new(snapshot)
    }

    async fn save_success(&self, config: &Config, warning: Option<SaveWarning>) -> SaveSuccess {
        SaveSuccess {
            snapshot: self.snapshot_of(config).await,
            warning,
        }
    }

    pub(crate) fn expanded(&self, text: &str) -> Result<HostPath, PathError> {
        let expanded = match text.strip_prefix('~') {
            Some(rest) if rest.is_empty() || rest.starts_with('/') => {
                Path::new(&self.home).join(rest.trim_start_matches('/'))
            }
            _ => Path::new(text).to_path_buf(),
        };
        HostPath::parse(&expanded)
    }

    pub(crate) async fn local_kind(&self, text: &str) -> Result<LocalKind, PathError> {
        let path = self.expanded(text)?;
        Ok(match self.host.fs.stat_following(&path).await {
            Ok(Some(FileKind::File)) => LocalKind::File(path),
            _ => LocalKind::Folder(path),
        })
    }

    async fn folder(&self, text: &str) -> Result<HostPath, PersistError> {
        let path = self.expanded(text)?;
        match HostPath::canonical_dir(self.host.fs.as_ref(), &path).await {
            Err(PathError::NotADirectory(_)) => Err(PersistError::from(Rejection::NotAFolder {
                folder: text.to_owned(),
            })),
            other => Ok(other?),
        }
    }

    async fn login_file(
        &self,
        provider: jackin_core::Provider,
        folder: &HostPath,
        text: &str,
    ) -> Result<HostPath, PersistError> {
        let unusable = || {
            PersistError::from(Rejection::NotAFolder {
                folder: text.to_owned(),
            })
        };
        let name = provider.login_file().ok_or_else(unusable)?;
        let file = HostPath::resolve(Path::new(name), folder)?;
        match read_credential_file(self.host.fs.as_ref(), &file).await? {
            Ok(_) => Ok(file),
            Err(_) => Err(unusable()),
        }
    }

    pub(crate) async fn save_workspace(
        &self,
        base: Option<&Workspace>,
        workspace: &Workspace,
    ) -> Settled {
        match self.try_save_workspace(base, workspace).await {
            Ok((id, snapshot)) => Msg::WorkspaceSaved {
                id: Some(id),
                result: Ok(snapshot),
            }
            .into(),
            Err(error) => {
                let reply = Msg::WorkspaceSaved {
                    id: None,
                    result: Err(save_failure(&error)),
                };
                self.failed(reply).await
            }
        }
    }

    async fn try_save_workspace(
        &self,
        base: Option<&Workspace>,
        workspace: &Workspace,
    ) -> Result<(WorkspaceId, SaveSuccess), Refused> {
        let config = self.config.load().await?;
        let _hold = match base.filter(|b| b.name != workspace.name) {
            Some(old) => {
                let old = WorkspaceName::parse(&old.name).map_err(PersistError::from)?;
                self.ensure_unused(&config, &old).await?
            }
            None => None,
        };
        let project = match base {
            None => {
                let source = workspace.mounts.first().map(|mount| &mount.source);
                match source {
                    Some(MountSource::Host(path)) => Some(self.folder(path).await?),
                    Some(MountSource::Git(_)) | None => {
                        return Err(PersistError::from(Rejection::NoProject).into());
                    }
                }
            }
            Some(_) => None,
        };
        let resolved = Resolved {
            project,
            folders: BTreeMap::new(),
        };
        let edits = persist::workspace(&config, base, workspace, &resolved)?;
        let (saved, warning) = self.applied(&config, edits).await?;
        let name = WorkspaceName::parse(&workspace.name).map_err(PersistError::from)?;
        if let Some(old) = base.filter(|b| b.name != workspace.name) {
            let old = WorkspaceName::parse(&old.name).map_err(PersistError::from)?;
            locked(&self.ids).rename(&old, &name);
        }
        let saved = self.save_success(&saved, warning).await;
        let id = locked(&self.ids)
            .lookup(&name)
            .ok_or_else(|| Refused::Text(name.to_string()))?;
        Ok((id, saved))
    }

    pub(crate) async fn delete_workspace(&self, id: WorkspaceId) -> Settled {
        let result = self.try_delete_workspace(id).await;
        match result {
            Ok(snapshot) => Msg::WorkspaceDeleted {
                id,
                result: Ok(snapshot),
            }
            .into(),
            Err(error) => {
                let reply = Msg::WorkspaceDeleted {
                    id,
                    result: Err(text(&error)),
                };
                self.failed(reply).await
            }
        }
    }

    async fn try_delete_workspace(&self, id: WorkspaceId) -> Result<SaveSuccess, Refused> {
        let name = locked(&self.ids)
            .name(id)
            .cloned()
            .ok_or_else(|| Refused::Text(id.to_string()))?;
        let config = self.config.load().await?;
        let _hold = self.ensure_unused(&config, &name).await?;
        let (saved, warning) = self
            .save_result(
                self.config
                    .edit(vec![ConfigEdit::RemoveWorkspace(name)])
                    .await,
            )
            .await?;
        Ok(self.save_success(&saved, warning).await)
    }

    pub(crate) async fn save_global(&self, base: &GlobalConfig, global: &GlobalConfig) -> Settled {
        let result = async {
            let config = self.config.load().await?;
            let edits = persist::global(base, global)?;
            let (saved, warning) = self.applied(&config, edits).await?;
            Ok::<_, Refused>(self.save_success(&saved, warning).await)
        }
        .await;
        match result {
            Ok(snapshot) => Msg::GlobalSaved {
                result: Ok(snapshot),
            }
            .into(),
            Err(error) => {
                let reply = Msg::GlobalSaved {
                    result: Err(save_failure(&error)),
                };
                self.failed(reply).await
            }
        }
    }

    async fn resolve_accounts(
        &self,
        base: &AccountRegistry,
        registry: &AccountRegistry,
    ) -> Result<Resolved, Refused> {
        let mut resolved = Resolved::default();
        for (provider, path) in persist::folders(registry, base) {
            let folder = self.local_source(provider, &path).await?;
            resolved.folders.insert((provider, path), folder);
        }
        Ok(resolved)
    }

    async fn local_source(
        &self,
        console_provider: Provider,
        path: &str,
    ) -> Result<Folder, Refused> {
        let kind = self.local_kind(path).await.map_err(PersistError::from)?;
        if let LocalKind::File(file) = &kind {
            self.sources
                .check(kind.source(false), &self.cancel)
                .await
                .map_err(|error| Refused::Text(text(&error)))?;
            return Ok(Folder::Login { file: file.clone() });
        }
        let folder = self.folder(path).await?;
        let provider = crate::project::core_provider(console_provider);
        if provider == jackin_core::Provider::Codex {
            let found = read_chatgpt_login(self.host.fs.as_ref(), &folder)
                .await
                .map_err(|error| Refused::Text(text(&error)))?;
            return Ok(Folder::Chatgpt {
                home: folder,
                account: found.account_id,
            });
        }
        Ok(Folder::Login {
            file: self.login_file(provider, &folder, path).await?,
        })
    }

    async fn apply_accounts(
        &self,
        edits: Vec<ConfigEdit>,
        agents: BTreeMap<AccountName, CoreAgent>,
    ) -> Result<(Config, Option<SaveWarning>), Refused> {
        if edits.is_empty() {
            return Ok((self.config.load().await?, None));
        }
        let plan = plan_account_edits_for_agents(&self.desk, edits, agents, &self.cancel)
            .await
            .map_err(|error| Refused::Text(text(&error)))?;
        match plan {
            AccountPlan::Offline(plan) => Ok(self.save_result(plan.apply().await).await?),
            AccountPlan::Gateway(plan) => {
                let gateways = self.gateways(plan.config()).await?;
                match plan.apply(&gateways).await {
                    Ok(config) => Ok((config, None)),
                    Err(jackin_session::AccountEditError::Config(error)) if error.committed() => {
                        Ok(self.save_result(Err(error)).await?)
                    }
                    Err(error) => Err(Refused::Text(text(&error))),
                }
            }
        }
    }

    async fn gateways(&self, config: &Config) -> Result<Box<Gateways>, Refused> {
        let home = self
            .sessions
            .home(config, self.connect.as_ref(), &self.cancel)
            .await;
        match home {
            Home::Ready(gateways) => Ok(gateways),
            Home::Down(error) => Err(Refused::Text(text(&*error))),
            Home::NotChosen => Err(ConfigError::NoGateway.into()),
        }
    }

    async fn ensure_unused(
        &self,
        config: &Config,
        name: &WorkspaceName,
    ) -> Result<Option<WorkspaceHold>, Refused> {
        if config.gateways().next().is_none() {
            return Ok(None);
        }
        self.gateways(config)
            .await?
            .ensure_workspace_unused(config, name, &self.cancel)
            .await
            .map(Some)
            .map_err(|error| Refused::Text(text(&error)))
    }

    pub(crate) async fn save_accounts(
        &self,
        ticket: Ticket,
        base: &AccountRegistry,
        registry: &AccountRegistry,
    ) -> Settled {
        let result = async {
            let resolved = self.resolve_accounts(base, registry).await?;
            let edits = persist::accounts(base, registry, &resolved)?;
            let added = persist::added(registry)?;
            let (saved, warning) = self
                .apply_accounts(edits, persist::agents(base, registry))
                .await?;
            Ok::<_, Refused>((self.save_success(&saved, warning).await, added))
        }
        .await;
        match result {
            Ok((snapshot, added)) => Msg::AccountsSaved {
                ticket,
                added,
                result: Ok(snapshot),
            }
            .into(),
            Err(error) => {
                let reply = Msg::AccountsSaved {
                    ticket,
                    added: None,
                    result: Err(text(&error)),
                };
                self.failed(reply).await
            }
        }
    }

    pub(crate) async fn read_changes(
        &self,
        ticket: Ticket,
        instance: &InstanceId,
        unpushed: usize,
    ) -> Settled {
        let result = async {
            let (gateway, sandbox) = self.sandbox(instance)?;
            let session = self.session(&gateway)?;
            read_changes(&session, &sandbox, unpushed, &self.cancel)
                .await
                .map_err(Refused::Text)
        }
        .await
        .map_err(|error| text(&error));
        Msg::ChangesRead { ticket, result }.into()
    }

    fn sandbox(
        &self,
        instance: &InstanceId,
    ) -> Result<(jackin_core::GatewayName, jackin_core::SandboxName), Refused> {
        self.locate(instance)
            .map(|observed| (observed.gateway, observed.summary.name))
            .ok_or_else(|| {
                Refused::Rejected(Rejection::NotInRoster {
                    instance: instance.short_label(),
                })
            })
    }

    pub(crate) fn pane_target(
        &self,
        instance: &InstanceId,
    ) -> Result<(Session, jackin_core::SandboxName), LinkFault> {
        let (gateway, name) = self
            .sandbox(instance)
            .map_err(|error| LinkFault::new(plain(&error)))?;
        match self.sessions.access(&gateway) {
            Access::Up(session) => Ok((*session, name)),
            Access::Down { error, .. } => {
                Err(LinkFault::new(text(&*error)).then(FailNext::RunDoctor))
            }
            Access::NotChosen => Err(LinkFault::new(text(&ConfigError::UnknownGateway(gateway)))),
        }
    }

    fn session(&self, gateway: &jackin_core::GatewayName) -> Result<Session, Refused> {
        match self.sessions.access(gateway) {
            Access::Up(session) => Ok(*session),
            Access::Down { error, .. } => Err(Refused::Text(text(&*error))),
            Access::NotChosen => Err(ConfigError::UnknownGateway(gateway.clone()).into()),
        }
    }

    async fn after_lifecycle(&self) -> Result<Box<Snapshot>, Refused> {
        let observation = self.observe().await?;
        Ok(Box::new(observation.snapshot))
    }

    async fn powered(
        &self,
        instance: &InstanceId,
        action: PowerAction,
    ) -> Result<Box<Snapshot>, String> {
        let result = async {
            let (gateway, name) = self.sandbox(instance)?;
            self.session(&gateway)?
                .power(&name, action, &self.cancel)
                .await
                .map_err(|error| Refused::Text(text(&error)))?;
            self.after_lifecycle().await
        }
        .await;
        result.map_err(|error| text(&error))
    }

    async fn power_unreadable(
        &self,
        instance: &InstanceId,
        action: PowerAction,
    ) -> Result<Box<Snapshot>, String> {
        let result = async {
            let observed = self.locate_unreadable(instance).ok_or_else(|| {
                Refused::Rejected(Rejection::NotInRoster {
                    instance: instance.short_label(),
                })
            })?;
            let gateway = observed.gateway;
            let sandbox = observed.sandbox;
            let allowed = match action {
                PowerAction::Start => jackin_core::Phase::Stopped,
                PowerAction::Stop => jackin_core::Phase::Ready,
            };
            if sandbox.phase != allowed {
                let phase = crate::project::sandbox_phase(sandbox.phase);
                let message = match action {
                    PowerAction::Start => format!(
                        "Cannot start sandbox {} while {} · only stopped sandboxes can start",
                        sandbox.name,
                        phase.label()
                    ),
                    PowerAction::Stop => format!(
                        "Cannot stop sandbox {} while {} · only ready sandboxes can stop",
                        sandbox.name,
                        phase.label()
                    ),
                };
                return Err(Refused::Text(message));
            }
            self.session(&gateway)?
                .power_unreadable(&sandbox.name, action, &self.cancel)
                .await
                .map_err(|error| Refused::Text(text(&error)))?;
            self.after_lifecycle().await
        }
        .await;
        result.map_err(|error| text(&error))
    }

    pub(crate) async fn power_start(&self, instance: &InstanceId) -> Msg {
        Msg::PowerStarted {
            instance: instance.clone(),
            result: self.power_unreadable(instance, PowerAction::Start).await,
        }
    }

    pub(crate) async fn power_stop(&self, instance: &InstanceId) -> Msg {
        Msg::PowerStopped {
            instance: instance.clone(),
            result: self.power_unreadable(instance, PowerAction::Stop).await,
        }
    }

    pub(crate) async fn stop(&self, instance: &InstanceId) -> Msg {
        Msg::Stopped {
            instance: instance.clone(),
            result: self.powered(instance, PowerAction::Stop).await,
        }
    }

    pub(crate) async fn restore(&self, instance: &InstanceId) -> Msg {
        let result = async {
            let (gateway, name) = self.sandbox(instance)?;
            let session = self.session(&gateway)?;
            session
                .power(&name, PowerAction::Start, &self.cancel)
                .await
                .map_err(|error| Refused::Text(text(&error)))?;
            self.await_supervisor(&session, &name).await?;
            self.start_agent(&session, &name).await?;
            self.after_lifecycle().await
        }
        .await;
        Msg::Restored {
            instance: instance.clone(),
            result: result.map_err(|error| text(&error)),
        }
    }

    async fn await_supervisor(
        &self,
        session: &Session,
        name: &jackin_core::SandboxName,
    ) -> Result<(), Refused> {
        poll_until(
            self.host.clock.as_ref(),
            SUPERVISOR_WAIT,
            &self.cancel,
            || async {
                let reading = session.supervisor_state(name).await?;
                Ok::<_, GatewayError>(match reading {
                    SupervisorReading::Reported(SupervisorState::Waiting) => Poll::Ready(()),
                    _ => Poll::Pending,
                })
            },
        )
        .await
        .map_err(|error| match error {
            PollError::Cancelled => Refused::StartCancelled,
            PollError::Timeout { .. } => Refused::StartTimedOut(name.clone()),
            PollError::Probe(error) => Refused::Text(text(&error)),
        })
    }

    async fn start_agent(
        &self,
        session: &Session,
        name: &jackin_core::SandboxName,
    ) -> Result<(), Refused> {
        let (events, feed) = launch_events(LineDelivery::BestEffort);
        drop(feed);
        session
            .restart_agent(name, events, self.cancel.clone())
            .await
            .map(drop)
            .map_err(|error| match error {
                RestartError::AlreadyRunning { .. } => Refused::AgentRunning(name.clone()),
                other => Refused::Text(text(&other)),
            })
    }

    pub(crate) async fn prewarm(&self, id: WorkspaceId) -> Msg {
        let result = self.try_prewarm(id).await.map_err(|error| text(&error));
        Msg::Prewarmed {
            workspace: id,
            result,
        }
    }

    async fn try_prewarm(&self, id: WorkspaceId) -> Result<(), Refused> {
        let name = locked(&self.ids)
            .name(id)
            .cloned()
            .ok_or_else(|| Refused::Text(id.to_string()))?;
        let config = self.config.load().await?;
        let workspace = config.require_workspace(&name)?;
        let (gateway, session) = match self.sessions.default_target() {
            Some((gateway, Access::Up(session))) => (gateway, *session),
            Some((_, Access::Down { error, .. })) => return Err(Refused::Text(text(&*error))),
            Some((_, Access::NotChosen)) | None => return Err(ConfigError::NoGateway.into()),
        };
        let request = BuildRequest {
            gateway: Some(&gateway),
            role: None,
            path: &workspace.path,
            rebuild: Rebuild::No,
        };
        build_observed(
            session.engine(),
            &self.roles,
            &config,
            &request,
            &self.cancel,
            |_| {},
        )
        .await
        .map(drop)
        .map_err(|error| Refused::Text(text(&error)))
    }

    pub(crate) async fn purge(&self, instance: &InstanceId) -> Msg {
        let result = async {
            let (gateway, name) = self.sandbox(instance)?;
            let config = self.config.load().await?;
            self.session(&gateway)?
                .purge(&config, &name, PurgeMode::Guarded)
                .await
                .map_err(|error| Refused::Text(text(&error)))?;
            self.after_lifecycle().await
        }
        .await;
        Msg::Purged {
            instance: instance.clone(),
            result: result.map_err(|error| text(&error)),
        }
    }

    pub(crate) async fn bind_role(
        &self,
        role: &str,
        agent: Agent,
        account: Option<&AccountId>,
    ) -> Settled {
        let result = async {
            let name = RoleName::parse(role).map_err(PersistError::from)?;
            let agent = core_agent(agent)?;
            let edit = match account {
                Some(id) => ConfigEdit::BindRoleAccount {
                    role: name,
                    agent,
                    account: AccountName::parse(id).map_err(PersistError::from)?,
                },
                None => ConfigEdit::UnbindRoleAccount { role: name, agent },
            };
            let (saved, warning) = self.save_result(self.config.edit(vec![edit]).await).await?;
            Ok::<_, Refused>(self.save_success(&saved, warning).await)
        }
        .await;
        let reply = |result| Msg::RoleBound {
            role: role.to_owned(),
            agent,
            result,
        };
        match result {
            Ok(snapshot) => reply(Ok(snapshot)).into(),
            Err(error) => self.failed(reply(Err(text(&error)))).await,
        }
    }

    pub(crate) async fn remove_role(&self, role: &str) -> Settled {
        let result = async {
            let name = RoleName::parse(role).map_err(PersistError::from)?;
            let (saved, warning) = self
                .save_result(self.config.edit(vec![ConfigEdit::RemoveRole(name)]).await)
                .await?;
            Ok::<_, Refused>(self.save_success(&saved, warning).await)
        }
        .await;
        let reply = |result| Msg::RoleRemoved {
            role: role.to_owned(),
            result,
        };
        match result {
            Ok(snapshot) => reply(Ok(snapshot)).into(),
            Err(error) => self.failed(reply(Err(text(&error)))).await,
        }
    }

    pub(crate) async fn add_role(&self, location: &str) -> Settled {
        let result = self.try_add_role(location.trim()).await;
        let reply = |result| Msg::RoleAdded {
            location: location.to_owned(),
            result,
        };
        match result {
            Ok(added) => reply(Ok(added)).into(),
            Err(error) => self.failed(reply(Err(text(&error)))).await,
        }
    }

    async fn try_add_role(&self, location: &str) -> Result<(String, SaveSuccess), Refused> {
        let (name, source) = if let Ok(url) = RoleUrl::parse(location) {
            let loaded = self
                .roles
                .fetch_git(&url, None, &self.cancel)
                .await
                .map_err(|error| Refused::Text(text(&error)))?;
            (loaded.name().clone(), RoleSource::Git { url, rev: None })
        } else {
            let root = self.folder(location).await?;
            let loaded = self
                .roles
                .load_path(&root, &CancellationToken::new())
                .await
                .map_err(|error| Refused::Text(text(&error)))?;
            (loaded.name().clone(), RoleSource::Path(root))
        };
        let (saved, warning) = self
            .save_result(
                self.config
                    .edit(vec![ConfigEdit::AddRole {
                        name: name.clone(),
                        source,
                    }])
                    .await,
            )
            .await?;
        Ok((name.to_string(), self.save_success(&saved, warning).await))
    }

    async fn role_root(&self, config: &Config, name: &RoleName) -> Result<HostPath, Refused> {
        let entry = config.role(name).ok_or_else(|| {
            Refused::Rejected(Rejection::UnknownRole {
                role: name.to_string(),
            })
        })?;
        match &entry.source {
            RoleSource::Path(path) => Ok(path.clone()),
            RoleSource::Git { .. } => {
                let anywhere = CoreWorkspace::new(
                    HostPath::parse(Path::new(&self.home)).map_err(PersistError::from)?,
                    false,
                );
                let loaded = self
                    .roles
                    .load(config, Some(name), &anywhere, &CancellationToken::new())
                    .await
                    .map_err(|error| Refused::Text(text(&error)))?;
                Ok(loaded.root().clone())
            }
        }
    }

    pub(crate) async fn load_role(&self, role: &str) -> Msg {
        let result = async {
            let config = self.config.load().await?;
            let name = RoleName::parse(role).map_err(PersistError::from)?;
            let root = self.role_root(&config, &name).await?;
            let loaded = self
                .roles
                .load_path(&root, &CancellationToken::new())
                .await
                .map_err(|error| Refused::Text(text(&error)))?;
            if let Some(entry) = config.role(&name) {
                locked(&self.docs)
                    .insert(name, (entry.source.clone(), Ok(loaded.manifest().clone())));
            }
            Ok::<_, Refused>(self.snapshot_of(&config).await)
        }
        .await;
        Msg::RoleLoaded {
            role: role.to_owned(),
            result: result.map_err(|error| text(&error)),
        }
    }

    pub(crate) async fn check_role(
        &self,
        ticket: Ticket,
        role: &str,
        doc: &RoleManifestDoc,
    ) -> Msg {
        let result = async {
            let config = self.config.load().await?;
            let name = RoleName::parse(role).map_err(PersistError::from)?;
            let root = self.role_root(&config, &name).await?;
            let found: Vec<RoleFinding> = match self.roles.read_manifest(&root).await {
                Err(errors) => errors.iter().map(|error| finding(error, doc)).collect(),
                Ok(mut manifest) => {
                    let base = manifest_doc(&manifest);
                    match merge_manifest(&mut manifest, &base, doc) {
                        Ok(()) => self
                            .roles
                            .findings_of(&root, manifest)
                            .await
                            .iter()
                            .map(|error| finding(error, doc))
                            .collect(),
                        Err(unmerged) => vec![unmerged.finding()],
                    }
                }
            };
            locked(&self.checks).insert(name, found);
            Ok::<_, Refused>(self.snapshot_of(&config).await)
        }
        .await;
        Msg::RoleChecked {
            ticket,
            role: role.to_owned(),
            result: result.map_err(|error| text(&error)),
        }
    }

    pub(crate) async fn save_role(
        &self,
        role: &str,
        base: &RoleManifestDoc,
        doc: &RoleManifestDoc,
    ) -> Msg {
        let result = async {
            let config = self.config.load().await?;
            let name = RoleName::parse(role).map_err(PersistError::from)?;
            let entry = config.role(&name).ok_or_else(|| {
                Refused::Rejected(Rejection::UnknownRole {
                    role: name.to_string(),
                })
            })?;
            let RoleSource::Path(root) = &entry.source else {
                return Err(
                    PersistError::from(Rejection::CannotSave(SaveField::RoleSource)).into(),
                );
            };
            let file =
                HostPath::resolve(Path::new(MANIFEST_FILE), root).map_err(PersistError::from)?;
            let _held = self.hold_manifest(&file).await?;
            let mut manifest = self
                .roles
                .read_manifest(root)
                .await
                .map_err(Refused::RoleErrors)?;
            merge_manifest(&mut manifest, base, doc).map_err(|unmerged| unmerged.refused)?;
            manifest
                .write(self.host.fs.as_ref(), &file)
                .await
                .map_err(|error: ManifestError| Refused::Text(text(&error)))?;
            locked(&self.docs).remove(&name);
            Ok::<_, Refused>(self.snapshot_of(&config).await)
        }
        .await;
        Msg::RoleSaved {
            role: role.to_owned(),
            result: result.map_err(|error| text(&error)),
        }
    }

    async fn hold_manifest(&self, file: &HostPath) -> Result<FileLock, Refused> {
        let dir = Path::new(&self.config_path)
            .parent()
            .ok_or(Refused::NoConfigDir)
            .and_then(|dir| {
                HostPath::parse(dir).map_err(|error| Refused::from(PersistError::from(error)))
            })?;
        let key = Sha256::of(file.to_string().as_bytes());
        let name = Path::new(MANIFEST_FILE).with_extension(&key.as_str()[..LOCK_KEY_LEN]);
        let target = HostPath::resolve(&name, &dir).map_err(PersistError::from)?;
        let lock = lock_path(&target).map_err(|error| Refused::Text(text(&error)))?;
        hold_lock(
            self.host.fs.as_ref(),
            self.host.clock.as_ref(),
            &lock,
            MANIFEST_LOCK,
            &self.cancel,
        )
        .await
        .map_err(|error| match error {
            PollError::Cancelled => Refused::ManifestCancelled,
            PollError::Timeout { .. } => Refused::ManifestBusy,
            PollError::Probe(error) => Refused::Text(text(&error)),
        })
    }

    pub(crate) async fn trust_role(&self, role: &str) -> Msg {
        let result = async {
            let config = self.config.load().await?;
            let name = RoleName::parse(role).map_err(PersistError::from)?;
            config.role(&name).ok_or_else(|| {
                Refused::Rejected(Rejection::UnknownRole {
                    role: name.to_string(),
                })
            })?;
            Ok::<_, Refused>(self.snapshot_of(&config).await)
        }
        .await;
        Msg::RoleTrusted {
            role: role.to_owned(),
            result: result.map_err(|error| text(&error)),
        }
    }
}

struct Unmerged {
    area: FindingArea,
    refused: Refused,
}

impl Unmerged {
    fn on(area: FindingArea) -> impl Fn(PersistError) -> Unmerged {
        move |error| Unmerged {
            area,
            refused: error.into(),
        }
    }

    fn finding(&self) -> RoleFinding {
        RoleFinding {
            area: self.area,
            key: None,
            message: text(&self.refused),
            fix: None,
        }
    }
}

fn merge_manifest(
    manifest: &mut RoleManifest,
    base: &RoleManifestDoc,
    doc: &RoleManifestDoc,
) -> Result<(), Unmerged> {
    let repo = |value: &str, area| {
        RepoPath::parse(value)
            .map_err(PersistError::from)
            .map_err(Unmerged::on(area))
    };
    if doc.name != base.name {
        return Err(Unmerged::on(FindingArea::Manifest)(PersistError::from(
            Rejection::CannotSave(SaveField::RoleName),
        )));
    }
    if doc.dockerfile != base.dockerfile {
        manifest.role.dockerfile = repo(&doc.dockerfile, FindingArea::Dockerfile)?;
    }
    if doc.policy != base.policy {
        manifest.role.policy = repo(&doc.policy, FindingArea::Policy)?;
    }
    if doc.codex_model != base.codex_model {
        let model = doc
            .codex_model
            .as_deref()
            .map(ModelName::parse)
            .transpose()
            .map_err(PersistError::from)
            .map_err(Unmerged::on(FindingArea::Manifest))?;
        match manifest.agents.entry(jackin_core::Agent::Codex) {
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                entry.get_mut().model = model;
            }
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(jackin_core::AgentSection {
                    model,
                    instructions: RepoPath::parse(jackin_core::CODEX_INSTRUCTIONS)
                        .map_err(PersistError::from)
                        .map_err(Unmerged::on(FindingArea::Manifest))?,
                });
            }
        }
    }
    if doc.docker != base.docker {
        manifest.docker.enabled = doc.docker;
    }
    if doc.hook_setup != base.hook_setup {
        manifest.hooks.setup = doc
            .hook_setup
            .as_deref()
            .map(|value| repo(value, FindingArea::Hook))
            .transpose()?;
    }
    if doc.hook_preflight != base.hook_preflight {
        manifest.hooks.preflight = doc
            .hook_preflight
            .as_deref()
            .map(|value| repo(value, FindingArea::Hook))
            .transpose()?;
    }
    let names: BTreeSet<&str> = base
        .env
        .iter()
        .chain(&doc.env)
        .map(|entry| entry.name.as_str())
        .collect();
    for name in names {
        let before = base.env.iter().find(|entry| entry.name == name);
        let after = doc.env.iter().find(|entry| entry.name == name);
        if before == after {
            continue;
        }
        let key = EnvName::parse(name)
            .map_err(PersistError::from)
            .map_err(Unmerged::on(FindingArea::Manifest))?;
        match after {
            Some(entry) => {
                manifest.env.insert(
                    key,
                    EnvSpec {
                        default: entry.default.clone(),
                        prompt: entry.prompt.clone(),
                    },
                );
            }
            None => {
                manifest.env.remove(&key);
            }
        }
    }
    Ok(())
}
