// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use futures::future::join_all;
use jackin_console::domain::doctor::{DoctorReport, DoctorSurface};
use jackin_console::world::{Snapshot, WallSource};
use jackin_core::{
    AccountName, Compatibility, Config, ConfigError, ConfigStore, GatewayName, HostPath,
    RoleManifest, RoleName, RoleSource as CoreRoleSource,
};
use jackin_session::{DoctorInput, HostPorts, MANIFEST_FILE, RoleCatalog, doctor};
use tokio_util::sync::CancellationToken;

use crate::doctor::{self, DoctorCell};
use crate::fleet::{Fleet, GatewayReading, read_fleet};
use crate::ids::IdTable;
use crate::origin::OriginCache;
use crate::project::{
    Facts, Origins, ProjectInput, RoleChecks, RoleDocError, RoleDocs, manifest_doc, project,
    unmapped_accounts,
};
use crate::sessions::{Access, Reached, SessionConnector, SessionSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Note {
    GatewayUnreachable { gateway: GatewayName, error: String },
    UnreadableLabels(usize),
    AccountNotShown(AccountName),
}

impl fmt::Display for Note {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Note::GatewayUnreachable { gateway, error } => {
                write!(f, "Could not list sandboxes on {gateway}: {error}")
            }
            Note::UnreadableLabels(count) => {
                write!(f, "{count} sandboxes have labels jackin cannot read")
            }
            Note::AccountNotShown(name) => {
                write!(f, "Account {name} has a provider the console cannot show")
            }
        }
    }
}

struct Notes<'a>(&'a [Note]);

impl fmt::Display for Notes<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, note) in self.0.iter().enumerate() {
            if index > 0 {
                f.write_str(" · ")?;
            }
            write!(f, "{note}")?;
        }
        Ok(())
    }
}

struct TimedOut(Duration);

impl fmt::Display for TimedOut {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "no answer within {} s", self.0.as_secs())
    }
}

#[derive(Debug)]
pub(crate) struct Observation {
    pub(crate) snapshot: Snapshot,
    pub(crate) notes: Vec<Note>,
    pub(crate) healthy: bool,
}

impl Observation {
    pub(crate) fn status(&self) -> Option<String> {
        (!self.notes.is_empty()).then(|| Notes(&self.notes).to_string())
    }
}

pub(crate) type DocCache = BTreeMap<
    RoleName,
    (
        CoreRoleSource,
        Result<jackin_console::domain::role::RoleManifestDoc, RoleDocError>,
    ),
>;

pub(crate) struct Inputs {
    pub(crate) config: ConfigStore,
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
    pub(crate) deadline: Duration,
    pub(crate) ids: Mutex<IdTable>,
    pub(crate) docs: Mutex<DocCache>,
    pub(crate) fleet: Mutex<Fleet>,
    pub(crate) checks: Mutex<RoleChecks>,
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
    pub deadline: Duration,
}

impl Inputs {
    pub(crate) fn new(parts: InputsParts) -> Inputs {
        let InputsParts {
            config,
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
            deadline,
        } = parts;
        Inputs {
            config,
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
            deadline,
            ids: Mutex::new(IdTable::new()),
            docs: Mutex::new(DocCache::new()),
            fleet: Mutex::new(Fleet::default()),
            checks: Mutex::new(RoleChecks::new()),
            origins: OriginCache::default(),
        }
    }

    pub(crate) async fn observe(&self) -> Result<Observation, ConfigError> {
        let config = self.config.load().await?;
        let fleet =
            match tokio::time::timeout(self.deadline, read_fleet(&self.sessions, &config)).await {
                Ok(fleet) => fleet,
                Err(_) => Fleet {
                    sandboxes: Vec::new(),
                    unreadable: 0,
                    gateways: self
                        .sessions
                        .reached()
                        .into_iter()
                        .map(|(name, _)| GatewayReading {
                            name,
                            failure: Some(TimedOut(self.deadline).to_string()),
                        })
                        .collect(),
                },
            };
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
        snapshot.surfaces.doctor = Some(DoctorSurface {
            noun: snapshot.surfaces.vocabulary.health_noun.clone(),
            state: self.doctor.state(),
        });
        let mut notes: Vec<Note> = fleet
            .failed()
            .map(|g| Note::GatewayUnreachable {
                gateway: g.name.clone(),
                error: g.failure.clone().unwrap_or_default(),
            })
            .collect();
        if fleet.unreadable > 0 {
            notes.push(Note::UnreadableLabels(fleet.unreadable));
        }
        notes.extend(
            unmapped_accounts(config)
                .into_iter()
                .map(Note::AccountNotShown),
        );
        Observation {
            snapshot,
            healthy: fleet.failed().next().is_none(),
            notes,
        }
    }

    async fn origins(&self, config: &Config) -> Origins {
        let session = self.sessions.get(self.sessions.default_name()).or_else(|| {
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
        tokio::time::timeout(self.deadline, read)
            .await
            .unwrap_or_default()
    }

    pub(crate) async fn run_doctor(&self, reconnect: bool) -> DoctorReport {
        if reconnect && let Ok(config) = self.config.load().await {
            self.sessions
                .reconnect(self.connect.as_ref(), &config, &self.cancel)
                .await;
        }
        let name = self.sessions.default_name();
        let access = self.sessions.access(name);
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
            (!matches!(access, Access::NotChosen)).then_some(name),
            self.wall.epoch_millis().div_euclid(1000),
        )
    }

    pub(crate) async fn doctor_at_start(&self) {
        let Some(run) = self.doctor.begin() else {
            return;
        };
        if let Ok(report) = tokio::time::timeout(self.deadline, self.run_doctor(false)).await {
            run.finish(report);
        }
    }

    async fn manifest_only(&self, root: &HostPath) -> Option<RoleManifest> {
        let file = HostPath::resolve(std::path::Path::new(MANIFEST_FILE), root).ok()?;
        let bytes = self.host.fs.read(&file).await.ok().flatten()?;
        RoleManifest::parse(&String::from_utf8_lossy(&bytes)).ok()
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
        let loaded = join_all(wanted.into_iter().map(|(name, source)| async move {
            let doc = match &source {
                CoreRoleSource::Path(path) => {
                    match self.roles.load_path(path, &CancellationToken::new()).await {
                        Ok(role) => Ok(manifest_doc(role.manifest())),
                        Err(error) => match self.manifest_only(path).await {
                            Some(manifest) => Ok(manifest_doc(&manifest)),
                            None => Err(RoleDocError::Unreadable(error.to_string())),
                        },
                    }
                }
                CoreRoleSource::Git { .. } => Err(RoleDocError::NotLoaded),
            };
            (name, (source, doc))
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
