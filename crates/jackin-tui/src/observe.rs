// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use futures::future::join_all;
use jackin_console::domain::doctor::{
    DoctorFailure, DoctorReport, DoctorSurface, STALE_AFTER_SECS,
};
use jackin_console::domain::instance::InstanceId;
use jackin_console::domain::vocabulary::{Notice, Rejection, Vocabulary};
use jackin_console::world::{Snapshot, WallSource};
use jackin_core::{
    Compatibility, Config, ConfigError, ConfigStore, EnvSnapshot, HostPath, RoleName,
    RoleSource as CoreRoleSource, Workspace as CoreWorkspace,
};
use jackin_session::{
    AccountDesk, DoctorInput, HostPorts, OnePasswordCli, RoleCatalog, SourceDesk, doctor,
};
use jackin_usage::HostUsage;
use tokio_util::sync::CancellationToken;

use crate::console::ConsoleLimits;
use crate::doctor::{self, DoctorCell};
use crate::fleet::{
    Fleet, ObservedInstance, ObservedSandbox, ObservedUnreadableSandbox, read_fleet,
};
use crate::ids::IdTable;
use crate::onepassword::OpSnapshot;
use crate::origin::OriginCache;
use crate::project::{
    Facts, Origins, ProjectInput, RoleChecks, RoleDocError, RoleDocs, observed_instance, project,
};
use crate::readings::Readings;
use crate::sessions::{Access, Reached, SessionConnector, SessionSet};

#[derive(Debug)]
pub(crate) struct Observation {
    pub(crate) snapshot: Snapshot,
    pub(crate) notices: Vec<Notice>,
}

impl Observation {
    pub(crate) fn status(&self, words: &Vocabulary) -> Option<String> {
        (!self.notices.is_empty()).then(|| words.notices(&self.notices))
    }
}

pub(crate) type DocCache = BTreeMap<
    RoleName,
    (
        CoreRoleSource,
        Result<jackin_core::RoleManifest, RoleDocError>,
    ),
>;

pub(crate) struct Inputs {
    pub(crate) config: ConfigStore,
    pub(crate) desk: AccountDesk,
    pub(crate) usage: Arc<HostUsage>,
    pub(crate) usage_gate: tokio::sync::Mutex<()>,
    refresh_gate: tokio::sync::Mutex<()>,
    pub(crate) op: OpSnapshot,
    pub(crate) op_cli: OnePasswordCli,
    pub(crate) sources: SourceDesk,
    pub(crate) sessions: SessionSet,
    pub(crate) roles: Arc<RoleCatalog>,
    pub(crate) host: HostPorts,
    pub(crate) connect: Arc<dyn SessionConnector>,
    pub(crate) wall: Arc<dyn WallSource>,
    pub(crate) engine_socket: HostPath,
    pub(crate) pinned: Compatibility,
    pub(crate) cancel: CancellationToken,
    pub(crate) doctor: DoctorCell,
    pub(crate) version: String,
    pub(crate) home: String,
    pub(crate) config_path: String,
    pub(crate) limits: ConsoleLimits,
    pub(crate) ids: Mutex<IdTable>,
    pub(crate) docs: Mutex<DocCache>,
    fleet: Mutex<Fleet>,
    pub(crate) checks: Mutex<RoleChecks>,
    pub(crate) readings: Mutex<Readings>,
    origins: OriginCache,
}

impl fmt::Debug for Inputs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Inputs").finish_non_exhaustive()
    }
}

pub(crate) fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Debug)]
pub(crate) struct InputsParts {
    pub config: ConfigStore,
    pub desk: AccountDesk,
    pub usage: Arc<HostUsage>,
    pub op: OpSnapshot,
    pub env: EnvSnapshot,
    pub sessions: SessionSet,
    pub roles: Arc<RoleCatalog>,
    pub host: HostPorts,
    pub connect: Arc<dyn SessionConnector>,
    pub wall: Arc<dyn WallSource>,
    pub engine_socket: HostPath,
    pub pinned: Compatibility,
    pub cancel: CancellationToken,
    pub version: String,
    pub home: String,
    pub config_path: String,
    pub limits: ConsoleLimits,
}

impl Inputs {
    pub(crate) fn new(parts: InputsParts) -> Inputs {
        let InputsParts {
            config,
            desk,
            usage,
            op,
            env,
            sessions,
            roles,
            host,
            connect,
            wall,
            engine_socket,
            pinned,
            cancel,
            version,
            home,
            config_path,
            limits,
        } = parts;
        let op_cli = OnePasswordCli::new(Arc::clone(&host.runner));
        let sources = SourceDesk::new(Arc::clone(&host.fs), env, op_cli.clone());
        Inputs {
            config,
            desk,
            usage,
            usage_gate: tokio::sync::Mutex::new(()),
            refresh_gate: tokio::sync::Mutex::new(()),
            op,
            op_cli,
            sources,
            sessions,
            roles,
            host,
            connect,
            wall,
            engine_socket,
            pinned,
            cancel,
            doctor: DoctorCell::default(),
            version,
            home,
            config_path,
            limits,
            ids: Mutex::new(IdTable::new()),
            docs: Mutex::new(DocCache::new()),
            fleet: Mutex::new(Fleet::default()),
            checks: Mutex::new(RoleChecks::new()),
            readings: Mutex::new(Readings::default()),
            origins: OriginCache::default(),
        }
    }

    async fn refreshed(&self) -> Result<Config, ConfigError> {
        let _turn = self.refresh_gate.lock().await;
        self.reloaded().await
    }

    async fn reloaded(&self) -> Result<Config, ConfigError> {
        let config = self.config.load().await?;
        self.sessions
            .reconcile(
                &config,
                self.connect.as_ref(),
                self.limits.connect_deadline,
                &self.cancel,
            )
            .await;
        Ok(config)
    }

    pub(crate) async fn observe(&self) -> Result<Observation, ConfigError> {
        let _turn = self.refresh_gate.lock().await;
        let config = self.reloaded().await?;
        let fleet = read_fleet(&self.sessions, &config, self.limits, &self.cancel).await;
        Ok(self.observation(&config, fleet).await)
    }

    pub(crate) async fn current(&self, config: &Config) -> Observation {
        let fleet = locked(&self.fleet).clone();
        self.observation(config, fleet).await
    }

    async fn observation(&self, config: &Config, fleet: Fleet) -> Observation {
        *locked(&self.fleet) = fleet.clone();
        let docs = self.role_docs(config).await;
        let origins = self.origins(config).await;
        let checks = locked(&self.checks).clone();
        let facts = Facts {
            version: self.version.clone(),
            config_path: self.config_path.clone(),
            home: self.home.clone(),
            now_secs: self.host.clock.now().0,
        };
        let mut snapshot = project(
            &ProjectInput {
                config,
                fleet: &fleet,
                facts: &facts,
                roles: &docs,
                checks: &checks,
                origins: &origins,
            },
            &mut locked(&self.ids),
        );
        locked(&self.readings).overlay(config, &mut snapshot.accounts);
        snapshot.surfaces.doctor = Some(DoctorSurface {
            noun: snapshot.surfaces.vocabulary.health_noun.clone(),
            state: self.doctor.state(),
            rerun_secs: Some(STALE_AFTER_SECS),
        });
        let mut notices: Vec<Notice> = fleet
            .failed()
            .map(|g| Notice::GatewayUnreachable {
                gateway: g.name.to_string(),
                reason: g.failure.clone().unwrap_or_default(),
            })
            .collect();
        if fleet.unreadable > 0 {
            notices.push(Notice::UnreadableLabels {
                count: fleet.unreadable,
            });
        }
        Observation { snapshot, notices }
    }

    async fn origins(&self, config: &Config) -> Origins {
        let session = self
            .sessions
            .default_name()
            .and_then(|name| self.sessions.get(&name))
            .or_else(|| {
                self.sessions
                    .reached()
                    .into_iter()
                    .find_map(|(_, reached)| match reached {
                        Reached::Up(session) => Some(*session),
                        Reached::Down(_) => None,
                    })
            });
        let Some(session) = session else {
            return Origins::new();
        };
        let read = self.origins.read(
            &session,
            self.host.fs.as_ref(),
            config,
            self.host.clock.now().0,
            &self.cancel,
        );
        tokio::time::timeout(self.limits.connect_deadline, read)
            .await
            .unwrap_or_default()
    }

    pub(crate) fn admit(
        &self,
        config: &Config,
        observed: ObservedSandbox,
        origins: &Origins,
    ) -> jackin_console::domain::instance::InstanceEntry {
        let instance = observed_instance(
            &observed,
            config,
            origins,
            &locked(&self.ids),
            self.host.clock.now().0,
        );
        locked(&self.fleet).admit(observed);
        instance
    }

    pub(crate) fn admit_unreadable(
        &self,
        gateway: jackin_core::GatewayName,
        sandbox: jackin_session::UnreadableSandbox,
        config: &Config,
    ) -> jackin_console::domain::instance::InstanceEntry {
        let observed = ObservedUnreadableSandbox { gateway, sandbox };
        let instance = crate::project::unreadable_instance(&observed, config, &locked(&self.ids));
        locked(&self.fleet).admit_unreadable(observed);
        instance
    }

    pub(crate) fn locate(&self, instance: &InstanceId) -> Option<ObservedInstance> {
        locked(&self.fleet).locate(instance)
    }

    pub(crate) async fn run_doctor(&self, reconnect: bool) -> Result<DoctorReport, DoctorFailure> {
        let config = if reconnect {
            self.refreshed().await.ok()
        } else {
            None
        };
        let run = async {
            if let Some(config) = &config {
                self.sessions
                    .reconnect(self.connect.as_ref(), config, &self.cancel)
                    .await;
            }
            let (name, access) = match self.sessions.default_target() {
                Some((name, access)) => (Some(name), access),
                None => (None, Access::NotChosen),
            };
            let input = DoctorInput {
                runner: self.host.runner.as_ref(),
                fs: self.host.fs.as_ref(),
                gateway: access.borrowed(),
                engine_socket: self.engine_socket.clone(),
                pinned: &self.pinned,
            };
            let checked = doctor(&input, &self.cancel).await;
            doctor::report(
                &checked,
                name.as_ref()
                    .filter(|_| !matches!(access, Access::NotChosen)),
                self.wall.epoch_millis().div_euclid(1000),
            )
        };
        let report = tokio::time::timeout(self.limits.connect_deadline, run)
            .await
            .map_err(|_| DoctorFailure::TimedOut)?;
        self.doctor.record(report.clone());
        Ok(report)
    }

    pub(crate) async fn doctor_at_start(&self) {
        drop(self.run_doctor(false).await);
    }

    async fn role_docs(&self, config: &Config) -> RoleDocs {
        let wanted: Vec<(RoleName, CoreRoleSource)> = {
            let cache = locked(&self.docs);
            config
                .roles()
                .filter(|(name, entry)| cache.get(*name).is_none_or(|(s, _)| *s != entry.source))
                .map(|(name, entry)| (name.clone(), entry.source.clone()))
                .collect()
        };
        let anywhere = HostPath::parse(Path::new(&self.home))
            .ok()
            .map(|home| CoreWorkspace::new(home, false));
        let loaded = join_all(wanted.into_iter().map(|(name, source)| {
            let anywhere = anywhere.clone();
            Box::pin(async move {
                let doc = match &source {
                    CoreRoleSource::Path(path) => {
                        match self.roles.load_path(path, &CancellationToken::new()).await {
                            Ok(role) => Ok(role.manifest().clone()),
                            Err(error) => match self.roles.read_manifest(path).await {
                                Ok(manifest) => Ok(manifest),
                                Err(_) => Err(RoleDocError::Unreadable(error.to_string())),
                            },
                        }
                    }
                    CoreRoleSource::Git { .. } => match &anywhere {
                        Some(anywhere) => match Box::pin(self.roles.load(
                            config,
                            Some(&name),
                            anywhere,
                            &self.cancel,
                        ))
                        .await
                        {
                            Ok(role) => Ok(role.manifest().clone()),
                            Err(error) => Err(RoleDocError::Unreadable(error.to_string())),
                        },
                        None => Err(RoleDocError::Rejected(Rejection::RoleNotLoaded)),
                    },
                };
                (name, (source, doc))
            })
        }))
        .await;
        let mut cache = locked(&self.docs);
        cache.extend(loaded);
        cache.retain(|name, _| config.role(name).is_some());
        cache
            .iter()
            .map(|(name, (_, doc))| (name.clone(), doc.clone()))
            .collect()
    }
}
