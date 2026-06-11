//! Capsule-local structured usage telemetry cache.
//!
//! This is a daemon-owned store under `/jackin/state/`: Capsule writes quota
//! snapshots after provider refresh and renderers read through the daemon cache,
//! not by opening this database. The schema mirrors the roadmap's V1 account
//! snapshot shape so the later host-daemon store can reuse the same rows.

use std::path::Path;
use std::time::Duration;

use jackin_protocol::control::{
    FocusedUsageView, QuotaBucketView, UsageConfidence, UsageSnapshotStatus, UsageSource,
};
#[cfg(test)]
use rusqlite::OptionalExtension;
use rusqlite::{Connection, params};
use sha2::{Digest, Sha256};

const SCHEMA_VERSION: &str = "1";

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
    let conn = open_store(path)?;
    upsert_account_snapshot_rows(&conn, view)
}

fn open_store(path: &Path) -> Result<Connection, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("create telemetry store dir failed: {err}"))?;
    }
    let conn =
        Connection::open(path).map_err(|err| format!("open telemetry store failed: {err}"))?;
    conn.busy_timeout(Duration::from_secs(2))
        .map_err(|err| format!("set telemetry store busy timeout failed: {err}"))?;
    initialize_schema(&conn)?;
    Ok(conn)
}

fn initialize_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS _meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS usage_samples (
            id INTEGER PRIMARY KEY,
            occurred_at INTEGER NOT NULL,
            session_id INTEGER,
            workspace TEXT,
            provider TEXT NOT NULL,
            model TEXT NOT NULL,
            token_input INTEGER,
            token_output INTEGER,
            token_cache_read INTEGER,
            token_cache_write INTEGER,
            cost_usd_micros INTEGER
        );
        CREATE INDEX IF NOT EXISTS usage_samples_by_time
            ON usage_samples (occurred_at);
        CREATE INDEX IF NOT EXISTS usage_samples_by_session
            ON usage_samples (session_id, occurred_at);
        CREATE INDEX IF NOT EXISTS usage_samples_by_workspace
            ON usage_samples (workspace, occurred_at);

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
    .map_err(|err| format!("initialize telemetry store schema failed: {err}"))?;
    conn.execute(
        "INSERT INTO _meta (key, value) VALUES ('schema_version', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [SCHEMA_VERSION],
    )
    .map_err(|err| format!("record telemetry store schema version failed: {err}"))?;
    Ok(())
}

fn upsert_account_snapshot_rows(conn: &Connection, view: &FocusedUsageView) -> Result<(), String> {
    let rows = account_snapshot_rows(view);
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

#[cfg(test)]
pub(crate) fn stored_account_snapshots(
    path: &Path,
) -> Result<Vec<StoredAccountUsageSnapshot>, String> {
    let conn = open_store(path)?;
    let mut stmt = conn
        .prepare(
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
        )
        .map_err(|err| format!("prepare telemetry snapshot query failed: {err}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok(StoredAccountUsageSnapshot {
                provider: row.get(0)?,
                account_key_hash: row.get(1)?,
                account_label: row.get(2)?,
                source: row.get(3)?,
                confidence: row.get(4)?,
                window_kind: row.get(5)?,
                used_amount: row.get(6)?,
                used_unit: row.get(7)?,
                limit_amount: row.get(8)?,
                limit_unit: row.get(9)?,
                resets_at: row.get(10)?,
                fetched_at: row.get(11)?,
                expires_at: row.get(12)?,
                status: row.get(13)?,
                last_error: row.get(14)?,
            })
        })
        .map_err(|err| format!("query telemetry snapshots failed: {err}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("read telemetry snapshot row failed: {err}"))
}

#[cfg(test)]
pub(crate) fn schema_version(path: &Path) -> Result<Option<String>, String> {
    let conn = open_store(path)?;
    conn.query_row(
        "SELECT value FROM _meta WHERE key = 'schema_version'",
        [],
        |row| row.get(0),
    )
    .optional()
    .map_err(|err| format!("query telemetry schema version failed: {err}"))
}

#[cfg(test)]
mod tests {
    use jackin_protocol::control::{
        FocusedAccountHeader, FocusedUsageView, QuotaBucketView, UsageConfidence,
        UsageSnapshotStatus, UsageSource, WorkspaceSpendView,
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
            workspace_spend: WorkspaceSpendView {
                today_cost_label: None,
                thirty_day_cost_label: None,
                thirty_day_tokens_label: None,
                latest_tokens_label: None,
                top_model: None,
                history: Vec::new(),
                provenance_label: "none".to_owned(),
            },
            status: UsageSnapshotStatus::Fresh,
            source: UsageSource::Cli,
            confidence: UsageConfidence::Authoritative,
            fetched_at_epoch: 1_781_185_560,
            updated_label: "Updated just now".to_owned(),
            status_bar_label: "Codex Session: 63% used · 37% left".to_owned(),
            provider_status: None,
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
            Some("1")
        );
    }
}
