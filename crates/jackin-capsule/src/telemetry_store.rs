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
    UsageSnapshotStatus, UsageSource, UsageSummaryView,
};
use sha2::{Digest, Sha256};
use turso::{Connection, Row, params};

const SCHEMA_VERSION: &str = "2";

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoredUsageSample {
    pub occurred_at: i64,
    pub instance_id: Option<String>,
    pub session_id: Option<i64>,
    pub workspace: Option<String>,
    pub provider: String,
    pub model: String,
    pub token_input: Option<i64>,
    pub token_output: Option<i64>,
    pub token_cache_read: Option<i64>,
    pub token_cache_write: Option<i64>,
    pub cost_usd_micros: Option<i64>,
    pub cost_source: Option<String>,
    pub source_hash: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UsageScanFileState {
    pub bytes_read: u64,
    pub lines_read: u64,
    pub size_bytes: u64,
    pub mtime_epoch: i64,
}

pub(crate) fn store_usage_snapshot(path: &Path, view: &FocusedUsageView) -> Result<(), String> {
    let path = path.to_path_buf();
    let rows = account_snapshot_rows(view);
    run_store(move || async move {
        let conn = open_store(&path).await?;
        upsert_account_snapshot_rows(&conn, rows).await
    })
}

pub(crate) fn store_usage_samples(
    path: &Path,
    samples: &[StoredUsageSample],
) -> Result<(), String> {
    if samples.is_empty() {
        return Ok(());
    }
    let path = path.to_path_buf();
    let samples = samples.to_vec();
    run_store(move || async move {
        let conn = open_store(&path).await?;
        insert_usage_sample_rows(&conn, &samples).await
    })
}

pub(crate) fn usage_scan_file_state(
    path: &Path,
    provider: &str,
    source_path: &Path,
) -> Result<Option<UsageScanFileState>, String> {
    let path = path.to_path_buf();
    let provider = provider.to_owned();
    let source_path = source_path.to_string_lossy().into_owned();
    run_store(move || async move {
        let conn = open_store(&path).await?;
        let mut rows = conn
            .query(
                "
                SELECT bytes_read, lines_read, size_bytes, mtime_epoch
                FROM usage_scan_files
                WHERE provider = ?1 AND source_path = ?2
                ",
                params![provider, source_path],
            )
            .await
            .map_err(|err| format!("read usage scan file state failed: {err}"))?;
        match rows
            .next()
            .await
            .map_err(|err| format!("read usage scan file state row failed: {err}"))?
        {
            Some(row) => Ok(Some(UsageScanFileState {
                bytes_read: row_i64(&row, 0, "bytes_read")?.try_into().unwrap_or(0),
                lines_read: row_i64(&row, 1, "lines_read")?.try_into().unwrap_or(0),
                size_bytes: row_i64(&row, 2, "size_bytes")?.try_into().unwrap_or(0),
                mtime_epoch: row_i64(&row, 3, "mtime_epoch")?,
            })),
            None => Ok(None),
        }
    })
}

pub(crate) fn store_usage_scan_file_state(
    path: &Path,
    provider: &str,
    source_path: &Path,
    state: UsageScanFileState,
) -> Result<(), String> {
    let path = path.to_path_buf();
    let provider = provider.to_owned();
    let source_path = source_path.to_string_lossy().into_owned();
    run_store(move || async move {
        let conn = open_store(&path).await?;
        conn.execute(
            "
            INSERT INTO usage_scan_files (
                provider,
                source_path,
                bytes_read,
                lines_read,
                size_bytes,
                mtime_epoch,
                scanned_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, strftime('%s', 'now'))
            ON CONFLICT(provider, source_path) DO UPDATE SET
                bytes_read = excluded.bytes_read,
                lines_read = excluded.lines_read,
                size_bytes = excluded.size_bytes,
                mtime_epoch = excluded.mtime_epoch,
                scanned_at = excluded.scanned_at
            ",
            params![
                provider,
                source_path,
                i64::try_from(state.bytes_read).unwrap_or(i64::MAX),
                i64::try_from(state.lines_read).unwrap_or(i64::MAX),
                i64::try_from(state.size_bytes).unwrap_or(i64::MAX),
                state.mtime_epoch,
            ],
        )
        .await
        .map_err(|err| format!("store usage scan file state failed: {err}"))?;
        Ok(())
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

        CREATE TABLE IF NOT EXISTS usage_samples (
            id INTEGER PRIMARY KEY,
            occurred_at INTEGER NOT NULL,
            instance_id TEXT,
            session_id INTEGER,
            workspace TEXT,
            provider TEXT NOT NULL,
            model TEXT NOT NULL,
            token_input INTEGER,
            token_output INTEGER,
            token_cache_read INTEGER,
            token_cache_write INTEGER,
            cost_usd_micros INTEGER,
            cost_source TEXT,
            source_hash TEXT
        );
        CREATE INDEX IF NOT EXISTS usage_samples_by_time
            ON usage_samples (occurred_at);
        CREATE INDEX IF NOT EXISTS usage_samples_by_session
            ON usage_samples (session_id, occurred_at);
        CREATE INDEX IF NOT EXISTS usage_samples_by_instance
            ON usage_samples (instance_id, occurred_at);
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

        CREATE TABLE IF NOT EXISTS usage_scan_files (
            provider TEXT NOT NULL,
            source_path TEXT NOT NULL,
            bytes_read INTEGER NOT NULL,
            lines_read INTEGER NOT NULL,
            size_bytes INTEGER NOT NULL,
            mtime_epoch INTEGER NOT NULL,
            scanned_at INTEGER NOT NULL,
            PRIMARY KEY(provider, source_path)
        );
        ",
    )
    .await
    .map_err(|err| format!("initialize telemetry store schema failed: {err}"))?;
    ensure_usage_samples_column(conn, "source_hash", "TEXT").await?;
    ensure_usage_samples_column(conn, "cost_source", "TEXT").await?;
    ensure_usage_samples_column(conn, "instance_id", "TEXT").await?;
    conn.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS usage_samples_by_source_hash
         ON usage_samples (source_hash)
         WHERE source_hash IS NOT NULL",
        (),
    )
    .await
    .map_err(|err| format!("initialize telemetry sample dedupe index failed: {err}"))?;
    conn.execute(
        "INSERT INTO _meta (key, value) VALUES ('schema_version', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [SCHEMA_VERSION],
    )
    .await
    .map_err(|err| format!("record telemetry store schema version failed: {err}"))?;
    Ok(())
}

async fn ensure_usage_samples_column(
    conn: &Connection,
    column_name: &str,
    column_type: &str,
) -> Result<(), String> {
    let mut rows = conn
        .query("PRAGMA table_info(usage_samples)", ())
        .await
        .map_err(|err| format!("inspect usage sample schema failed: {err}"))?;
    let mut columns = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|err| format!("read usage sample schema failed: {err}"))?
    {
        columns.push(row_string(&row, 1, "column_name")?);
    }
    if !columns.iter().any(|column| column == column_name) {
        conn.execute(
            &format!("ALTER TABLE usage_samples ADD COLUMN {column_name} {column_type}"),
            (),
        )
        .await
        .map_err(|err| format!("add usage sample {column_name} column failed: {err}"))?;
    }
    Ok(())
}

async fn insert_usage_sample_rows(
    conn: &Connection,
    samples: &[StoredUsageSample],
) -> Result<(), String> {
    for sample in samples {
        conn.execute(
            "
            INSERT OR IGNORE INTO usage_samples (
                occurred_at,
                instance_id,
                session_id,
                workspace,
                provider,
                model,
                token_input,
                token_output,
                token_cache_read,
                token_cache_write,
                cost_usd_micros,
                cost_source,
                source_hash
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            ",
            params![
                sample.occurred_at,
                sample.instance_id.clone(),
                sample.session_id,
                sample.workspace.clone(),
                sample.provider.clone(),
                sample.model.clone(),
                sample.token_input,
                sample.token_output,
                sample.token_cache_read,
                sample.token_cache_write,
                sample.cost_usd_micros,
                sample.cost_source.clone(),
                sample.source_hash.clone(),
            ],
        )
        .await
        .map_err(|err| format!("insert telemetry usage sample failed: {err}"))?;
    }
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

pub(crate) fn usage_summary(
    path: &Path,
    instance_id: Option<&str>,
    workspace: Option<&str>,
    session_id: Option<i64>,
    window_seconds: Option<i64>,
    now_epoch: i64,
) -> Result<UsageSummaryView, String> {
    let path = path.to_path_buf();
    let instance_id_param = instance_id.map(str::to_owned);
    let workspace_param = workspace.map(str::to_owned);
    let workspace_view = workspace.map(str::to_owned);
    let since = window_seconds.map(|window| now_epoch.saturating_sub(window.max(0)));
    run_store(move || async move {
        let conn = open_store(&path).await?;
        let mut rows = conn
            .query(
                "
                SELECT
                    COUNT(*),
                    COALESCE(SUM(token_input), 0),
                    COALESCE(SUM(token_output), 0),
                    COALESCE(SUM(token_cache_read), 0),
                    COALESCE(SUM(token_cache_write), 0),
                    COALESCE(SUM(cost_usd_micros), 0),
                    COALESCE(SUM(CASE WHEN cost_source = 'explicit_usd' AND cost_usd_micros IS NOT NULL THEN 1 ELSE 0 END), 0),
                    COALESCE(SUM(CASE WHEN cost_source = 'price_table' AND cost_usd_micros IS NOT NULL THEN 1 ELSE 0 END), 0),
                    COALESCE(SUM(CASE WHEN cost_usd_micros IS NULL THEN 1 ELSE 0 END), 0),
                    MIN(occurred_at),
                    MAX(occurred_at)
                FROM usage_samples
                WHERE (?1 IS NULL OR instance_id = ?1)
                  AND (?2 IS NULL OR workspace = ?2)
                  AND (?3 IS NULL OR session_id = ?3)
                  AND (?4 IS NULL OR occurred_at >= ?4)
                ",
                params![instance_id_param, workspace_param, session_id, since],
            )
            .await
            .map_err(|err| format!("query telemetry usage summary failed: {err}"))?;
        let row = rows
            .next()
            .await
            .map_err(|err| format!("read telemetry usage summary row failed: {err}"))?
            .ok_or_else(|| "telemetry usage summary returned no row".to_owned())?;
        let sample_count = row_i64(&row, 0, "sample_count")?;
        let token_input = row_i64(&row, 1, "token_input")?;
        let token_output = row_i64(&row, 2, "token_output")?;
        let token_cache_read = row_i64(&row, 3, "token_cache_read")?;
        let token_cache_write = row_i64(&row, 4, "token_cache_write")?;
        let cost_usd_micros = row_i64(&row, 5, "cost_usd_micros")?;
        let exact_cost_sample_count = row_i64(&row, 6, "exact_cost_sample_count")?;
        let estimated_cost_sample_count = row_i64(&row, 7, "estimated_cost_sample_count")?;
        let unpriced_sample_count = row_i64(&row, 8, "unpriced_sample_count")?;
        Ok(UsageSummaryView {
            workspace: workspace_view,
            session_id,
            window_seconds,
            sample_count: u64::try_from(sample_count).unwrap_or(0),
            token_input: u64::try_from(token_input).unwrap_or(0),
            token_output: u64::try_from(token_output).unwrap_or(0),
            token_cache_read: u64::try_from(token_cache_read).unwrap_or(0),
            token_cache_write: u64::try_from(token_cache_write).unwrap_or(0),
            cost_usd_micros: u64::try_from(cost_usd_micros).unwrap_or(0),
            exact_cost_sample_count: u64::try_from(exact_cost_sample_count).unwrap_or(0),
            estimated_cost_sample_count: u64::try_from(estimated_cost_sample_count).unwrap_or(0),
            unpriced_sample_count: u64::try_from(unpriced_sample_count).unwrap_or(0),
            first_occurred_at: row_opt_i64(&row, 9, "first_occurred_at")?,
            last_occurred_at: row_opt_i64(&row, 10, "last_occurred_at")?,
        })
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
fn stored_usage_samples(path: &Path) -> Result<Vec<StoredUsageSample>, String> {
    let path = path.to_path_buf();
    run_store(move || async move {
        let conn = open_store(&path).await?;
        let mut rows = conn
            .query(
                "
                SELECT
                    occurred_at,
                    instance_id,
                    session_id,
                    workspace,
                    provider,
                    model,
                    token_input,
                    token_output,
                    token_cache_read,
                    token_cache_write,
                    cost_usd_micros,
                    cost_source,
                    source_hash
                FROM usage_samples
                ORDER BY occurred_at, provider, model, source_hash
                ",
                (),
            )
            .await
            .map_err(|err| format!("query telemetry usage samples failed: {err}"))?;
        let mut samples = Vec::new();
        while let Some(row) = rows
            .next()
            .await
            .map_err(|err| format!("read telemetry usage sample row failed: {err}"))?
        {
            samples.push(StoredUsageSample {
                occurred_at: row_i64(&row, 0, "occurred_at")?,
                instance_id: row_opt_string(&row, 1, "instance_id")?,
                session_id: row_opt_i64(&row, 2, "session_id")?,
                workspace: row_opt_string(&row, 3, "workspace")?,
                provider: row_string(&row, 4, "provider")?,
                model: row_string(&row, 5, "model")?,
                token_input: row_opt_i64(&row, 6, "token_input")?,
                token_output: row_opt_i64(&row, 7, "token_output")?,
                token_cache_read: row_opt_i64(&row, 8, "token_cache_read")?,
                token_cache_write: row_opt_i64(&row, 9, "token_cache_write")?,
                cost_usd_micros: row_opt_i64(&row, 10, "cost_usd_micros")?,
                cost_source: row_opt_string(&row, 11, "cost_source")?,
                source_hash: row_string(&row, 12, "source_hash")?,
            });
        }
        Ok(samples)
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
            instance: None,
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
            Some("2")
        );
    }

    #[test]
    fn usage_scan_file_state_roundtrips() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("usage.db");
        let source = dir.path().join("session.jsonl");
        let state = UsageScanFileState {
            bytes_read: 128,
            lines_read: 3,
            size_bytes: 256,
            mtime_epoch: 1_781_185_560,
        };

        store_usage_scan_file_state(&db, "Codex", &source, state).expect("store scan state");

        assert_eq!(
            usage_scan_file_state(&db, "Codex", &source).expect("read scan state"),
            Some(state)
        );
        assert_eq!(
            usage_scan_file_state(&db, "Claude", &source).expect("read other provider"),
            None
        );
    }

    #[test]
    fn usage_samples_are_inserted_once_by_source_hash() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("usage.db");
        let sample = StoredUsageSample {
            occurred_at: 1_781_185_560,
            instance_id: Some("instance-a".to_owned()),
            session_id: Some(7),
            workspace: Some("capsule".to_owned()),
            provider: "Claude".to_owned(),
            model: "claude-sonnet-4-5".to_owned(),
            token_input: Some(10),
            token_output: Some(20),
            token_cache_read: Some(3),
            token_cache_write: Some(4),
            cost_usd_micros: Some(0),
            cost_source: Some("explicit_usd".to_owned()),
            source_hash: "sha256:sample".to_owned(),
        };

        store_usage_samples(&db, std::slice::from_ref(&sample)).expect("store sample");
        store_usage_samples(&db, &[sample]).expect("dedupe sample");

        let rows = stored_usage_samples(&db).expect("read samples");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].provider, "Claude");
        assert_eq!(rows[0].model, "claude-sonnet-4-5");
        assert_eq!(rows[0].token_input, Some(10));
        assert_eq!(rows[0].token_output, Some(20));
        assert_eq!(rows[0].token_cache_read, Some(3));
        assert_eq!(rows[0].token_cache_write, Some(4));
        assert_eq!(rows[0].cost_source.as_deref(), Some("explicit_usd"));
    }

    #[test]
    fn usage_summary_filters_samples() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("usage.db");
        let samples = vec![
            StoredUsageSample {
                occurred_at: 1_781_185_500,
                instance_id: Some("instance-a".to_owned()),
                session_id: Some(7),
                workspace: Some("capsule".to_owned()),
                provider: "Codex".to_owned(),
                model: "gpt-5.5".to_owned(),
                token_input: Some(10),
                token_output: Some(20),
                token_cache_read: Some(3),
                token_cache_write: Some(4),
                cost_usd_micros: Some(50),
                cost_source: Some("price_table".to_owned()),
                source_hash: "sha256:a".to_owned(),
            },
            StoredUsageSample {
                occurred_at: 1_781_185_560,
                instance_id: Some("instance-b".to_owned()),
                session_id: Some(8),
                workspace: Some("capsule".to_owned()),
                provider: "Claude".to_owned(),
                model: "claude-sonnet-4-5".to_owned(),
                token_input: Some(100),
                token_output: Some(200),
                token_cache_read: None,
                token_cache_write: None,
                cost_usd_micros: None,
                cost_source: None,
                source_hash: "sha256:b".to_owned(),
            },
        ];

        store_usage_samples(&db, &samples).expect("store samples");

        let workspace_summary =
            usage_summary(&db, None, Some("capsule"), None, Some(120), 1_781_185_600)
                .expect("workspace summary");
        assert_eq!(workspace_summary.sample_count, 2);
        assert_eq!(workspace_summary.token_input, 110);
        assert_eq!(workspace_summary.token_output, 220);
        assert_eq!(workspace_summary.token_cache_read, 3);
        assert_eq!(workspace_summary.token_cache_write, 4);
        assert_eq!(workspace_summary.cost_usd_micros, 50);
        assert_eq!(workspace_summary.estimated_cost_sample_count, 1);
        assert_eq!(workspace_summary.exact_cost_sample_count, 0);
        assert_eq!(workspace_summary.unpriced_sample_count, 1);
        assert_eq!(workspace_summary.first_occurred_at, Some(1_781_185_500));
        assert_eq!(workspace_summary.last_occurred_at, Some(1_781_185_560));

        let session_summary =
            usage_summary(&db, None, None, Some(8), None, 1_781_185_600).expect("session summary");
        assert_eq!(session_summary.sample_count, 1);
        assert_eq!(session_summary.token_input, 100);
        assert_eq!(session_summary.unpriced_sample_count, 1);
        assert_eq!(session_summary.session_id, Some(8));

        let instance_summary =
            usage_summary(&db, Some("instance-a"), None, None, None, 1_781_185_600)
                .expect("instance summary");
        assert_eq!(instance_summary.sample_count, 1);
        assert_eq!(instance_summary.token_input, 10);

        let instance_workspace_summary = usage_summary(
            &db,
            Some("instance-a"),
            Some("capsule"),
            None,
            None,
            1_781_185_600,
        )
        .expect("instance workspace summary");
        assert_eq!(instance_workspace_summary.sample_count, 1);
        assert_eq!(instance_workspace_summary.token_input, 10);
    }
}
