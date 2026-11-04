// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use futures::future::join_all;
use jackin_console::world::Snapshot;
use jackin_core::{
    AccountName, Clock, Config, ConfigError, ConfigStore, GatewayName, RoleName,
    RoleSource as CoreRoleSource,
};
use jackin_session::RoleCatalog;
use tokio_util::sync::CancellationToken;

use crate::console::ConsoleLimits;
use crate::fleet::{WorkCache, read_fleet};
use crate::ids::IdTable;
use crate::project::{
    Facts, ProjectInput, RoleDocError, RoleDocs, manifest_doc, project, unmapped_accounts,
};
use crate::sessions::SessionSet;

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
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) version: String,
    pub(crate) home: String,
    pub(crate) config_path: String,
    pub(crate) limits: ConsoleLimits,
    work: WorkCache,
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
    pub clock: Arc<dyn Clock>,
    pub version: String,
    pub home: String,
    pub config_path: String,
    pub limits: ConsoleLimits,
}

impl Inputs {
    pub(crate) fn new(parts: InputsParts) -> Inputs {
        let InputsParts {
            config,
            sessions,
            roles,
            clock,
            version,
            home,
            config_path,
            limits,
        } = parts;
        Inputs {
            config,
            sessions,
            roles,
            clock,
            version,
            home,
            config_path,
            limits,
            work: WorkCache::default(),
            ids: Mutex::new(IdTable::new()),
            docs: Mutex::new(DocCache::new()),
        }
    }

    pub(crate) async fn observe(&self) -> Result<Observation, ConfigError> {
        let config = self.config.load().await?;
        let fleet = read_fleet(&self.sessions, &config, self.limits, &self.work).await;
        let docs = self.role_docs(&config).await;
        let facts = Facts {
            version: self.version.clone(),
            config_path: self.config_path.clone(),
            home: self.home.clone(),
            now_secs: self.clock.now().0,
        };
        let snapshot = project(
            &ProjectInput {
                config: &config,
                fleet: &fleet,
                facts: &facts,
                roles: &docs,
            },
            &mut locked(&self.ids),
        );
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
