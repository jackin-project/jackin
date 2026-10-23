// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::future::Future;
use std::ops::Deref;
use std::path::Path;
#[cfg(test)]
use std::sync::Mutex;
use std::sync::{Arc, OnceLock};

use jackin_usage_store_backend::{
    Connection, LocalStore, ManagedStoreKind, StoreOperationGuard, acquire_store_operation,
};

use jackin_protocol::control::FocusedUsageView;

#[cfg(test)]
use super::CONNECTION_BUILDS;
use super::{account_snapshot_rows, initialize_schema, upsert_account_snapshot_rows};

static STORE_CONNECTIONS: OnceLock<tokio::sync::Mutex<HashMap<String, Arc<LocalStore>>>> =
    OnceLock::new();

pub(crate) struct StoreLease {
    _operation: StoreOperationGuard,
    store: Arc<LocalStore>,
}

impl Deref for StoreLease {
    type Target = Connection;

    fn deref(&self) -> &Self::Target {
        self.store.connection()
    }
}

#[cfg(any(test, feature = "test-support"))]
pub fn store_usage_snapshot(path: &Path, view: &FocusedUsageView) -> Result<(), String> {
    store_usage_snapshots(path, std::slice::from_ref(view))
}

pub fn store_usage_snapshots(path: &Path, views: &[FocusedUsageView]) -> Result<(), String> {
    let path = path.to_path_buf();
    let rows = views
        .iter()
        .map(account_snapshot_rows)
        .collect::<Result<Vec<_>, String>>()?
        .into_iter()
        .flatten()
        .collect();
    block_on_store(async move {
        let store = open_store(&path).await?;
        if let Err(error) = upsert_account_snapshot_rows(store.store.connection(), rows).await {
            remove_cached_store(&path).await?;
            return Err(error);
        }
        Ok(())
    })
}

pub(crate) fn block_on_store<T, Fut>(future: Fut) -> Result<T, String>
where
    Fut: Future<Output = Result<T, String>>,
{
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    let runtime = if let Some(runtime) = RUNTIME.get() {
        runtime
    } else {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .map_err(|err| format!("create usage snapshot store runtime failed: {err}"))?;
        RUNTIME.get_or_init(move || runtime)
    };
    runtime.block_on(future)
}

pub(crate) async fn open_store(path: &Path) -> Result<StoreLease, String> {
    // Acquire using the caller's path before creating parents. The backend
    // canonicalizes/pins even a missing leaf and parent, so directory setup
    // and every later connection/schema/data operation share one gate.
    let operation = acquire_store_operation(path)
        .await
        .map_err(|err| format!("acquire usage snapshot store operation failed: {err}"))?;
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("create usage snapshot store dir failed: {err}"))?;
    }
    let path = canonical_store_path(path)?;
    let path_string = path_to_turso(&path)?;
    let connections = STORE_CONNECTIONS.get_or_init(|| tokio::sync::Mutex::new(HashMap::new()));
    let cached = connections.lock().await.get(&path_string).cloned();
    if let Some(store) = cached {
        // A cached handle must still observe an external or partial schema
        // change, especially a future schema that this binary cannot write.
        if let Err(error) = initialize_schema(&store).await {
            // Migration finalization can fail with a transaction still open.
            // Drop the cached handle so a retry cannot inherit that state.
            remove_cached_store_by_key(&path_string).await;
            return Err(error);
        }
        return Ok(StoreLease {
            _operation: operation,
            store,
        });
    }
    let store = Arc::new(
        LocalStore::open_managed(
            Path::new(&path),
            ManagedStoreKind::QuotaSnapshot,
            &operation,
        )
        .await
        .map_err(|err| format!("open usage snapshot store failed: {err}"))?,
    );
    record_connection_build(&path_string);
    initialize_schema(&store).await?;
    connections
        .lock()
        .await
        .insert(path_string, Arc::clone(&store));
    Ok(StoreLease {
        _operation: operation,
        store,
    })
}

#[cfg(test)]
pub(crate) async fn close_cached_store(path: &Path) -> Result<(), String> {
    let path = canonical_store_path(path)?;
    let path = path_to_turso(&path)?;
    let _operation = acquire_store_operation(Path::new(&path))
        .await
        .map_err(|err| format!("acquire usage snapshot store operation failed: {err}"))?;
    remove_cached_store_by_key(&path).await;
    Ok(())
}

async fn remove_cached_store(path: &Path) -> Result<(), String> {
    let path = canonical_store_path(path)?;
    let path = path_to_turso(&path)?;
    remove_cached_store_by_key(&path).await;
    Ok(())
}

async fn remove_cached_store_by_key(path: &str) {
    let connections = STORE_CONNECTIONS.get_or_init(|| tokio::sync::Mutex::new(HashMap::new()));
    connections.lock().await.remove(path);
}

pub(crate) fn path_to_turso(path: &Path) -> Result<String, String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| "usage snapshot store path is not utf8".to_owned())
}

fn canonical_store_path(path: &Path) -> Result<std::path::PathBuf, String> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .ok_or_else(|| "usage snapshot store path has no file name".to_owned())?;
    let canonical_parent = std::fs::canonicalize(parent)
        .map_err(|err| format!("canonicalize usage snapshot store dir failed: {err}"))?;
    Ok(canonical_parent.join(file_name))
}

#[cfg(test)]
pub(crate) fn record_connection_build(path: &str) {
    if let Ok(mut builds) = CONNECTION_BUILDS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
    {
        *builds.entry(path.to_owned()).or_default() += 1;
    }
}

#[cfg(not(test))]
pub(crate) fn record_connection_build(_path: &str) {}

#[cfg(test)]
pub(crate) fn connection_build_count(path: &Path) -> Result<usize, String> {
    let path = canonical_store_path(path)?;
    let path = path_to_turso(&path)?;
    Ok(CONNECTION_BUILDS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .map(|builds| builds.get(&path).copied().unwrap_or_default())
        .unwrap_or_default())
}
