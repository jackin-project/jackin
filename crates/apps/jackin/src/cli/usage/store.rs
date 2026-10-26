// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use jackin_core::account_key_hash;
use jackin_protocol::control::AccountUsageSnapshotView;
use jackin_protocol::usage_broker::UsageAccountCapability;
use jackin_usage::store_backend::{
    BackupReceipt, CommittedMigrationValidation, Connection, DbOperation, LocalStore,
    ManagedStoreKind, Row, StoreOperationGuard, acquire_store_operation, operation, params,
};

use jackin_core::JackinPaths;

const SCHEMA_VERSION: &str = "3";

mod identity;

struct PreparedAccount {
    account: AccountUsageSnapshotView,
    account_key_hash: String,
    quota_bucket_json: String,
}

struct SnapshotColumn {
    declared_type: String,
    not_null: bool,
    primary_key_position: i64,
}

struct StoredIdentity {
    provider: String,
    account_key_hash: String,
    capability: Option<UsageAccountCapability>,
}

pub(super) async fn upsert_accounts(
    paths: &JackinPaths,
    accounts: &[AccountUsageSnapshotView],
) -> Result<PathBuf> {
    let rows = prepare_account_rows(accounts)?;
    let path = host_account_cache_path(paths);
    let store_operation = acquire_store_operation(&path)
        .await
        .context("acquire host usage cache operation")?;
    let store = open_store(&path, &store_operation).await?;
    upsert_prepared_account_rows(store.connection(), &rows).await?;
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
            capability_account_id TEXT,
            capability_surface_id TEXT,
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
    if !columns.contains("capability_account_id") {
        operation(
            DbOperation::Update,
            conn.execute(
                "ALTER TABLE account_usage_snapshots ADD COLUMN capability_account_id TEXT",
                (),
            ),
        )
        .await
        .context("add host usage account capability id")?;
    }
    if !columns.contains("capability_surface_id") {
        operation(
            DbOperation::Update,
            conn.execute(
                "ALTER TABLE account_usage_snapshots ADD COLUMN capability_surface_id TEXT",
                (),
            ),
        )
        .await
        .context("add host usage capability surface id")?;
    }
    let previous = prior_version.unwrap_or_else(|| "unversioned".to_owned());
    if previous != "2" {
        operation(
            DbOperation::Upsert,
            conn.execute(
                "INSERT OR IGNORE INTO _meta (key, value) VALUES ('schema_version_before_quota_v2', ?1)",
                [previous.clone()],
            ),
        )
        .await
        .context("preserve prior host usage cache schema version")?;
    }
    operation(
        DbOperation::Upsert,
        conn.execute(
            "INSERT OR IGNORE INTO _meta (key, value) VALUES ('schema_version_before_capability_v3', ?1)",
            [previous],
        ),
    )
    .await
    .context("preserve prior host usage capability schema version")?;
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
    Ok(backup_receipt)
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
    match version {
        None | Some("0" | "1") => {
            if columns.contains("quota_bucket_json") {
                anyhow::bail!("pre-v2 host usage cache contains a partial typed quota column");
            }
            if columns.contains("capability_account_id")
                || columns.contains("capability_surface_id")
            {
                anyhow::bail!("pre-v3 host usage cache contains partial capability columns");
            }
            validate_snapshot_columns(conn, "pre-v2", false, false).await?;
            validate_unique_identity_key(conn).await?;
        }
        Some("2") => {
            if columns.contains("capability_account_id")
                || columns.contains("capability_surface_id")
            {
                anyhow::bail!("schema 2 contains unexpected capability identity columns");
            }
            validate_snapshot_columns(conn, "2", true, false).await?;
            validate_unique_identity_key(conn).await?;
            validate_typed_quota_rows(conn).await?;
            validate_legacy_identity_rows(conn).await?;
        }
        Some("3") => {
            validate_current_columns(conn).await?;
            validate_unique_identity_key(conn).await?;
        }
        Some(other) => {
            anyhow::bail!("host usage cache schema {other:?} is not a recognized version");
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
    if parsed > 3 {
        anyhow::bail!("host usage cache schema {parsed} is newer than supported schema 3");
    }
    Ok(())
}

async fn validate_current_schema(conn: &Connection) -> Result<()> {
    validate_current_columns(conn).await?;
    validate_unique_identity_key(conn).await?;
    validate_typed_quota_rows(conn).await?;
    read_stored_identities(conn).await?;
    Ok(())
}

async fn validate_typed_quota_rows(conn: &Connection) -> Result<()> {
    operation(DbOperation::Select, async {
        let mut rows = conn
            .query(
                "SELECT id, window_kind, quota_bucket_json, typeof(quota_bucket_json) FROM account_usage_snapshots ORDER BY id",
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
            let encoded_type = row_string(&row, 3)?;
            if !matches!(encoded_type.as_str(), "null" | "text") {
                anyhow::bail!(
                    "quota_bucket_json for host usage row {id} ({window:?}) must be NULL or TEXT, got {encoded_type}"
                );
            }
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

async fn validate_current_columns(conn: &Connection) -> Result<()> {
    validate_snapshot_columns(conn, SCHEMA_VERSION, true, true).await?;
    let columns = account_snapshot_column_info(conn).await?;
    for name in ["capability_account_id", "capability_surface_id"] {
        if columns.get(name).is_some_and(|column| column.not_null) {
            anyhow::bail!("host usage cache schema 3 capability columns must be nullable");
        }
    }
    Ok(())
}

async fn validate_snapshot_columns(
    conn: &Connection,
    schema: &str,
    include_quota: bool,
    include_capability: bool,
) -> Result<()> {
    let mut required = vec![
        ("id", "INTEGER", None),
        ("provider", "TEXT", Some(true)),
        ("account_key_hash", "TEXT", Some(true)),
        ("account_label", "TEXT", Some(true)),
        ("source", "TEXT", Some(true)),
        ("confidence", "TEXT", Some(true)),
        ("window_kind", "TEXT", Some(true)),
        ("used_amount", "INTEGER", Some(false)),
        ("used_unit", "TEXT", Some(false)),
        ("limit_amount", "INTEGER", Some(false)),
        ("limit_unit", "TEXT", Some(false)),
        ("resets_at", "INTEGER", Some(false)),
        ("fetched_at", "INTEGER", Some(true)),
        ("expires_at", "INTEGER", Some(false)),
        ("status", "TEXT", Some(true)),
        ("last_error", "TEXT", Some(false)),
    ];
    if include_quota {
        required.push(("quota_bucket_json", "TEXT", Some(false)));
    }
    if include_capability {
        required.extend([
            ("capability_account_id", "TEXT", None),
            ("capability_surface_id", "TEXT", None),
        ]);
    }

    let columns = account_snapshot_column_info(conn).await?;
    for (name, expected_type, expected_not_null) in required {
        let Some(column) = columns.get(name) else {
            anyhow::bail!("host usage cache schema {schema} is missing {name}");
        };
        if !sqlite_type_matches(&column.declared_type, expected_type) {
            anyhow::bail!(
                "host usage cache schema {schema} has incompatible type for {name}: {:?}",
                column.declared_type
            );
        }
        if expected_not_null.is_some_and(|expected| column.not_null != expected) {
            anyhow::bail!(
                "host usage cache schema {schema} has incompatible nullability for {name}"
            );
        }
    }
    validate_rowid_primary_key(conn, schema, &columns).await?;
    Ok(())
}

fn sqlite_type_matches(declared_type: &str, expected: &str) -> bool {
    let declared_type = declared_type.to_ascii_uppercase();
    let affinity = if declared_type.contains("INT") {
        "INTEGER"
    } else if declared_type.contains("CHAR")
        || declared_type.contains("CLOB")
        || declared_type.contains("TEXT")
    {
        "TEXT"
    } else if declared_type.contains("BLOB") || declared_type.trim().is_empty() {
        "BLOB"
    } else if declared_type.contains("REAL")
        || declared_type.contains("FLOA")
        || declared_type.contains("DOUB")
    {
        "REAL"
    } else {
        "NUMERIC"
    };
    affinity == expected
}

async fn validate_rowid_primary_key(
    conn: &Connection,
    schema: &str,
    columns: &std::collections::HashMap<String, SnapshotColumn>,
) -> Result<()> {
    let has_rowid_id = columns.get("id").is_some_and(|column| {
        column.primary_key_position == 1
            && column.declared_type.trim().eq_ignore_ascii_case("INTEGER")
    });
    let has_other_primary_key_column = columns
        .iter()
        .any(|(name, column)| name != "id" && column.primary_key_position != 0);
    if !has_rowid_id || has_other_primary_key_column {
        anyhow::bail!(
            "host usage cache schema {schema} id must be a rowid-generating INTEGER PRIMARY KEY"
        );
    }

    validate_rowid_table_metadata(conn, schema).await?;

    let mut indexes = operation(
        DbOperation::Select,
        conn.query("PRAGMA index_list(account_usage_snapshots)", ()),
    )
    .await
    .context("inspect host usage rowid primary key")?;
    while let Some(index) = indexes
        .next()
        .await
        .context("read host usage rowid primary-key index")?
    {
        if row_string(&index, 3)? == "pk" {
            anyhow::bail!(
                "host usage cache schema {schema} id must be a rowid-generating INTEGER PRIMARY KEY"
            );
        }
    }
    Ok(())
}

async fn validate_rowid_table_metadata(conn: &Connection, schema: &str) -> Result<()> {
    let mut tables = operation(
        DbOperation::Select,
        conn.query("PRAGMA main.table_list(account_usage_snapshots)", ()),
    )
    .await
    .context("inspect host usage rowid table metadata")?;
    let (table_schema, table_name, table_type, without_rowid) = {
        let Some(table) = tables
            .next()
            .await
            .context("read host usage rowid table metadata")?
        else {
            anyhow::bail!(
                "host usage cache schema {schema} id must be a rowid-generating INTEGER PRIMARY KEY"
            );
        };
        let table_schema = table
            .get::<String>(0)
            .context("read host usage rowid table schema")?;
        let table_name = table
            .get::<String>(1)
            .context("read host usage rowid table name")?;
        let table_type = table
            .get::<String>(2)
            .context("read host usage rowid table type")?;
        let without_rowid = table
            .get::<i64>(4)
            .context("read host usage rowid table flag")?;
        (table_schema, table_name, table_type, without_rowid)
    };
    let has_duplicate = tables
        .next()
        .await
        .context("read duplicate host usage rowid table metadata")?
        .is_some();
    drop(tables);
    if has_duplicate
        || table_schema != "main"
        || table_name != "account_usage_snapshots"
        || table_type != "table"
        || without_rowid != 0
    {
        anyhow::bail!(
            "host usage cache schema {schema} id must be a rowid-generating INTEGER PRIMARY KEY"
        );
    }
    Ok(())
}

async fn validate_unique_identity_key(conn: &Connection) -> Result<()> {
    let expected = ["provider", "account_key_hash", "source", "window_kind"];
    let mut indexes = operation(
        DbOperation::Select,
        conn.query("PRAGMA index_list(account_usage_snapshots)", ()),
    )
    .await
    .context("inspect host usage unique key")?;
    let mut unique_indexes = Vec::new();
    while let Some(index) = indexes
        .next()
        .await
        .context("read host usage unique index")?
    {
        let unique: i64 = index.get(2).context("read host usage index uniqueness")?;
        let partial: i64 = index.get(4).context("read host usage index partial flag")?;
        if unique == 0 || partial != 0 {
            continue;
        }
        unique_indexes.push(row_string(&index, 1)?);
    }
    drop(indexes);
    for name in unique_indexes {
        let escaped_name = name.replace('"', "\"\"");
        let pragma = format!("PRAGMA index_info(\"{escaped_name}\")");
        let mut columns = operation(DbOperation::Select, conn.query(&pragma, ()))
            .await
            .context("inspect host usage unique key columns")?;
        let mut actual = Vec::new();
        while let Some(column) = columns
            .next()
            .await
            .context("read host usage unique key column")?
        {
            actual.push(row_opt_string(&column, 2)?);
        }
        if actual
            .iter()
            .map(Option::as_deref)
            .collect::<Option<Vec<_>>>()
            == Some(expected.to_vec())
        {
            return Ok(());
        }
    }
    anyhow::bail!("host usage cache is missing the required unique account key")
}

async fn account_snapshot_column_info(
    conn: &Connection,
) -> Result<std::collections::HashMap<String, SnapshotColumn>> {
    operation(DbOperation::Select, async {
        let mut rows = conn
            .query("PRAGMA table_info(account_usage_snapshots)", ())
            .await
            .context("inspect host usage cache columns")?;
        let mut columns = std::collections::HashMap::new();
        while let Some(row) = rows.next().await.context("read host usage cache column")? {
            let name = row_string(&row, 1)?;
            let declared_type = row_string(&row, 2)?;
            let not_null: i64 = row.get(3).context("read host usage column nullability")?;
            let primary_key_position: i64 =
                row.get(5).context("read host usage primary-key position")?;
            columns.insert(
                name,
                SnapshotColumn {
                    declared_type,
                    not_null: not_null != 0,
                    primary_key_position,
                },
            );
        }
        Ok(columns)
    })
    .await
}

async fn account_snapshot_columns(conn: &Connection) -> Result<std::collections::HashSet<String>> {
    Ok(account_snapshot_column_info(conn)
        .await?
        .into_keys()
        .collect())
}

async fn read_stored_identities(conn: &Connection) -> Result<Vec<StoredIdentity>> {
    let identities = operation(DbOperation::Select, async {
        let mut rows = conn
            .query(
                "SELECT id, provider, account_key_hash, capability_account_id, capability_surface_id, typeof(provider), typeof(account_key_hash), typeof(capability_account_id), typeof(capability_surface_id) FROM account_usage_snapshots ORDER BY id",
                (),
            )
            .await
            .context("query host usage account identities")?;
        let mut identities = Vec::new();
        while let Some(row) = rows
            .next()
            .await
            .context("read host usage account identity")?
        {
            let id = row_i64(&row, 0)?;
            let provider_type = row_string(&row, 5)?;
            let key_type = row_string(&row, 6)?;
            if provider_type != "text" || key_type != "text" {
                anyhow::bail!(
                    "host usage row {id} provider and account key must be TEXT, got {provider_type}/{key_type}"
                );
            }
            let account_id_type = row_string(&row, 7)?;
            let surface_id_type = row_string(&row, 8)?;
            if !matches!(account_id_type.as_str(), "null" | "text")
                || !matches!(surface_id_type.as_str(), "null" | "text")
            {
                anyhow::bail!(
                    "host usage row {id} capability cells must be NULL or TEXT, got {account_id_type}/{surface_id_type}"
                );
            }
            let provider = row_string(&row, 1)?;
            let account_key_hash = row_string(&row, 2)?;
            let capability = identity::decode_account_capability(
                &provider,
                &account_key_hash,
                row_opt_string(&row, 3)?,
                row_opt_string(&row, 4)?,
            )
            .map_err(|error| anyhow::anyhow!("validate host usage row {id} identity: {error}"))?;
            identities.push(StoredIdentity {
                provider,
                account_key_hash,
                capability,
            });
        }
        Ok::<_, anyhow::Error>(identities)
    })
    .await?;
    identity::validate_identity_groups(identities.iter().map(|row| {
        (
            row.provider.as_str(),
            row.account_key_hash.as_str(),
            row.capability.as_ref(),
        )
    }))
    .map_err(anyhow::Error::msg)?;
    Ok(identities)
}

async fn validate_legacy_identity_rows(conn: &Connection) -> Result<()> {
    let identities = operation(DbOperation::Select, async {
        let mut rows = conn
            .query(
                "SELECT id, provider, account_key_hash, typeof(provider), typeof(account_key_hash) FROM account_usage_snapshots ORDER BY id",
                (),
            )
            .await
            .context("query schema 2 host usage identities")?;
        let mut identities = Vec::new();
        while let Some(row) = rows
            .next()
            .await
            .context("read schema 2 host usage identity")?
        {
            let id = row_i64(&row, 0)?;
            let provider_type = row_string(&row, 3)?;
            let key_type = row_string(&row, 4)?;
            if provider_type != "text" || key_type != "text" {
                anyhow::bail!(
                    "schema 2 host usage row {id} provider and account key must be TEXT, got {provider_type}/{key_type}"
                );
            }
            let provider = row_string(&row, 1)?;
            let account_key_hash = row_string(&row, 2)?;
            let capability = identity::decode_account_capability(
                &provider,
                &account_key_hash,
                None,
                None,
            )
            .map_err(|error| {
                anyhow::anyhow!("validate schema 2 host usage row {id} identity: {error}")
            })?;
            identities.push(StoredIdentity {
                provider,
                account_key_hash,
                capability,
            });
        }
        Ok::<_, anyhow::Error>(identities)
    })
    .await?;
    identity::validate_identity_groups(identities.iter().map(|row| {
        (
            row.provider.as_str(),
            row.account_key_hash.as_str(),
            row.capability.as_ref(),
        )
    }))
    .map_err(anyhow::Error::msg)
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

fn prepare_account_rows(accounts: &[AccountUsageSnapshotView]) -> Result<Vec<PreparedAccount>> {
    let keys = accounts
        .iter()
        .map(|account| {
            identity::account_key_for_view(
                &account.provider,
                &account.account_label,
                account.account_capability.as_ref(),
            )
        })
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(anyhow::Error::msg)?;
    identity::validate_identity_groups(accounts.iter().zip(&keys).map(|(account, key)| {
        (
            account.provider.as_str(),
            key.as_str(),
            account.account_capability.as_ref(),
        )
    }))
    .map_err(anyhow::Error::msg)?;

    accounts
        .iter()
        .zip(keys)
        .map(|(account, account_key_hash)| {
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
            let quota_bucket_json =
                serde_json::to_string(bucket).context("encode host usage quota bucket")?;
            Ok(PreparedAccount {
                account: account.clone(),
                account_key_hash,
                quota_bucket_json,
            })
        })
        .collect()
}

async fn upsert_prepared_account_row(conn: &Connection, row: &PreparedAccount) -> Result<()> {
    let account = &row.account;
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
                quota_bucket_json,
                capability_account_id,
                capability_surface_id
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
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
                quota_bucket_json = excluded.quota_bucket_json,
                capability_account_id = excluded.capability_account_id,
                capability_surface_id = excluded.capability_surface_id
            ",
            params![
                account.provider.clone(),
                row.account_key_hash.clone(),
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
                row.quota_bucket_json.clone(),
                account
                    .account_capability
                    .as_ref()
                    .map(|capability| capability.account_id.clone()),
                account
                    .account_capability
                    .as_ref()
                    .map(|capability| capability.surface_id.clone()),
            ],
        ),
    )
    .await
    .context("upsert host usage account snapshot")?;
    Ok(())
}

async fn upsert_prepared_account_rows(conn: &Connection, rows: &[PreparedAccount]) -> Result<()> {
    for row in rows {
        identity::validate_account_identity(
            &row.account.provider,
            &row.account_key_hash,
            row.account.account_capability.as_ref(),
        )
        .map_err(anyhow::Error::msg)?;
        let bucket = row.account.quota_bucket.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "host usage account {:?} is missing its typed quota bucket",
                row.account.window_kind
            )
        })?;
        bucket.validate().map_err(|error| {
            anyhow::anyhow!(
                "validate host usage quota bucket {:?}: {error}",
                row.account.window_kind
            )
        })?;
    }
    identity::validate_identity_groups(rows.iter().map(|row| {
        (
            row.account.provider.as_str(),
            row.account_key_hash.as_str(),
            row.account.account_capability.as_ref(),
        )
    }))
    .map_err(anyhow::Error::msg)?;

    operation(DbOperation::Begin, conn.execute("BEGIN IMMEDIATE", ()))
        .await
        .context("begin host usage account snapshot upsert")?;
    let upsert = async {
        let existing = read_stored_identities(conn).await?;
        identity::validate_identity_groups(
            existing
                .iter()
                .map(|row| {
                    (
                        row.provider.as_str(),
                        row.account_key_hash.as_str(),
                        row.capability.as_ref(),
                    )
                })
                .chain(rows.iter().map(|row| {
                    (
                        row.account.provider.as_str(),
                        row.account_key_hash.as_str(),
                        row.account.account_capability.as_ref(),
                    )
                })),
        )
        .map_err(anyhow::Error::msg)?;
        for row in rows {
            upsert_prepared_account_row(conn, row).await?;
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
                quota_bucket_json,
                account_key_hash,
                capability_account_id,
                capability_surface_id
            FROM account_usage_snapshots
            ORDER BY provider, account_label, source, window_kind
            ",
                (),
            )
            .await
            .context("query host usage account snapshots")?;
        let mut accounts = Vec::new();
        let mut identities = Vec::new();
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
            let provider = row_string(&row, 0)?;
            let account_key_hash = row_string(&row, 15)?;
            let account_capability = identity::decode_account_capability(
                &provider,
                &account_key_hash,
                row_opt_string(&row, 16)?,
                row_opt_string(&row, 17)?,
            )
            .map_err(|error| anyhow::anyhow!("validate host usage identity: {error}"))?;
            identities.push(StoredIdentity {
                provider: provider.clone(),
                account_key_hash,
                capability: account_capability.clone(),
            });
            accounts.push(AccountUsageSnapshotView {
                provider,
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
                account_capability,
            });
        }
        identity::validate_identity_groups(identities.iter().map(|identity| {
            (
                identity.provider.as_str(),
                identity.account_key_hash.as_str(),
                identity.capability.as_ref(),
            )
        }))
        .map_err(anyhow::Error::msg)?;
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
