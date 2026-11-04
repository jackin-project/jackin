// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use jackin_console::backend::Ticket;
use jackin_console::domain::account::{AccountId, AccountRegistry};
use jackin_console::domain::agent::Agent;
use jackin_console::domain::instance::InstanceId;
use jackin_console::domain::role::{FindingArea, RoleFinding, RoleManifestDoc};
use jackin_console::domain::vocabulary::{Rejection, SaveField, Unbuilt, Vocabulary};
use jackin_console::domain::workspace::{MountSource, Workspace, WorkspaceId};
use jackin_console::world::{GlobalConfig, Msg, SaveFailure, Snapshot};
use jackin_core::plain;
use jackin_core::{
    Agent as CoreAgent, Config, ConfigEdit, ConfigError, EnvName, FileKind, HostPath, ModelName,
    RepoPath, RoleManifest, RoleName, RoleSource, WorkspaceName,
};
use jackin_core::{EnvSpec, ManifestError, Workspace as CoreWorkspace};
use jackin_openshell::PowerAction;
use jackin_session::{
    AccountPlan, MANIFEST_FILE, PurgeMode, RoleError, error_chain, plan_account_edits,
    read_chatgpt_login,
};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::bridge::Settled;
use crate::changes::read_changes;
use crate::observe::{Inputs, Observation, locked};
use crate::persist::{self, Folder, PersistError, Resolved, sentence};
use crate::project::{manifest_doc, sandbox_id};
use crate::sessions::{Access, Home};

#[derive(Debug, Error)]
enum Refused {
    #[error("{}", sentence(.0))]
    Rejected(Rejection),
    #[error("{}", plain(.0))]
    Text(String),
    #[error(transparent)]
    Persist(#[from] PersistError),
    #[error(transparent)]
    Config(#[from] ConfigError),
}

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
    match agent {
        Agent::Codex => Ok(CoreAgent::Codex),
        Agent::ClaudeCode | Agent::Amp | Agent::KimiCode | Agent::OpenCode | Agent::GrokBuild => {
            Err(Rejection::CannotSave(SaveField::Agent).into())
        }
    }
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

impl Inputs {
    async fn saved_snapshot(&self) -> Option<Snapshot> {
        let config = self.config.load().await.ok()?;
        Some(self.current(&config).await.snapshot)
    }

    async fn failed(&self, reply: Msg) -> Settled {
        Settled::reverted(reply, self.saved_snapshot().await)
    }

    async fn applied(&self, config: &Config, edits: Vec<ConfigEdit>) -> Result<Config, Refused> {
        if edits.is_empty() {
            return Ok(config.clone());
        }
        Ok(self.config.edit(edits).await?)
    }

    async fn snapshot_of(&self, config: &Config) -> Box<Snapshot> {
        let Observation { snapshot, .. } = self.current(config).await;
        Box::new(snapshot)
    }

    pub(crate) fn expanded(&self, text: &str) -> Result<HostPath, jackin_core::PathError> {
        let expanded = match text.strip_prefix('~') {
            Some(rest) if rest.is_empty() || rest.starts_with('/') => {
                Path::new(&self.home).join(rest.trim_start_matches('/'))
            }
            _ => Path::new(text).to_path_buf(),
        };
        HostPath::parse(&expanded)
    }

    async fn folder(&self, text: &str) -> Result<HostPath, PersistError> {
        let path = self.expanded(text)?;
        let folder = || {
            PersistError::from(Rejection::NotAFolder {
                folder: text.to_owned(),
            })
        };
        let canonical = self.host.fs.canonicalize(&path).await?.ok_or_else(folder)?;
        match self.host.fs.stat_following(&canonical).await? {
            Some(FileKind::Dir) => Ok(canonical),
            _ => Err(folder()),
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
        match self.host.fs.stat_following(&file).await? {
            Some(FileKind::File) => Ok(file),
            _ => Err(unusable()),
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
    ) -> Result<(WorkspaceId, Box<Snapshot>), Refused> {
        let config = self.config.load().await?;
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
        let saved = self.applied(&config, edits).await?;
        let name = WorkspaceName::parse(&workspace.name).map_err(PersistError::from)?;
        if let Some(old) = base.filter(|b| b.name != workspace.name) {
            let old = WorkspaceName::parse(&old.name).map_err(PersistError::from)?;
            locked(&self.ids).rename(&old, &name);
        }
        let snapshot = self.snapshot_of(&saved).await;
        let id = locked(&self.ids)
            .lookup(&name)
            .ok_or_else(|| Refused::Text(name.to_string()))?;
        Ok((id, snapshot))
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

    async fn try_delete_workspace(&self, id: WorkspaceId) -> Result<Box<Snapshot>, Refused> {
        let name = locked(&self.ids)
            .name(id)
            .cloned()
            .ok_or_else(|| Refused::Text(id.to_string()))?;
        let saved = self
            .config
            .edit(vec![ConfigEdit::RemoveWorkspace(name)])
            .await?;
        Ok(self.snapshot_of(&saved).await)
    }

    pub(crate) async fn save_global(&self, base: &GlobalConfig, global: &GlobalConfig) -> Settled {
        let result = async {
            let config = self.config.load().await?;
            let edits = persist::global(base, global)?;
            let saved = self.applied(&config, edits).await?;
            Ok::<_, Refused>(self.snapshot_of(&saved).await)
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
            let folder = self.folder(&path).await?;
            let provider = crate::project::core_provider(provider);
            let resolved_folder = if provider == jackin_core::Provider::Codex {
                let found = read_chatgpt_login(self.host.fs.as_ref(), &folder)
                    .await
                    .map_err(|error| Refused::Text(text(&error)))?;
                Folder::Chatgpt {
                    home: folder,
                    account: found.account_id,
                }
            } else {
                Folder::Login {
                    file: self.login_file(provider, &folder, &path).await?,
                }
            };
            resolved.folders.insert(path, resolved_folder);
        }
        Ok(resolved)
    }

    async fn apply_accounts(&self, edits: Vec<ConfigEdit>) -> Result<Config, Refused> {
        if edits.is_empty() {
            return Ok(self.config.load().await?);
        }
        let plan = plan_account_edits(&self.desk, edits, &self.cancel)
            .await
            .map_err(|error| Refused::Text(text(&error)))?;
        match plan {
            AccountPlan::Offline(plan) => Ok(plan.apply().await?),
            AccountPlan::Gateway(plan) => {
                let home = self
                    .sessions
                    .home(plan.config(), self.connect.as_ref(), &self.cancel)
                    .await;
                match home {
                    Home::Ready(gateways) => plan
                        .apply(&gateways)
                        .await
                        .map_err(|error| Refused::Text(text(&error))),
                    Home::Down(error) => Err(Refused::Text(text(&*error))),
                    Home::NotChosen => Err(ConfigError::NoGateway.into()),
                }
            }
        }
    }

    pub(crate) async fn save_accounts(
        &self,
        base: &AccountRegistry,
        registry: &AccountRegistry,
    ) -> Settled {
        let result = async {
            let resolved = self.resolve_accounts(base, registry).await?;
            let edits = persist::accounts(base, registry, &resolved)?;
            let added = persist::added(registry)?;
            let saved = self.apply_accounts(edits).await?;
            Ok::<_, Refused>((self.snapshot_of(&saved).await, added))
        }
        .await;
        match result {
            Ok((snapshot, added)) => Msg::AccountsSaved {
                added,
                result: Ok(snapshot),
            }
            .into(),
            Err(error) => {
                let reply = Msg::AccountsSaved {
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
        locked(&self.fleet)
            .sandboxes
            .iter()
            .find(|observed| &sandbox_id(&observed.summary.name) == instance)
            .map(|observed| (observed.gateway.clone(), observed.summary.name.clone()))
            .ok_or_else(|| {
                Refused::Rejected(Rejection::NotInRoster {
                    instance: instance.clone(),
                })
            })
    }

    pub(crate) fn pane_target(
        &self,
        instance: &InstanceId,
    ) -> Result<(jackin_session::Session, jackin_core::SandboxName), String> {
        let (gateway, name) = self.sandbox(instance).map_err(|error| plain(&error))?;
        let session = self.session(&gateway).map_err(|error| plain(&error))?;
        Ok((session, name))
    }

    fn session(
        &self,
        gateway: &jackin_core::GatewayName,
    ) -> Result<jackin_session::Session, Refused> {
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

    pub(crate) async fn stop(&self, instance: &InstanceId) -> Msg {
        let result = async {
            let (gateway, name) = self.sandbox(instance)?;
            self.session(&gateway)?
                .power(&name, PowerAction::Stop, &self.cancel)
                .await
                .map_err(|error| Refused::Text(text(&error)))?;
            self.after_lifecycle().await
        }
        .await;
        Msg::Stopped {
            instance: instance.clone(),
            result: result.map_err(|error| text(&error)),
        }
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
                    account: jackin_core::AccountName::parse(id).map_err(PersistError::from)?,
                },
                None => ConfigEdit::UnbindRoleAccount { role: name, agent },
            };
            let saved = self.config.edit(vec![edit]).await?;
            Ok::<_, Refused>(self.snapshot_of(&saved).await)
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
            let saved = self.config.edit(vec![ConfigEdit::RemoveRole(name)]).await?;
            Ok::<_, Refused>(self.snapshot_of(&saved).await)
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

    async fn try_add_role(&self, location: &str) -> Result<(String, Box<Snapshot>), Refused> {
        if jackin_core::RoleUrl::parse(location).is_ok() {
            return Err(Refused::Text(
                Vocabulary::openshell().unbuilt(Unbuilt::AddGitRole),
            ));
        }
        let root = self.folder(location).await?;
        let loaded = self
            .roles
            .load_path(&root, &CancellationToken::new())
            .await
            .map_err(|error| Refused::Text(text(&error)))?;
        let name = loaded.name().clone();
        let saved = self
            .config
            .edit(vec![ConfigEdit::AddRole {
                name: name.clone(),
                source: RoleSource::Path(root),
            }])
            .await?;
        Ok((name.to_string(), self.snapshot_of(&saved).await))
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
                locked(&self.docs).insert(
                    name,
                    (entry.source.clone(), Ok(manifest_doc(loaded.manifest()))),
                );
            }
            Ok::<_, Refused>(self.snapshot_of(&config).await)
        }
        .await;
        Msg::RoleLoaded {
            role: role.to_owned(),
            result: result.map_err(|error| text(&error)),
        }
    }

    pub(crate) async fn check_role(&self, role: &str, doc: &RoleManifestDoc) -> Msg {
        let result = async {
            let config = self.config.load().await?;
            let name = RoleName::parse(role).map_err(PersistError::from)?;
            let root = self.role_root(&config, &name).await?;
            let found: Vec<RoleFinding> = self
                .roles
                .findings(&root)
                .await
                .iter()
                .map(|error| finding(error, doc))
                .collect();
            locked(&self.checks).insert(name, found);
            Ok::<_, Refused>(self.snapshot_of(&config).await)
        }
        .await;
        Msg::RoleChecked {
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
            let bytes = self
                .host
                .fs
                .read(&file)
                .await
                .map_err(PersistError::from)?
                .unwrap_or_default();
            let mut manifest = RoleManifest::parse(&String::from_utf8_lossy(&bytes))
                .map_err(|error| Refused::Text(text(&error)))?;
            merge_manifest(&mut manifest, base, doc)?;
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

fn merge_manifest(
    manifest: &mut RoleManifest,
    base: &RoleManifestDoc,
    doc: &RoleManifestDoc,
) -> Result<(), Refused> {
    let repo = |value: &str| RepoPath::parse(value).map_err(PersistError::from);
    if doc.name != base.name {
        return Err(PersistError::from(Rejection::CannotSave(SaveField::RoleName)).into());
    }
    if doc.dockerfile != base.dockerfile {
        manifest.role.dockerfile = repo(&doc.dockerfile)?;
    }
    if doc.policy != base.policy {
        manifest.role.policy = repo(&doc.policy)?;
    }
    if doc.codex_model != base.codex_model {
        manifest.codex.model = doc
            .codex_model
            .as_deref()
            .map(ModelName::parse)
            .transpose()
            .map_err(PersistError::from)?;
    }
    if doc.docker != base.docker {
        manifest.docker.enabled = doc.docker;
    }
    if doc.hook_setup != base.hook_setup {
        manifest.hooks.setup = doc.hook_setup.as_deref().map(repo).transpose()?;
    }
    if doc.hook_preflight != base.hook_preflight {
        manifest.hooks.preflight = doc.hook_preflight.as_deref().map(repo).transpose()?;
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
        let key = EnvName::parse(name).map_err(PersistError::from)?;
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
