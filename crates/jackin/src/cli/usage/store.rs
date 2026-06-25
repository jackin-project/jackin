use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use jackin_protocol::control::AccountUsageSnapshotView;
use sha2::{Digest, Sha256};
use turso::{Connection, Row, params};

use crate::paths::JackinPaths;

const SCHEMA_VERSION: &str = "1";

pub(super) async fn upsert_accounts(
    paths: &JackinPaths,
    accounts: &[AccountUsageSnapshotView],
) -> Result<PathBuf> {
    let path = host_account_cache_path(paths);
    let conn = open_store(&path).await?;
    upsert_account_rows(&conn, accounts).await?;
    Ok(path)
}

pub(super) async fn read_accounts(
    paths: &JackinPaths,
) -> Result<(PathBuf, Vec<AccountUsageSnapshotView>)> {
    let path = host_account_cache_path(paths);
    if !path.exists() {
        return Ok((path, Vec::new()));
    }
    let conn = open_existing_store(&path).await?;
    if !table_exists(&conn, "account_usage_snapshots").await? {
        return Ok((path, Vec::new()));
    }
    let accounts = read_account_rows(&conn).await?;
    Ok((path, accounts))
}

fn host_account_cache_path(paths: &JackinPaths) -> PathBuf {
    paths.data_dir.join("daemon").join("accounts.db")
}

async fn open_store(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create host usage cache dir {}", parent.display()))?;
    }
    let path = path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("host usage cache path is not UTF-8"))?;
    let db = turso::Builder::new_local(path)
        .build()
        .await
        .context("open host usage cache")?;
    let conn = db.connect().context("connect host usage cache")?;
    initialize_schema(&conn).await?;
    Ok(conn)
}

async fn open_existing_store(path: &Path) -> Result<Connection> {
    let path = path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("host usage cache path is not UTF-8"))?;
    let db = turso::Builder::new_local(path)
        .build()
        .await
        .context("open host usage cache")?;
    db.connect().context("connect host usage cache")
}

async fn initialize_schema(conn: &Connection) -> Result<()> {
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
    .context("initialize host usage cache schema")?;
    conn.execute(
        "INSERT INTO _meta (key, value) VALUES ('schema_version', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [SCHEMA_VERSION],
    )
    .await
    .context("record host usage cache schema version")?;
    Ok(())
}

async fn table_exists(conn: &Connection, table: &str) -> Result<bool> {
    let mut rows = conn
        .query(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table],
        )
        .await
        .context("query host usage cache schema")?;
    Ok(rows
        .next()
        .await
        .context("read host usage cache schema row")?
        .is_some())
}

async fn upsert_account_rows(
    conn: &Connection,
    accounts: &[AccountUsageSnapshotView],
) -> Result<()> {
    for account in accounts {
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
                account.provider.clone(),
                account_key_hash(&account.provider, &account.account_label),
                account.account_label.clone(),
                account.source.clone(),
                account.confidence.clone(),
                account.window_kind.clone(),
                account.used_amount,
                account.used_unit.clone(),
                account.limit_amount,
                account.limit_unit.clone(),
                account.resets_at,
                account.fetched_at,
                account.expires_at,
                account.status.clone(),
                account.last_error.clone(),
            ],
        )
        .await
        .context("upsert host usage account snapshot")?;
    }
    Ok(())
}

async fn read_account_rows(conn: &Connection) -> Result<Vec<AccountUsageSnapshotView>> {
    let mut rows = conn
        .query(
            "
            SELECT
                provider,
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
            ORDER BY provider, account_label, source, window_kind
            ",
            (),
        )
        .await
        .context("query host usage account snapshots")?;
    let mut accounts = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .context("read host usage account snapshot row")?
    {
        accounts.push(AccountUsageSnapshotView {
            provider: row_string(&row, 0)?,
            account_label: row_string(&row, 1)?,
            source: row_string(&row, 2)?,
            confidence: row_string(&row, 3)?,
            window_kind: row_string(&row, 4)?,
            used_amount: row_opt_i64(&row, 5)?,
            used_unit: row_opt_string(&row, 6)?,
            limit_amount: row_opt_i64(&row, 7)?,
            limit_unit: row_opt_string(&row, 8)?,
            resets_at: row_opt_i64(&row, 9)?,
            fetched_at: row_i64(&row, 10)?,
            expires_at: row_opt_i64(&row, 11)?,
            status: row_string(&row, 12)?,
            last_error: row_opt_string(&row, 13)?,
        });
    }
    Ok(accounts)
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

#[cfg(test)]
pub(super) async fn count_account_rows(path: PathBuf) -> Result<i64> {
    let conn = open_store(&path).await?;
    let mut rows = conn
        .query("SELECT COUNT(*) FROM account_usage_snapshots", ())
        .await
        .context("count host usage account rows")?;
    let row = rows
        .next()
        .await
        .context("read host usage account count")?
        .ok_or_else(|| anyhow::anyhow!("missing host usage account count row"))?;
    row_i64(&row, 0)
}

fn row_i64(row: &Row, index: usize) -> Result<i64> {
    row.get(index)
        .with_context(|| format!("read integer column {index}"))
}

fn row_opt_i64(row: &Row, index: usize) -> Result<Option<i64>> {
    row.get(index)
        .with_context(|| format!("read optional integer column {index}"))
}

fn row_string(row: &Row, index: usize) -> Result<String> {
    row.get(index)
        .with_context(|| format!("read text column {index}"))
}

fn row_opt_string(row: &Row, index: usize) -> Result<Option<String>> {
    row.get(index)
        .with_context(|| format!("read optional text column {index}"))
}

#[cfg(test)]
mod tests;
