//! Capsule-local structured usage telemetry cache.
//!
//! This is a daemon-owned store under `/jackin/state/`: Capsule writes quota
//! snapshots after provider refresh and renderers read through the daemon cache,
//! not by opening this database. The schema mirrors the roadmap V1 account
//! snapshot shape so the later host-daemon store can reuse the same rows.

use std::future::Future;
use std::path::Path;
use std::thread;

use jackin_protocol::control::{
    AccountUsageSnapshotView, FocusedUsageView, QuotaBucketView, UsageConfidence,
    UsageSnapshotStatus, UsageSource,
};
use sha2::{Digest, Sha256};
use turso::{Connection, Row, params};

const SCHEMA_VERSION: &str = "3";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoredAccountUsageSnapshot {
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
}

pub(crate) fn store_usage_snapshot(path: &Path, view: &FocusedUsageView) -> Result<(), String> {
    let path = path.to_path_buf();
    let rows = account_snapshot_rows(view);
    run_store(move || async move {
        let conn = open_store(&path).await?;
        upsert_account_snapshot_rows(&conn, rows).await
    })
}

fn run_store<T, F, Fut>(f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = Result<T, String>> + 'static,
{
    thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .map_err(|err| format!("create telemetry store runtime failed: {err}"))?;
        runtime.block_on(f())
    })
    .join()
    .map_err(|_| "telemetry store thread panicked".to_owned())?
}

async fn open_store(path: &Path) -> Result<Connection, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("create telemetry store dir failed: {err}"))?;
    }
    let path = path_to_turso(path)?;
    let db = turso::Builder::new_local(&path)
        .build()
        .await
        .map_err(|err| format!("open telemetry store failed: {err}"))?;
    let conn = db
        .connect()
        .map_err(|err| format!("connect telemetry store failed: {err}"))?;
    initialize_schema(&conn).await?;
    Ok(conn)
}

fn path_to_turso(path: &Path) -> Result<String, String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| "telemetry store path is not utf8".to_owned())
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
            UNIQUE(provider, account_key_hash, source, window_kind)
        );

        ",
    )
    .await
    .map_err(|err| format!("initialize telemetry store schema failed: {err}"))?;
    conn.execute(
        "INSERT INTO _meta (key, value) VALUES ('schema_version', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [SCHEMA_VERSION],
    )
    .await
    .map_err(|err| format!("record telemetry store schema version failed: {err}"))?;
    Ok(())
}

async fn upsert_account_snapshot_rows(
    conn: &Connection,
    rows: Vec<StoredAccountUsageSnapshot>,
) -> Result<(), String> {
    for row in rows {
        conn.execute(
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
                last_error
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
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
                last_error = excluded.last_error
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
            ],
        )
        .await
        .map_err(|err| format!("upsert telemetry account snapshot failed: {err}"))?;
    }
    Ok(())
}

fn account_snapshot_rows(view: &FocusedUsageView) -> Vec<StoredAccountUsageSnapshot> {
    let provider = view.account.provider_label.clone();
    let account_label = view.account.account_label.clone();
    let account_key_hash = account_key_hash(&provider, &account_label);
    let source = source_label(view.source).to_owned();
    let confidence = confidence_label(view.confidence).to_owned();
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
                resets_at: None,
                fetched_at,
                expires_at: None,
                status: status_label(bucket.status).to_owned(),
                last_error: last_error.clone(),
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

fn account_key_hash(provider: &str, account_label: &str) -> String {
    let digest = Sha256::digest(format!("{provider}\0{account_label}").as_bytes());
    format!("sha256:{}", hex_lower(&digest))
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from(HEX[usize::from(byte >> 4)]));
        out.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    out
}

fn source_label(source: UsageSource) -> &'static str {
    match source {
        UsageSource::ProviderApi => "provider_api",
        UsageSource::Cli => "cli",
        UsageSource::LocalLogs => "local_logs",
        UsageSource::Cache => "cache",
        UsageSource::None => "none",
    }
}

fn confidence_label(confidence: UsageConfidence) -> &'static str {
    match confidence {
        UsageConfidence::Authoritative => "authoritative",
        UsageConfidence::Estimated => "estimated",
        UsageConfidence::PresenceOnly => "presence_only",
        UsageConfidence::None => "none",
    }
}

fn status_label(status: UsageSnapshotStatus) -> &'static str {
    match status {
        UsageSnapshotStatus::Fresh => "fresh",
        UsageSnapshotStatus::Stale => "stale",
        UsageSnapshotStatus::NeedsLogin => "needs_login",
        UsageSnapshotStatus::NeedsSecret => "needs_secret",
        UsageSnapshotStatus::Unsupported => "unsupported",
        UsageSnapshotStatus::Unavailable => "unavailable",
        UsageSnapshotStatus::Error => "error",
    }
}

pub(crate) fn account_snapshot_views(path: &Path) -> Result<Vec<AccountUsageSnapshotView>, String> {
    stored_account_snapshots(path).map(|rows| {
        rows.into_iter()
            .map(|row| AccountUsageSnapshotView {
                provider: row.provider,
                account_label: row.account_label,
                source: row.source,
                confidence: row.confidence,
                window_kind: row.window_kind,
                used_amount: row.used_amount,
                used_unit: row.used_unit,
                limit_amount: row.limit_amount,
                limit_unit: row.limit_unit,
                resets_at: row.resets_at,
                fetched_at: row.fetched_at,
                expires_at: row.expires_at,
                status: row.status,
                last_error: row.last_error,
            })
            .collect()
    })
}

fn stored_account_snapshots(path: &Path) -> Result<Vec<StoredAccountUsageSnapshot>, String> {
    let path = path.to_path_buf();
    run_store(move || async move {
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
                    last_error
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
            });
        }
        Ok(snapshots)
    })
}

#[cfg(test)]
pub(crate) fn schema_version(path: &Path) -> Result<Option<String>, String> {
    let path = path.to_path_buf();
    run_store(move || async move {
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

fn row_i64(row: &Row, idx: usize, name: &str) -> Result<i64, String> {
    row.get(idx)
        .map_err(|err| format!("decode telemetry {name} failed: {err}"))
}

fn row_opt_i64(row: &Row, idx: usize, name: &str) -> Result<Option<i64>, String> {
    row.get(idx)
        .map_err(|err| format!("decode telemetry {name} failed: {err}"))
}

fn row_string(row: &Row, idx: usize, name: &str) -> Result<String, String> {
    row.get(idx)
        .map_err(|err| format!("decode telemetry {name} failed: {err}"))
}

fn row_opt_string(row: &Row, idx: usize, name: &str) -> Result<Option<String>, String> {
    row.get(idx)
        .map_err(|err| format!("decode telemetry {name} failed: {err}"))
}

#[cfg(test)]
mod tests {
    use jackin_protocol::control::{
        FocusedAccountHeader, FocusedUsageView, QuotaBucketView, UsageConfidence,
        UsageSnapshotStatus, UsageSource,
    };

    use super::*;

    fn usage_view() -> FocusedUsageView {
        FocusedUsageView {
            focused_agent: Some("codex".to_owned()),
            focused_provider: Some("OpenAI".to_owned()),
            account: FocusedAccountHeader {
                provider_label: "Codex".to_owned(),
                account_label: "alexey@example.com".to_owned(),
                plan_label: Some("Pro 20x".to_owned()),
            },
            buckets: vec![
                QuotaBucketView {
                    label: "Session".to_owned(),
                    used_label: Some("63% used".to_owned()),
                    limit_label: Some("100%".to_owned()),
                    remaining_percent: Some(37),
                    reset_label: Some("Resets in 1h".to_owned()),
                    pace_label: None,
                    status: UsageSnapshotStatus::Fresh,
                },
                QuotaBucketView {
                    label: "Credits".to_owned(),
                    used_label: None,
                    limit_label: None,
                    remaining_percent: None,
                    reset_label: None,
                    pace_label: Some("ACP billing unavailable".to_owned()),
                    status: UsageSnapshotStatus::Unsupported,
                },
            ],
            status: UsageSnapshotStatus::Fresh,
            source: UsageSource::Cli,
            confidence: UsageConfidence::Authoritative,
            fetched_at_epoch: 1_781_185_560,
            updated_label: "Updated just now".to_owned(),
            status_bar_label: "Codex Session: 63% used · 37% left".to_owned(),
            tabs: Vec::new(),
            last_error: None,
        }
    }

    #[test]
    fn account_snapshot_rows_are_persisted_and_upserted() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("usage.db");

        store_usage_snapshot(&db, &usage_view()).expect("store first snapshot");
        let mut changed = usage_view();
        changed.buckets[0].remaining_percent = Some(25);
        changed.fetched_at_epoch += 60;
        store_usage_snapshot(&db, &changed).expect("store updated snapshot");

        let rows = stored_account_snapshots(&db).expect("read snapshots");
        assert_eq!(rows.len(), 2);
        let session = rows
            .iter()
            .find(|row| row.window_kind == "Session")
            .expect("session row");
        assert_eq!(session.provider, "Codex");
        assert!(session.account_key_hash.starts_with("sha256:"));
        assert_eq!(session.source, "cli");
        assert_eq!(session.confidence, "authoritative");
        assert_eq!(session.used_amount, Some(75));
        assert_eq!(session.used_unit.as_deref(), Some("percent"));
        assert_eq!(session.limit_amount, Some(100));
        assert_eq!(session.status, "fresh");
        assert_eq!(session.fetched_at, 1_781_185_620);
    }

    #[test]
    fn telemetry_store_records_schema_version() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("usage.db");

        store_usage_snapshot(&db, &usage_view()).expect("store snapshot");

        assert_eq!(
            schema_version(&db).expect("schema version").as_deref(),
            Some("3")
        );
    }
}
