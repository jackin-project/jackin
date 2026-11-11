// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use futures::future::join_all;
use jackin_core::{Config, FileSystem, HostPath, ProjectRoot};
use jackin_session::{GithubOrigin, Session};
use tokio_util::sync::CancellationToken;

use crate::project::Origins;

const FRESH_SECS: i64 = 60;

type Entry = (i64, Option<GithubOrigin>);

#[derive(Debug, Default)]
pub(crate) struct OriginCache {
    entries: Mutex<BTreeMap<HostPath, Entry>>,
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl OriginCache {
    pub(crate) async fn read(
        &self,
        session: &Session,
        fs: &dyn FileSystem,
        config: &Config,
        now_secs: i64,
        cancel: &CancellationToken,
    ) -> Origins {
        let stale: Vec<HostPath> = {
            let entries = locked(&self.entries);
            config
                .workspaces()
                .map(|(_, workspace)| workspace.path.clone())
                .filter(|path| {
                    entries
                        .get(path)
                        .is_none_or(|(at, _)| now_secs.saturating_sub(*at) >= FRESH_SECS)
                })
                .collect()
        };
        let read = join_all(stale.into_iter().map(|path| async move {
            let origin = match ProjectRoot::open(fs, &path).await {
                Ok(project) => session
                    .project_origin(&project, cancel)
                    .await
                    .ok()
                    .flatten(),
                Err(_) => None,
            };
            (path, (now_secs, origin))
        }))
        .await;
        let mut entries = locked(&self.entries);
        entries.extend(read);
        entries.retain(|path, _| config.workspaces().any(|(_, w)| w.path == *path));
        config
            .workspaces()
            .filter_map(|(name, workspace)| {
                let (_, origin) = entries.get(&workspace.path)?;
                Some((name.clone(), origin.clone()?))
            })
            .collect()
    }
}
