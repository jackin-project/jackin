// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Capsule-local structured usage telemetry cache.
//!
//! This is a daemon-owned store under `/jackin/state/`: Capsule writes quota
//! snapshots after provider refresh and renderers read through the daemon cache,
//! not by opening this database. The schema mirrors the roadmap V1 account
//! snapshot shape so the later host-daemon store can reuse the same rows.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use crate::store_backend::{Connection, Row, connect_local, params};
use jackin_core::account_key::account_key_hash;
use jackin_protocol::control::{FocusedUsageView, QuotaBucketView};
#[cfg(test)]
use jackin_protocol::control::{UsageConfidence, UsageSnapshotStatus, UsageSource};

const SCHEMA_VERSION: &str = "4";

#[cfg(test)]
static CONNECTION_BUILDS: OnceLock<Mutex<HashMap<String, usize>>> = OnceLock::new();

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredAccountUsageSnapshot {
    pub provider: String,
    pub account_key_hash: String,
    pub account_label: String,
    pub source: String,
    pub confidence: String,
    pub window_kind: String,
    pub used_amount: Option<i64>,
    pub used_unit: Option<String>,
    pub limit_amount: Option<i64>,
    pub limit_unit: Option<String>,
    pub resets_at: Option<i64>,
    pub fetched_at: i64,
    pub expires_at: Option<i64>,
    pub status: String,
    pub last_error: Option<String>,
    pub focused_provider: Option<String>,
    pub plan_label: Option<String>,
    pub remaining_percent: Option<i64>,
    pub used_label: Option<String>,
    pub limit_label: Option<String>,
    pub reset_label: Option<String>,
    pub pace_label: Option<String>,
    pub view_status: String,
    pub updated_label: String,
    pub status_bar_label: String,
}

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

fn block_on_store<T, Fut>(future: Fut) -> Result<T, String>
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
            .map_err(|err| format!("create telemetry store runtime failed: {err}"))?;
        RUNTIME.get_or_init(move || runtime)
    };
    runtime.block_on(future)
}

async fn open_store(path: &Path) -> Result<Connection, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("create telemetry store dir failed: {err}"))?;
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
        .map_err(|err| format!("open telemetry store failed: {err}"))?;
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

fn path_to_turso(path: &Path) -> Result<String, String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| "telemetry store path is not utf8".to_owned())
}

#[cfg(test)]
fn record_connection_build(path: &str) {
    if let Ok(mut builds) = CONNECTION_BUILDS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
    {
        *builds.entry(path.to_owned()).or_default() += 1;
    }
}

#[cfg(not(test))]
fn record_connection_build(_path: &str) {}

#[cfg(test)]
fn connection_build_count(path: &Path) -> Result<usize, String> {
    let path = path_to_turso(path)?;
    Ok(CONNECTION_BUILDS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .map(|builds| builds.get(&path).copied().unwrap_or_default())
        .unwrap_or_default())
}

async fn initialize_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS _meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS account_usage_snapshots (
            id INTEGER PRIMARY KEY,
            provider TEXT NOT NULL,
            account_key_hash TEXT NOT NULL,
            account_label TEXT NOT NULL,
            source TEXT NOT NULL,
            confidence TEXT NOT NULL,
            window_kind TEXT NOT NULL,
            used_amount INTEGER,
            used_unit TEXT,
            limit_amount INTEGER,
            limit_unit TEXT,
            resets_at INTEGER,
            fetched_at INTEGER NOT NULL,
            expires_at INTEGER,
            status TEXT NOT NULL,
            last_error TEXT,
            focused_provider TEXT,
            plan_label TEXT,
            remaining_percent INTEGER,
            used_label TEXT,
            limit_label TEXT,
            reset_label TEXT,
            pace_label TEXT,
            view_status TEXT NOT NULL DEFAULT 'unavailable',
            updated_label TEXT NOT NULL DEFAULT 'Unavailable',
            status_bar_label TEXT NOT NULL DEFAULT 'usage unavailable',
            UNIQUE(provider, account_key_hash, source, window_kind)
        );

        ",
    )
    .await
    .map_err(|err| format!("initialize telemetry store schema failed: {err}"))?;
    ensure_account_snapshot_columns(conn).await?;
    conn.execute(
        "INSERT INTO _meta (key, value) VALUES ('schema_version', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [SCHEMA_VERSION],
    )
    .await
    .map_err(|err| format!("record telemetry store schema version failed: {err}"))?;
    Ok(())
}

async fn ensure_account_snapshot_columns(conn: &Connection) -> Result<(), String> {
    let mut rows = conn
        .query("PRAGMA table_info(account_usage_snapshots)", ())
        .await
        .map_err(|err| format!("inspect telemetry snapshot schema failed: {err}"))?;
    let mut columns = HashSet::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|err| format!("read telemetry snapshot schema failed: {err}"))?
    {
        columns.insert(row_string(&row, 1, "column_name")?);
    }
    for (name, ddl) in [
        (
            "focused_provider",
            "ALTER TABLE account_usage_snapshots ADD COLUMN focused_provider TEXT",
        ),
        (
            "plan_label",
            "ALTER TABLE account_usage_snapshots ADD COLUMN plan_label TEXT",
        ),
        (
            "remaining_percent",
            "ALTER TABLE account_usage_snapshots ADD COLUMN remaining_percent INTEGER",
        ),
        (
            "used_label",
            "ALTER TABLE account_usage_snapshots ADD COLUMN used_label TEXT",
        ),
        (
            "limit_label",
            "ALTER TABLE account_usage_snapshots ADD COLUMN limit_label TEXT",
        ),
        (
            "reset_label",
            "ALTER TABLE account_usage_snapshots ADD COLUMN reset_label TEXT",
        ),
        (
            "pace_label",
            "ALTER TABLE account_usage_snapshots ADD COLUMN pace_label TEXT",
        ),
        (
            "view_status",
            "ALTER TABLE account_usage_snapshots ADD COLUMN view_status TEXT NOT NULL DEFAULT 'unavailable'",
        ),
        (
            "updated_label",
            "ALTER TABLE account_usage_snapshots ADD COLUMN updated_label TEXT NOT NULL DEFAULT 'Unavailable'",
        ),
        (
            "status_bar_label",
            "ALTER TABLE account_usage_snapshots ADD COLUMN status_bar_label TEXT NOT NULL DEFAULT 'usage unavailable'",
        ),
    ] {
        if !columns.contains(name) {
            conn.execute(ddl, ())
                .await
                .map_err(|err| format!("upgrade telemetry snapshot schema failed: {err}"))?;
        }
    }
    Ok(())
}

async fn upsert_account_snapshot_rows(
    conn: &Connection,
    rows: Vec<StoredAccountUsageSnapshot>,
) -> Result<(), String> {
    jackin_diagnostics::incr_db_statement("begin");
    conn.execute("BEGIN", ())
        .await
        .map_err(|err| format!("begin telemetry snapshot transaction failed: {err}"))?;
    for row in rows {
        jackin_diagnostics::incr_db_statement("upsert_account_usage_snapshot");
        if let Err(err) = conn
            .execute(
            "
            INSERT INTO account_usage_snapshots (
                provider,
                account_key_hash,
                account_label,
                source,
                confidence,
                window_kind,
                used_amount,
                used_unit,
                limit_amount,
                limit_unit,
                resets_at,
                fetched_at,
                expires_at,
                status,
                last_error,
                focused_provider,
                plan_label,
                remaining_percent,
                used_label,
                limit_label,
                reset_label,
                pace_label,
                view_status,
                updated_label,
                status_bar_label
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25)
            ON CONFLICT(provider, account_key_hash, source, window_kind) DO UPDATE SET
                account_label = excluded.account_label,
                confidence = excluded.confidence,
                used_amount = excluded.used_amount,
                used_unit = excluded.used_unit,
                limit_amount = excluded.limit_amount,
                limit_unit = excluded.limit_unit,
                resets_at = excluded.resets_at,
                fetched_at = excluded.fetched_at,
                expires_at = excluded.expires_at,
                status = excluded.status,
                last_error = excluded.last_error,
                focused_provider = excluded.focused_provider,
                plan_label = excluded.plan_label,
                remaining_percent = excluded.remaining_percent,
                used_label = excluded.used_label,
                limit_label = excluded.limit_label,
                reset_label = excluded.reset_label,
                pace_label = excluded.pace_label,
                view_status = excluded.view_status,
                updated_label = excluded.updated_label,
                status_bar_label = excluded.status_bar_label
            ",
            params![
                row.provider,
                row.account_key_hash,
                row.account_label,
                row.source,
                row.confidence,
                row.window_kind,
                row.used_amount,
                row.used_unit,
                row.limit_amount,
                row.limit_unit,
                row.resets_at,
                row.fetched_at,
                row.expires_at,
                row.status,
                row.last_error,
                row.focused_provider,
                row.plan_label,
                row.remaining_percent,
                row.used_label,
                row.limit_label,
                row.reset_label,
                row.pace_label,
                row.view_status,
                row.updated_label,
                row.status_bar_label,
            ],
        )
        .await
        {
            // Roll the whole batch back so a mid-batch failure never leaves a
            // partially-written snapshot set; surface the original row error.
            if let Err(rollback_err) = conn.execute("ROLLBACK", ()).await {
                crate::cdebug!("telemetry snapshot rollback failed: {rollback_err}");
            }
            return Err(format!("upsert telemetry account snapshot failed: {err}"));
        }
    }
    conn.execute("COMMIT", ())
        .await
        .map_err(|err| format!("commit telemetry snapshot transaction failed: {err}"))?;
    Ok(())
}

fn account_snapshot_rows(view: &FocusedUsageView) -> Vec<StoredAccountUsageSnapshot> {
    let provider = view.account.provider_label.clone();
    let account_label = view.account.account_label.clone();
    let account_key_hash = account_key_hash(&provider, &account_label);
    let source = crate::usage::usage_source_storage_label(view.source).to_owned();
    let confidence = crate::usage::usage_confidence_storage_label(view.confidence).to_owned();
    let fetched_at = view.fetched_at_epoch;
    let last_error = view.last_error.clone();
    view.buckets
        .iter()
        .map(|bucket| {
            let quota = quota_amounts(bucket);
            StoredAccountUsageSnapshot {
                provider: provider.clone(),
                account_key_hash: account_key_hash.clone(),
                account_label: account_label.clone(),
                source: source.clone(),
                confidence: confidence.clone(),
                window_kind: bucket.label.clone(),
                used_amount: quota.used_amount,
                used_unit: quota.used_unit,
                limit_amount: quota.limit_amount,
                limit_unit: quota.limit_unit,
                resets_at: bucket.resets_at,
                fetched_at,
                expires_at: None,
                status: crate::usage::usage_status_storage_label(bucket.status).to_owned(),
                last_error: last_error.clone(),
                focused_provider: view.focused_provider.clone(),
                plan_label: view.account.plan_label.clone(),
                remaining_percent: bucket.remaining_percent.map(i64::from),
                used_label: bucket.used_label.clone(),
                limit_label: bucket.limit_label.clone(),
                reset_label: bucket.reset_label.clone(),
                pace_label: bucket.pace_label.clone(),
                view_status: crate::usage::usage_status_storage_label(view.status).to_owned(),
                updated_label: view.updated_label.clone(),
                status_bar_label: view.status_bar_label.clone(),
            }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct QuotaAmounts {
    used_amount: Option<i64>,
    used_unit: Option<String>,
    limit_amount: Option<i64>,
    limit_unit: Option<String>,
}

fn quota_amounts(bucket: &QuotaBucketView) -> QuotaAmounts {
    if let Some(remaining) = bucket.remaining_percent {
        return QuotaAmounts {
            used_amount: Some(i64::from(100_u8.saturating_sub(remaining.min(100)))),
            used_unit: Some("percent".to_owned()),
            limit_amount: Some(100),
            limit_unit: Some("percent".to_owned()),
        };
    }
    QuotaAmounts {
        used_amount: None,
        used_unit: None,
        limit_amount: None,
        limit_unit: None,
    }
}

#[cfg(test)]
pub fn focused_usage_view(
    path: &Path,
    focused_agent: Option<&str>,
    focused_provider: Option<&str>,
    now_epoch: i64,
) -> Result<Option<FocusedUsageView>, String> {
    let rows = stored_account_snapshots(path)?;
    let tabs = usage_provider_tabs_from_rows(&rows);
    let resolved_provider = focused_provider.or_else(|| {
        focused_agent.and_then(|agent| crate::usage::resolved_usage_provider_label(agent, None))
    });
    let Some((provider, rows)) = select_provider_rows(rows, resolved_provider) else {
        return Ok(None);
    };
    let Some(first) = rows.first() else {
        return Ok(None);
    };
    let status = usage_status_from_label(&first.view_status);
    let source = usage_source_from_label(&first.source);
    let confidence = usage_confidence_from_label(&first.confidence);
    let fetched_at = rows.iter().map(|row| row.fetched_at).max().unwrap_or(0);
    let mut buckets = rows
        .iter()
        .map(|row| QuotaBucketView {
            used_money: None,
            limit_money: None,
            severity: jackin_protocol::control::UsageSeverity::default(),
            label: row.window_kind.clone(),
            used_label: row.used_label.clone(),
            limit_label: row.limit_label.clone(),
            remaining_percent: row
                .remaining_percent
                .and_then(|value| u8::try_from(value.clamp(0, 100)).ok()),
            reset_label: row.reset_label.clone(),
            resets_at: row.resets_at,
            // The headline is persisted as `status_bar_label`, so the slot tag is
            // not stored; a store-restored bucket carries none and the live
            // refresh re-tags it.
            status_slot: None,
            pace_label: row.pace_label.clone(),
            status: usage_status_from_label(&row.status),
        })
        .collect::<Vec<_>>();
    buckets.sort_by_key(|bucket| usage_bucket_order(&provider, &bucket.label));
    Ok(Some(FocusedUsageView {
        focused_agent: focused_agent.map(str::to_owned),
        focused_provider: first
            .focused_provider
            .clone()
            .or_else(|| resolved_provider.map(str::to_owned))
            .or_else(|| Some(provider.clone())),
        account: jackin_protocol::control::FocusedAccountHeader {
            provider_label: provider,
            account_label: first.account_label.clone(),
            // username + credential_origin are live-snapshot fields, not
            // persisted in the store yet; a store-restored header gets them
            // on the next refresh.
            username: None,
            plan_label: first.plan_label.clone(),
            credential_origin: None,
        },
        buckets,
        status,
        source,
        confidence,
        fetched_at_epoch: fetched_at,
        updated_label: if matches!(
            status,
            UsageSnapshotStatus::Fresh | UsageSnapshotStatus::Stale
        ) || first.updated_label.trim().is_empty()
        {
            crate::usage::relative_updated_label(fetched_at, now_epoch)
        } else {
            first.updated_label.clone()
        },
        status_bar_label: if first.status_bar_label.trim().is_empty() {
            lifecycle_status_bar_label(status)
        } else {
            first.status_bar_label.clone()
        },
        tabs,
        last_error: first.last_error.clone(),
    }))
}

#[cfg(test)]
fn usage_bucket_order(provider: &str, label: &str) -> usize {
    let provider = normalize_provider_label(provider);
    let order: &[&str] = if provider_matches("openai", &provider)
        || provider_matches("codex", &provider)
    {
        &[
            "Session",
            "Weekly",
            "Codex Spark 5-hour",
            "Codex Spark Weekly",
            "Limit Reset Credits",
            "Credits",
        ]
    } else if provider_matches("anthropic", &provider) || provider_matches("claude", &provider) {
        &[
            "Session",
            "Weekly",
            "All models",
            "Sonnet",
            "Daily Routines",
        ]
    } else if provider_matches("amp", &provider) {
        &["Amp Free", "Credits", "Individual credits"]
    } else if provider_matches("zai", &provider) || provider_matches("glm", &provider) {
        // F9: short/active window first (operator override of CodexBar's
        // Tokens, MCP, 5-hour order).
        &["5-hour", "Tokens", "MCP"]
    } else if provider_matches("kimi", &provider) {
        // F10: rate (short/active) window on top, then Weekly (operator
        // override of CodexBar's Weekly, Rate Limit order).
        &["Rate Limit", "Weekly"]
    } else if provider_matches("minimax", &provider) {
        &["General · 5h", "General · Weekly", "Video"]
    } else {
        &[]
    };
    order
        .iter()
        .position(|entry| provider_matches(entry, label))
        .unwrap_or(order.len())
}

#[cfg(test)]
fn select_provider_rows(
    rows: Vec<StoredAccountUsageSnapshot>,
    focused_provider: Option<&str>,
) -> Option<(String, Vec<StoredAccountUsageSnapshot>)> {
    let focused = focused_provider.unwrap_or_default();
    let mut matches = rows
        .into_iter()
        .filter(|row| provider_matches(focused, &row.provider))
        .collect::<Vec<_>>();
    if matches.is_empty() {
        return None;
    }
    let latest = matches.iter().map(|row| row.fetched_at).max()?;
    matches.retain(|row| row.fetched_at == latest);
    let provider = matches.first()?.provider.clone();
    Some((provider, matches))
}

#[cfg(test)]
fn provider_matches(needle: &str, provider: &str) -> bool {
    if needle.trim().is_empty() {
        return false;
    }
    let needle = normalize_provider_label(needle);
    let provider = normalize_provider_label(provider);
    provider.contains(&needle)
        || needle.contains(&provider)
        || (needle.contains("openai") && provider.contains("codex"))
        || (needle.contains("codex") && provider.contains("openai"))
        || (needle.contains("anthropic") && provider.contains("claude"))
        || (needle.contains("claude") && provider.contains("anthropic"))
        || (needle.contains("xai") && provider.contains("grok"))
        || (needle.contains("grok") && provider.contains("xai"))
        || (needle.contains("zai") && provider.contains("glm"))
        || (needle.contains("glm") && provider.contains("zai"))
}

#[cfg(test)]
fn normalize_provider_label(value: &str) -> String {
    value
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase()
}

#[cfg(test)]
fn usage_provider_tabs_from_rows(
    rows: &[StoredAccountUsageSnapshot],
) -> Vec<jackin_protocol::control::UsageProviderTab> {
    [
        "Codex",
        "Claude",
        "Amp",
        "Grok Build",
        "GLM / Z.AI",
        "Kimi",
        "MiniMax",
    ]
    .into_iter()
    .map(|label| {
        let latest = rows
            .iter()
            .filter(|row| provider_matches(label, &row.provider))
            .max_by_key(|row| row.fetched_at);
        jackin_protocol::control::UsageProviderTab {
            label: label.to_owned(),
            status_label: latest.map_or_else(
                || "not cached".to_owned(),
                |row| tab_status_label(row, rows),
            ),
            account_label: latest.map_or_else(
                || "account unavailable".to_owned(),
                |row| row.account_label.clone(),
            ),
            plan_label: latest.and_then(|row| row.plan_label.clone()),
            source_label: latest.map(|row| format!("{} · {}", row.view_status, row.source)),
            active: false,
        }
    })
    .collect()
}

#[cfg(test)]
fn tab_status_label(
    row: &StoredAccountUsageSnapshot,
    rows: &[StoredAccountUsageSnapshot],
) -> String {
    rows.iter()
        .filter(|candidate| {
            candidate.provider == row.provider
                && candidate.account_key_hash == row.account_key_hash
                && candidate.fetched_at == row.fetched_at
        })
        .find_map(|candidate| {
            candidate
                .remaining_percent
                .map(|remaining| format!("{} {remaining}% left", candidate.window_kind))
        })
        .unwrap_or_else(|| row.view_status.clone())
}

#[cfg(test)]
fn usage_status_from_label(label: &str) -> UsageSnapshotStatus {
    match label {
        "fresh" => UsageSnapshotStatus::Fresh,
        "stale" => UsageSnapshotStatus::Stale,
        "needs_login" => UsageSnapshotStatus::NeedsLogin,
        "needs_secret" => UsageSnapshotStatus::NeedsSecret,
        "unsupported" => UsageSnapshotStatus::Unsupported,
        "error" => UsageSnapshotStatus::Error,
        _ => UsageSnapshotStatus::Unavailable,
    }
}

#[cfg(test)]
fn usage_source_from_label(label: &str) -> UsageSource {
    match label {
        "provider_api" => UsageSource::ProviderApi,
        "cli" => UsageSource::Cli,
        "local_logs" => UsageSource::LocalLogs,
        "cache" => UsageSource::Cache,
        _ => UsageSource::None,
    }
}

#[cfg(test)]
fn usage_confidence_from_label(label: &str) -> UsageConfidence {
    match label {
        "authoritative" => UsageConfidence::Authoritative,
        "estimated" => UsageConfidence::Estimated,
        "presence_only" => UsageConfidence::PresenceOnly,
        _ => UsageConfidence::None,
    }
}

#[cfg(test)]
fn lifecycle_status_bar_label(status: UsageSnapshotStatus) -> String {
    match status {
        UsageSnapshotStatus::Fresh => "usage cached",
        UsageSnapshotStatus::Stale => "stale",
        UsageSnapshotStatus::NeedsLogin => "needs login",
        UsageSnapshotStatus::NeedsSecret => "needs secret",
        UsageSnapshotStatus::Unsupported => "unsupported",
        UsageSnapshotStatus::Unavailable => "usage unavailable",
        UsageSnapshotStatus::Error => "error",
    }
    .to_owned()
}

#[cfg(test)]
fn stored_account_snapshots(path: &Path) -> Result<Vec<StoredAccountUsageSnapshot>, String> {
    let path = path.to_path_buf();
    block_on_store(async move {
        let conn = open_store(&path).await?;
        let mut rows = conn
            .query(
                "
                SELECT
                    provider,
                    account_key_hash,
                    account_label,
                    source,
                    confidence,
                    window_kind,
                    used_amount,
                    used_unit,
                    limit_amount,
                    limit_unit,
                    resets_at,
                    fetched_at,
                    expires_at,
                    status,
                    last_error,
                    focused_provider,
                    plan_label,
                    remaining_percent,
                    used_label,
                    limit_label,
                    reset_label,
                    pace_label,
                    view_status,
                    updated_label,
                    status_bar_label
                FROM account_usage_snapshots
                ORDER BY provider, account_key_hash, source, window_kind
                ",
                (),
            )
            .await
            .map_err(|err| format!("query telemetry snapshots failed: {err}"))?;
        let mut snapshots = Vec::new();
        while let Some(row) = rows
            .next()
            .await
            .map_err(|err| format!("read telemetry snapshot row failed: {err}"))?
        {
            snapshots.push(StoredAccountUsageSnapshot {
                provider: row_string(&row, 0, "provider")?,
                account_key_hash: row_string(&row, 1, "account_key_hash")?,
                account_label: row_string(&row, 2, "account_label")?,
                source: row_string(&row, 3, "source")?,
                confidence: row_string(&row, 4, "confidence")?,
                window_kind: row_string(&row, 5, "window_kind")?,
                used_amount: row_opt_i64(&row, 6, "used_amount")?,
                used_unit: row_opt_string(&row, 7, "used_unit")?,
                limit_amount: row_opt_i64(&row, 8, "limit_amount")?,
                limit_unit: row_opt_string(&row, 9, "limit_unit")?,
                resets_at: row_opt_i64(&row, 10, "resets_at")?,
                fetched_at: row_i64(&row, 11, "fetched_at")?,
                expires_at: row_opt_i64(&row, 12, "expires_at")?,
                status: row_string(&row, 13, "status")?,
                last_error: row_opt_string(&row, 14, "last_error")?,
                focused_provider: row_opt_string(&row, 15, "focused_provider")?,
                plan_label: row_opt_string(&row, 16, "plan_label")?,
                remaining_percent: row_opt_i64(&row, 17, "remaining_percent")?,
                used_label: row_opt_string(&row, 18, "used_label")?,
                limit_label: row_opt_string(&row, 19, "limit_label")?,
                reset_label: row_opt_string(&row, 20, "reset_label")?,
                pace_label: row_opt_string(&row, 21, "pace_label")?,
                view_status: row_string(&row, 22, "view_status")?,
                updated_label: row_string(&row, 23, "updated_label")?,
                status_bar_label: row_string(&row, 24, "status_bar_label")?,
            });
        }
        Ok(snapshots)
    })
}

#[cfg(test)]
pub fn schema_version(path: &Path) -> Result<Option<String>, String> {
    let path = path.to_path_buf();
    block_on_store(async move {
        let conn = open_store(&path).await?;
        let mut rows = conn
            .query("SELECT value FROM _meta WHERE key = 'schema_version'", ())
            .await
            .map_err(|err| format!("query telemetry schema version failed: {err}"))?;
        rows.next()
            .await
            .map_err(|err| format!("read telemetry schema version failed: {err}"))?
            .map(|row| row_string(&row, 0, "schema_version"))
            .transpose()
    })
}

#[cfg(test)]
fn row_i64(row: &Row, idx: usize, name: &str) -> Result<i64, String> {
    row.get(idx)
        .map_err(|err| format!("decode telemetry {name} failed: {err}"))
}

#[cfg(test)]
fn row_opt_i64(row: &Row, idx: usize, name: &str) -> Result<Option<i64>, String> {
    row.get(idx)
        .map_err(|err| format!("decode telemetry {name} failed: {err}"))
}

fn row_string(row: &Row, idx: usize, name: &str) -> Result<String, String> {
    row.get(idx)
        .map_err(|err| format!("decode telemetry {name} failed: {err}"))
}

#[cfg(test)]
fn row_opt_string(row: &Row, idx: usize, name: &str) -> Result<Option<String>, String> {
    row.get(idx)
        .map_err(|err| format!("decode telemetry {name} failed: {err}"))
}

#[cfg(test)]
mod tests;
