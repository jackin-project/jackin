// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Snapshot writes and connection handling.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use crate::store_backend::{Connection, connect_local};

use jackin_protocol::control::FocusedUsageView;

#[cfg(test)]
use super::CONNECTION_BUILDS;
use super::{account_snapshot_rows, initialize_schema, upsert_account_snapshot_rows};

#[cfg(test)]
pub fn store_usage_snapshot(path: &Path, view: &FocusedUsageView) -> Result<(), String> {
    store_usage_snapshots(path, std::slice::from_ref(view))
}

pub fn store_usage_snapshots(path: &Path, views: &[FocusedUsageView]) -> Result<(), String> {
    let path = path.to_path_buf();
    let rows = views
        .iter()
        .flat_map(account_snapshot_rows)
        .collect::<Vec<_>>();
    block_on_store(async move {
        let conn = open_store(&path).await?;
        upsert_account_snapshot_rows(&conn, rows).await
    })
}

pub(crate) fn block_on_store<T, Fut>(future: Fut) -> Result<T, String>
where
    Fut: Future<Output = Result<T, String>>,
{
    // One process-wide current-thread runtime, reused across every store call.
    // Callers run inside `spawn_blocking` (no enclosing runtime), so `block_on`
    // never nests; sequential reuse avoids rebuilding a runtime per snapshot
    // write. Build errors propagate without panicking. INVARIANT: never call the
    // store functions from inside the async runtime — route them through
    // `spawn_blocking`, or `block_on` panics ("Cannot start a runtime from within
    // a runtime").
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

pub(crate) async fn open_store(path: &Path) -> Result<Connection, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("create usage snapshot store dir failed: {err}"))?;
    }
    let path = path_to_turso(path)?;
    static STORE_CONNECTIONS: OnceLock<tokio::sync::Mutex<HashMap<String, Connection>>> =
        OnceLock::new();
    let connections = STORE_CONNECTIONS.get_or_init(|| tokio::sync::Mutex::new(HashMap::new()));
    let mut connections = connections.lock().await;
    if let Some(conn) = connections.get(&path) {
        return Ok(conn.clone());
    }
    let conn = connect_local(&path)
        .await
        .map_err(|err| format!("open usage snapshot store failed: {err}"))?;
    record_connection_build(&path);
    // Schema creation + the ALTER-based migration are idempotent but not free;
    // run them once per database path per process. Keyed by the resolved turso
    // path so distinct stores (e.g. each test's temp DB) each migrate once.
    static INITIALIZED_DBS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    let initialized = INITIALIZED_DBS.get_or_init(|| Mutex::new(HashSet::new()));
    let already_initialized = initialized.lock().is_ok_and(|set| set.contains(&path));
    if !already_initialized {
        initialize_schema(&conn).await?;
        if let Ok(mut set) = initialized.lock() {
            set.insert(path.clone());
        }
    }
    connections.insert(path, conn.clone());
    Ok(conn)
}

pub(crate) fn path_to_turso(path: &Path) -> Result<String, String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| "usage snapshot store path is not utf8".to_owned())
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
    let path = path_to_turso(path)?;
    Ok(CONNECTION_BUILDS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .map(|builds| builds.get(&path).copied().unwrap_or_default())
        .unwrap_or_default())
}
