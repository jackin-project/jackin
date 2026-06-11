use std::path::PathBuf;

use anyhow::{Context, Result};
use jackin_protocol::control::AccountUsageSnapshotView;
use sha2::{Digest, Sha256};
use turso::{Connection, params};

#[cfg(test)]
use turso::Row;

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

fn host_account_cache_path(paths: &JackinPaths) -> PathBuf {
    paths.data_dir.join("daemon").join("accounts.db")
}

async fn open_store(path: &PathBuf) -> Result<Connection> {
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

#[cfg(test)]
fn row_i64(row: &Row, index: usize) -> Result<i64> {
    row.get_value(index)
        .context("read integer column")?
        .as_integer()
        .ok_or_else(|| anyhow::anyhow!("column {index} is not an integer"))
        .copied()
}

#[cfg(test)]
mod tests;
