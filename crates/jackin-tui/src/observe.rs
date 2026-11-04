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
    AccountName, Compatibility, Config, ConfigError, ConfigStore, GatewayName, HostPath, RoleName,
    RoleSource as CoreRoleSource,
};
use jackin_session::{DoctorInput, HostPorts, RoleCatalog, doctor};
use tokio_util::sync::CancellationToken;

use crate::doctor::{self, DoctorCell};
use crate::fleet::{Fleet, GatewayReading, read_fleet};
use crate::ids::IdTable;
use crate::project::{
    Facts, ProjectInput, RoleDocError, RoleDocs, manifest_doc, project, unmapped_accounts,
};
use crate::sessions::{Access, SessionConnector, SessionSet};

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

type DocCache = BTreeMap<
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
    ids: Mutex<IdTable>,
    docs: Mutex<DocCache>,
}

impl fmt::Debug for Inputs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Inputs").finish_non_exhaustive()
    }
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
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
        let docs = self.role_docs(&config).await;
        let facts = Facts {
            version: self.version.clone(),
            config_path: self.config_path.clone(),
            home: self.home.clone(),
            now_secs: self.host.clock.now().0,
        };
        let mut snapshot = project(
            &ProjectInput {
                config: &config,
                fleet: &fleet,
                facts: &facts,
                roles: &docs,
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
            unmapped_accounts(&config)
                .into_iter()
                .map(Note::AccountNotShown),
        );
        Ok(Observation {
            snapshot,
            healthy: fleet.failed().next().is_none(),
            notes,
        })
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
                        Err(error) => Err(RoleDocError::Unreadable(error.to_string())),
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
