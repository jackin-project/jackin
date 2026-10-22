// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use jackin_core::account_key_hash;
use jackin_protocol::control::AccountUsageSnapshotView;
use jackin_usage::store_backend::{
    BackupReceipt, CommittedMigrationValidation, Connection, DbOperation, LocalStore,
    ManagedStoreKind, Row, StoreOperationGuard, acquire_store_operation, operation, params,
};

use jackin_core::JackinPaths;

const SCHEMA_VERSION: &str = "2";

pub(super) async fn upsert_accounts(
    paths: &JackinPaths,
    accounts: &[AccountUsageSnapshotView],
) -> Result<PathBuf> {
    let path = host_account_cache_path(paths);
    let store_operation = acquire_store_operation(&path)
        .await
        .context("acquire host usage cache operation")?;
    let store = open_store(&path, &store_operation).await?;
    upsert_account_rows(store.connection(), accounts).await?;
    Ok(path)
}

pub(super) async fn read_accounts(
    paths: &JackinPaths,
) -> Result<(PathBuf, Vec<AccountUsageSnapshotView>)> {
    let path = host_account_cache_path(paths);
    let store_operation = acquire_store_operation(&path)
        .await
        .context("acquire host usage cache operation")?;
    if !path.exists() {
        return Ok((path, Vec::new()));
    }
    let store = open_existing_store(&path, &store_operation).await?;
    let conn = store.connection();
    let version = stored_schema_version(conn).await?;
    ensure_supported_schema_version(version.clone())?;
    validate_existing_schema_shape(conn, version.as_deref()).await?;
    if !table_exists(conn, "account_usage_snapshots").await? {
        return Ok((path, Vec::new()));
    }
    initialize_schema(&store).await?;
    let accounts = read_account_rows(conn).await?;
    Ok((path, accounts))
}

fn host_account_cache_path(paths: &JackinPaths) -> PathBuf {
    paths.data_dir.join("daemon").join("accounts.db")
}

async fn open_store(path: &Path, operation: &StoreOperationGuard) -> Result<LocalStore> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create host usage cache dir {}", parent.display()))?;
    }
    let store = open_existing_store(path, operation).await?;
    initialize_schema(&store).await?;
    Ok(store)
}

async fn open_existing_store(path: &Path, operation: &StoreOperationGuard) -> Result<LocalStore> {
    LocalStore::open_managed(path, ManagedStoreKind::CliUsage, operation)
        .await
        .map_err(|err| anyhow::anyhow!("host usage cache: {err}"))
}

async fn initialize_schema(store: &LocalStore) -> Result<()> {
    let conn = store.connection();
    operation(
        DbOperation::Update,
        conn.execute_batch("PRAGMA foreign_keys = ON"),
    )
    .await
    .context("enable host usage cache foreign keys")?;
    let prior_version = stored_schema_version(conn).await?;
    ensure_supported_schema_version(prior_version.clone())?;
    validate_existing_schema_shape(conn, prior_version.as_deref()).await?;
    if prior_version.as_deref() == Some(SCHEMA_VERSION) {
        let validation = validate_committed_schema(store).await?;
        store
            .mark_current_schema_migration_committed(
                ManagedStoreKind::CliUsage,
                SCHEMA_VERSION.parse::<u64>().unwrap_or_default(),
                validation,
            )
            .await
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        return Ok(());
    }
    operation(DbOperation::Begin, conn.execute("BEGIN IMMEDIATE", ()))
        .await
        .context("begin host usage cache migration")?;
    let migration = migrate_schema(store).await;
    match migration {
        Ok(receipt) => match operation(DbOperation::Commit, conn.execute("COMMIT", ())).await {
            Ok(_) => {
                if let Some(receipt) = receipt {
                    let validation = validate_committed_schema(store).await?;
                    store
                        .mark_migration_committed(&receipt, validation)
                        .await
                        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                } else {
                    // A competing writer may have committed the migration
                    // while this caller waited for BEGIN IMMEDIATE. Finalize
                    // any pending receipt only after our read-only transaction
                    // has committed and current-schema validation succeeded.
                    let validation = validate_committed_schema(store).await?;
                    store
                        .mark_current_schema_migration_committed(
                            ManagedStoreKind::CliUsage,
                            SCHEMA_VERSION.parse::<u64>().unwrap_or_default(),
                            validation,
                        )
                        .await
                        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                }
                Ok(())
            }
            Err(commit_error) => {
                let rollback = operation(DbOperation::Rollback, conn.execute("ROLLBACK", ())).await;
                match rollback {
                    Ok(_) => Err(anyhow::anyhow!(
                        "commit host usage cache migration failed: {commit_error}"
                    )),
                    Err(rollback_error) => Err(anyhow::anyhow!(
                        "commit host usage cache migration failed: {commit_error}; rollback failed: {rollback_error}"
                    )),
                }
            }
        },
        Err(error) => {
            let rollback = operation(DbOperation::Rollback, conn.execute("ROLLBACK", ())).await;
            match rollback {
                Ok(_) => Err(error),
                Err(rollback_error) => Err(anyhow::anyhow!(
                    "host usage cache migration failed: {error:#}; rollback failed: {rollback_error}"
                )),
            }
        }
    }
}

async fn validate_committed_schema(store: &LocalStore) -> Result<CommittedMigrationValidation> {
    validate_current_schema(store.connection()).await?;
    let validation = store
        .validate_committed_migration(
            ManagedStoreKind::CliUsage,
            SCHEMA_VERSION.parse::<u64>().unwrap_or_default(),
        )
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    validate_current_schema(validation.connection()).await?;
    Ok(validation)
}

async fn migrate_schema(store: &LocalStore) -> Result<Option<BackupReceipt>> {
    let conn = store.connection();
    let prior_version = stored_schema_version(conn).await?;
    ensure_supported_schema_version(prior_version.clone())?;
    validate_existing_schema_shape(conn, prior_version.as_deref()).await?;
    if prior_version.as_deref() == Some(SCHEMA_VERSION) {
        validate_current_schema(conn).await?;
        return Ok(None);
    }
    let old_version = prior_version
        .as_deref()
        .map(str::parse::<u64>)
        .transpose()
        .context("parse old host usage cache schema version")?;
    let backup_receipt = store
        .backup_before_migration(
            ManagedStoreKind::CliUsage,
            old_version,
            SCHEMA_VERSION.parse::<u64>().unwrap_or_default(),
        )
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    operation(
        DbOperation::Update,
        conn.execute_batch(
            "
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
            quota_bucket_json TEXT,
            UNIQUE(provider, account_key_hash, source, window_kind)
        );
        ",
        ),
    )
    .await
    .context("initialize host usage cache schema")?;
    let columns = account_snapshot_columns(conn).await?;
    if !columns.contains("quota_bucket_json") {
        operation(
            DbOperation::Update,
            conn.execute(
                "ALTER TABLE account_usage_snapshots ADD COLUMN quota_bucket_json TEXT",
                (),
            ),
        )
        .await
        .context("add typed quota bucket to host usage cache")?;
    }
    let previous = prior_version.unwrap_or_else(|| "unversioned".to_owned());
    operation(
        DbOperation::Upsert,
        conn.execute(
            "INSERT OR IGNORE INTO _meta (key, value) VALUES ('schema_version_before_quota_v2', ?1)",
            [previous],
        ),
    )
    .await
    .context("preserve prior host usage cache schema version")?;
    operation(
        DbOperation::Upsert,
        conn.execute(
            "INSERT INTO _meta (key, value) VALUES ('schema_version', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [SCHEMA_VERSION],
        ),
    )
    .await
    .context("record host usage cache schema version")?;
    validate_current_schema(conn).await?;
    #[cfg(test)]
    inject_requested_commit_failure(conn).await?;
    Ok(backup_receipt)
}

#[cfg(test)]
async fn inject_requested_commit_failure(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "_meta").await? {
        return Ok(());
    }
    let mut rows = operation(
        DbOperation::Select,
        conn.query(
            "SELECT value FROM _meta WHERE key = 'test_fail_schema_commit'",
            (),
        ),
    )
    .await
    .context("read host cache commit failure marker")?;
    let marker = rows
        .next()
        .await
        .context("read host cache commit failure marker row")?
        .map(|row| row_string(&row, 0))
        .transpose()?;
    drop(rows);
    if marker.as_deref() == Some("1") {
        operation(
            DbOperation::Update,
            conn.execute_batch(
                "CREATE TEMP TABLE quota_commit_parent(id INTEGER PRIMARY KEY);
                 CREATE TEMP TABLE quota_commit_child(
                     parent_id INTEGER REFERENCES quota_commit_parent(id)
                         DEFERRABLE INITIALLY DEFERRED
                 );
                 INSERT INTO quota_commit_child(parent_id) VALUES (1);",
            ),
        )
        .await
        .context("prepare host cache commit failure marker")?;
    }
    Ok(())
}

async fn validate_existing_schema_shape(conn: &Connection, version: Option<&str>) -> Result<()> {
    let has_meta = table_exists(conn, "_meta").await?;
    let has_snapshots = table_exists(conn, "account_usage_snapshots").await?;
    if !has_snapshots {
        if version.is_some() || has_meta {
            anyhow::bail!(
                "versioned or partial host usage cache is missing account_usage_snapshots"
            );
        }
        return Ok(());
    }

    let columns = account_snapshot_columns(conn).await?;
    if version != Some(SCHEMA_VERSION) && columns.contains("quota_bucket_json") {
        anyhow::bail!("pre-v2 host usage cache contains a partial typed quota column");
    }
    for required in [
        "id",
        "provider",
        "account_key_hash",
        "account_label",
        "source",
        "confidence",
        "window_kind",
        "used_amount",
        "used_unit",
        "limit_amount",
        "limit_unit",
        "resets_at",
        "fetched_at",
        "expires_at",
        "status",
        "last_error",
    ] {
        if !columns.contains(required) {
            anyhow::bail!("host usage cache is missing required legacy column {required}");
        }
    }
    Ok(())
}

fn ensure_supported_schema_version(version: Option<String>) -> Result<()> {
    let Some(version) = version else {
        return Ok(());
    };
    let parsed = version
        .parse::<u32>()
        .with_context(|| format!("unsupported host usage cache schema version {version:?}"))?;
    if parsed > 2 {
        anyhow::bail!("host usage cache schema {parsed} is newer than supported schema 2");
    }
    Ok(())
}

async fn validate_current_schema(conn: &Connection) -> Result<()> {
    let columns = account_snapshot_columns(conn).await?;
    for required in [
        "provider",
        "account_key_hash",
        "account_label",
        "source",
        "confidence",
        "window_kind",
        "used_amount",
        "used_unit",
        "limit_amount",
        "limit_unit",
        "resets_at",
        "fetched_at",
        "expires_at",
        "status",
        "last_error",
        "quota_bucket_json",
    ] {
        if !columns.contains(required) {
            anyhow::bail!("host usage cache schema 2 is missing {required}");
        }
    }
    operation(DbOperation::Select, async {
        let mut rows = conn
            .query(
                "SELECT id, window_kind, quota_bucket_json FROM account_usage_snapshots ORDER BY id",
                (),
            )
            .await
            .context("validate host usage typed quota rows")?;
        while let Some(row) = rows
            .next()
            .await
            .context("read host usage typed quota row")?
        {
            let id: i64 = row.get(0).context("read host usage quota row id")?;
            let window: String = row.get(1).context("read host usage quota window")?;
            let encoded: Option<String> = row
                .get(2)
                .context("read host usage quota payload")?;
            if let Some(encoded) = encoded {
                let bucket: jackin_protocol::control::QuotaBucketView =
                    serde_json::from_str(&encoded).with_context(|| {
                        format!("decode quota_bucket_json for host usage row {id} ({window:?})")
                    })?;
                bucket.validate().map_err(|error| {
                    anyhow::anyhow!(
                        "validate quota_bucket_json for host usage row {id} ({window:?}): {error}"
                    )
                })?;
            }
        }
        Ok::<(), anyhow::Error>(())
    })
    .await?;
    Ok(())
}

async fn account_snapshot_columns(conn: &Connection) -> Result<std::collections::HashSet<String>> {
    operation(DbOperation::Select, async {
        let mut rows = conn
            .query("PRAGMA table_info(account_usage_snapshots)", ())
            .await
            .context("inspect host usage cache columns")?;
        let mut columns = std::collections::HashSet::new();
        while let Some(row) = rows.next().await.context("read host usage cache column")? {
            columns.insert(row_string(&row, 1)?);
        }
        Ok(columns)
    })
    .await
}

async fn stored_schema_version(conn: &Connection) -> Result<Option<String>> {
    if !table_exists(conn, "_meta").await? {
        return Ok(None);
    }
    operation(DbOperation::Select, async {
        let mut rows = conn
            .query("SELECT value FROM _meta WHERE key = 'schema_version'", ())
            .await
            .context("read host usage cache schema version")?;
        rows.next()
            .await
            .context("read host usage cache schema version row")?
            .map(|row| row_string(&row, 0))
            .transpose()
    })
    .await
}

async fn table_exists(conn: &Connection, table: &str) -> Result<bool> {
    operation(DbOperation::Select, async {
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
    })
    .await
}

async fn upsert_account_rows(
    conn: &Connection,
    accounts: &[AccountUsageSnapshotView],
) -> Result<()> {
    let rows = accounts
        .iter()
        .map(|account| {
            let bucket = account.quota_bucket.as_ref().ok_or_else(|| {
                anyhow::anyhow!(
                    "host usage account {:?} is missing its typed quota bucket",
                    account.window_kind
                )
            })?;
            bucket.validate().map_err(|error| {
                anyhow::anyhow!(
                    "validate host usage quota bucket {:?}: {error}",
                    account.window_kind
                )
            })?;
            let bucket_json =
                serde_json::to_string(bucket).context("encode host usage quota bucket")?;
            Ok((account, bucket_json))
        })
        .collect::<Result<Vec<_>>>()?;
    if rows.is_empty() {
        return Ok(());
    }
    operation(DbOperation::Begin, conn.execute("BEGIN IMMEDIATE", ()))
        .await
        .context("begin host usage account snapshot upsert")?;
    let upsert = async {
        for (account, bucket_json) in rows {
            operation(
                DbOperation::Upsert,
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
                last_error,
                quota_bucket_json
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
            ON CONFLICT(provider, account_key_hash, source, window_kind) DO UPDATE SET
                account_label = excluded.account_label,
                confidence = excluded.confidence,
                used_amount = CASE
                    WHEN excluded.used_amount IS NULL AND excluded.used_unit IS NULL
                    THEN account_usage_snapshots.used_amount
                    ELSE excluded.used_amount
                END,
                used_unit = CASE
                    WHEN excluded.used_amount IS NULL AND excluded.used_unit IS NULL
                    THEN account_usage_snapshots.used_unit
                    ELSE excluded.used_unit
                END,
                limit_amount = CASE
                    WHEN excluded.limit_amount IS NULL AND excluded.limit_unit IS NULL
                    THEN account_usage_snapshots.limit_amount
                    ELSE excluded.limit_amount
                END,
                limit_unit = CASE
                    WHEN excluded.limit_amount IS NULL AND excluded.limit_unit IS NULL
                    THEN account_usage_snapshots.limit_unit
                    ELSE excluded.limit_unit
                END,
                resets_at = excluded.resets_at,
                fetched_at = excluded.fetched_at,
                expires_at = excluded.expires_at,
                status = excluded.status,
                last_error = excluded.last_error,
                quota_bucket_json = excluded.quota_bucket_json
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
                        bucket_json,
                    ],
                ),
            )
            .await
            .context("upsert host usage account snapshot")?;
        }
        Ok::<(), anyhow::Error>(())
    }
    .await;
    if let Err(error) = upsert {
        let rollback = operation(DbOperation::Rollback, conn.execute("ROLLBACK", ())).await;
        return match rollback {
            Ok(_) => Err(error),
            Err(rollback_error) => Err(anyhow::anyhow!(
                "upsert host usage account snapshot failed: {error:#}; rollback failed: {rollback_error}"
            )),
        };
    }
    match operation(DbOperation::Commit, conn.execute("COMMIT", ())).await {
        Ok(_) => Ok(()),
        Err(commit_error) => {
            let rollback = operation(DbOperation::Rollback, conn.execute("ROLLBACK", ())).await;
            match rollback {
                Ok(_) => Err(anyhow::anyhow!(
                    "commit host usage account snapshot upsert failed: {commit_error}"
                )),
                Err(rollback_error) => Err(anyhow::anyhow!(
                    "commit host usage account snapshot upsert failed: {commit_error}; rollback failed: {rollback_error}"
                )),
            }
        }
    }
}

async fn read_account_rows(conn: &Connection) -> Result<Vec<AccountUsageSnapshotView>> {
    operation(DbOperation::Select, async {
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
                last_error,
                quota_bucket_json
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
            let window_kind = row_string(&row, 4)?;
            let quota_bucket = row_opt_string(&row, 14)?
                .map(|json| {
                    let bucket: jackin_protocol::control::QuotaBucketView =
                        serde_json::from_str(&json).with_context(|| {
                            format!("decode host usage quota bucket for {window_kind:?}")
                        })?;
                    bucket.validate().map_err(|error| {
                        anyhow::anyhow!(
                            "validate host usage quota bucket for {window_kind:?}: {error}"
                        )
                    })?;
                    Ok::<jackin_protocol::control::QuotaBucketView, anyhow::Error>(bucket)
                })
                .transpose()?;
            accounts.push(AccountUsageSnapshotView {
                provider: row_string(&row, 0)?,
                account_label: row_string(&row, 1)?,
                source: row_string(&row, 2)?,
                confidence: row_string(&row, 3)?,
                window_kind,
                used_amount: row_opt_i64(&row, 5)?,
                used_unit: row_opt_string(&row, 6)?,
                limit_amount: row_opt_i64(&row, 7)?,
                limit_unit: row_opt_string(&row, 8)?,
                resets_at: row_opt_i64(&row, 9)?,
                fetched_at: row_i64(&row, 10)?,
                expires_at: row_opt_i64(&row, 11)?,
                status: row_string(&row, 12)?,
                last_error: row_opt_string(&row, 13)?,
                quota_bucket,
                account_capability: None,
            });
        }
        Ok(accounts)
    })
    .await
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
