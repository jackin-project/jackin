// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;

use jackin_protocol::control::{
    CanonicalRemainingPercent, PercentObservationOrigin, QuotaBucketView, UsageSeverity,
};
use jackin_usage_store_backend::{
    self, CommittedMigrationValidation, Connection, DbOperation, LocalStore, ManagedStoreKind,
};

use super::{SCHEMA_VERSION, row_i64, row_opt_i64, row_opt_string, row_string};

pub(crate) async fn initialize_schema(store: &LocalStore) -> Result<(), String> {
    let conn = store.connection();
    jackin_usage_store_backend::operation(
        DbOperation::Update,
        conn.execute_batch("PRAGMA foreign_keys = ON"),
    )
    .await
    .map_err(|err| format!("enable usage snapshot store foreign keys failed: {err}"))?;

    let stored_version = stored_schema_version(conn).await?;
    validate_existing_schema_shape(conn, stored_version.as_deref()).await?;
    if finish_if_current_schema(store, stored_version.as_deref()).await? {
        return Ok(());
    }
    migrate_schema_transaction(store).await
}

async fn finish_if_current_schema(
    store: &LocalStore,
    stored_version: Option<&str>,
) -> Result<bool, String> {
    let Some(version) = stored_version else {
        return Ok(false);
    };
    let parsed = version
        .parse::<u32>()
        .map_err(|_| format!("unsupported usage snapshot store schema version {version:?}"))?;
    if parsed > SCHEMA_VERSION.parse::<u32>().unwrap_or_default() {
        return Err(format!(
            "usage snapshot store schema {parsed} is newer than supported schema {SCHEMA_VERSION}"
        ));
    }
    if parsed != SCHEMA_VERSION.parse::<u32>().unwrap_or_default() {
        return Ok(false);
    }
    let validation = validate_committed_schema(store).await?;
    store
        .mark_current_schema_migration_committed(
            ManagedStoreKind::QuotaSnapshot,
            SCHEMA_VERSION.parse::<u64>().unwrap_or_default(),
            validation,
        )
        .await
        .map_err(|error| error.to_string())?;
    Ok(true)
}

async fn migrate_schema_transaction(store: &LocalStore) -> Result<(), String> {
    let conn = store.connection();
    jackin_usage_store_backend::operation(DbOperation::Begin, conn.execute("BEGIN IMMEDIATE", ()))
        .await
        .map_err(|err| format!("begin usage snapshot schema migration failed: {err}"))?;

    let migration = async {
        let current_version = stored_schema_version(conn).await?;
        validate_existing_schema_shape(conn, current_version.as_deref()).await?;
        if let Some(version) = current_version.as_deref() {
            let parsed = version.parse::<u32>().map_err(|_| {
                format!("unsupported usage snapshot store schema version {version:?}")
            })?;
            if parsed > SCHEMA_VERSION.parse::<u32>().unwrap_or_default() {
                return Err(format!(
                    "usage snapshot store schema {parsed} is newer than supported schema {SCHEMA_VERSION}"
                ));
            }
            if parsed == SCHEMA_VERSION.parse::<u32>().unwrap_or_default() {
                validate_current_schema(conn).await?;
                // Another process may have completed the migration after our
                // first version read. Leave receipt recovery until after this
                // read-only writer transaction commits.
                return Ok(None);
            }
        }

        let old_version = current_version
            .as_deref()
            .map(str::parse::<u64>)
            .transpose()
            .map_err(|_| "usage snapshot schema version is not numeric".to_owned())?;
        let backup_receipt = store
            .backup_before_migration(
                ManagedStoreKind::QuotaSnapshot,
                old_version,
                SCHEMA_VERSION.parse::<u64>().unwrap_or_default(),
            )
            .await
            .map_err(|error| error.to_string())?;

        create_snapshot_tables(conn).await?;

        ensure_account_snapshot_columns(conn).await?;
        ensure_quota_bucket_column(conn).await?;
        migrate_legacy_bucket_payloads(conn).await?;

        let old_version = current_version.unwrap_or_else(|| "unversioned".to_owned());
        jackin_usage_store_backend::operation(
            DbOperation::Upsert,
            conn.execute(
                "INSERT OR IGNORE INTO _meta (key, value) VALUES ('schema_version_before_quota_v5', ?1)",
                [old_version],
            ),
        )
        .await
        .map_err(|err| format!("preserve previous usage snapshot schema version failed: {err}"))?;
        jackin_usage_store_backend::operation(
            DbOperation::Upsert,
            conn.execute(
                "INSERT INTO _meta (key, value) VALUES ('schema_version', ?1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                [SCHEMA_VERSION],
            ),
        )
        .await
        .map_err(|err| format!("record usage snapshot schema version failed: {err}"))?;

        validate_current_schema(conn).await?;
        #[cfg(test)]
        inject_requested_commit_failure(conn).await?;
        Ok(backup_receipt)
    }
    .await;

    match migration {
        Ok(backup_receipt) => {
            match jackin_usage_store_backend::operation(
                DbOperation::Commit,
                conn.execute("COMMIT", ()),
            )
            .await
            {
                Ok(_) => {
                    if let Some(receipt) = backup_receipt {
                        let validation = validate_committed_schema(store).await?;
                        store
                            .mark_migration_committed(&receipt, validation)
                            .await
                            .map_err(|error| error.to_string())?;
                    } else {
                        let validation = validate_committed_schema(store).await?;
                        store
                            .mark_current_schema_migration_committed(
                                ManagedStoreKind::QuotaSnapshot,
                                SCHEMA_VERSION.parse::<u64>().unwrap_or_default(),
                                validation,
                            )
                            .await
                            .map_err(|error| error.to_string())?;
                    }
                    Ok(())
                }
                Err(commit_error) => {
                    let rollback = jackin_usage_store_backend::operation(
                        DbOperation::Rollback,
                        conn.execute("ROLLBACK", ()),
                    )
                    .await;
                    match rollback {
                        Ok(_) => Err(format!(
                            "commit usage snapshot schema migration failed: {commit_error}"
                        )),
                        Err(rollback_error) => Err(format!(
                            "commit usage snapshot schema migration failed: {commit_error}; rollback failed: {rollback_error}"
                        )),
                    }
                }
            }
        }
        Err(error) => {
            let rollback = jackin_usage_store_backend::operation(
                DbOperation::Rollback,
                conn.execute("ROLLBACK", ()),
            )
            .await;
            match rollback {
                Ok(_) => Err(error),
                Err(rollback_error) => Err(format!("{error}; rollback failed: {rollback_error}")),
            }
        }
    }
}

async fn create_snapshot_tables(conn: &Connection) -> Result<(), String> {
    jackin_usage_store_backend::operation(
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
                quota_bucket_json TEXT NOT NULL,
                UNIQUE(provider, account_key_hash, source, window_kind)
            );
            ",
        ),
    )
    .await
    .map_err(|err| format!("initialize usage snapshot schema failed: {err}"))
}

async fn validate_committed_schema(
    store: &LocalStore,
) -> Result<CommittedMigrationValidation, String> {
    validate_current_schema(store.connection()).await?;
    let validation = store
        .validate_committed_migration(
            ManagedStoreKind::QuotaSnapshot,
            SCHEMA_VERSION.parse::<u64>().unwrap_or_default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    validate_current_schema(validation.connection()).await?;
    Ok(validation)
}

async fn validate_existing_schema_shape(
    conn: &Connection,
    version: Option<&str>,
) -> Result<(), String> {
    let has_meta = table_exists(conn, "_meta").await?;
    let has_snapshots = table_exists(conn, "account_usage_snapshots").await?;
    if !has_snapshots {
        if version.is_some() || has_meta {
            return Err(
                "versioned or partial usage snapshot store is missing account_usage_snapshots"
                    .to_owned(),
            );
        }
        return Ok(());
    }

    let columns = snapshot_columns(conn).await?;
    let supports_legacy_migration = match version {
        None => true,
        Some(version) => version
            .parse::<u32>()
            .is_ok_and(|version| version < SCHEMA_VERSION.parse::<u32>().unwrap_or_default()),
    };
    if supports_legacy_migration && columns.contains("quota_bucket_json") {
        return Err("pre-v5 usage snapshot store contains a partial typed quota column".to_owned());
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
            return Err(format!(
                "usage snapshot store is missing required legacy column {required}"
            ));
        }
    }

    if version.and_then(|value| value.parse::<u32>().ok()) == Some(4) {
        for required in [
            "focused_provider",
            "plan_label",
            "remaining_percent",
            "used_label",
            "limit_label",
            "reset_label",
            "pace_label",
            "view_status",
            "updated_label",
            "status_bar_label",
        ] {
            if !columns.contains(required) {
                return Err(format!(
                    "usage snapshot schema 4 is missing required column {required}"
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
async fn inject_requested_commit_failure(conn: &Connection) -> Result<(), String> {
    if !table_exists(conn, "_meta").await? {
        return Ok(());
    }
    let mut rows = jackin_usage_store_backend::operation(
        DbOperation::Select,
        conn.query(
            "SELECT value FROM _meta WHERE key = 'test_fail_schema_commit'",
            (),
        ),
    )
    .await
    .map_err(|err| format!("read schema commit failure test marker failed: {err}"))?;
    let marker = rows
        .next()
        .await
        .map_err(|err| format!("read schema commit failure test marker row failed: {err}"))?
        .map(|row| row_string(&row, 0, "test_fail_schema_commit"))
        .transpose()?;
    drop(rows);
    if marker.as_deref() == Some("1") {
        jackin_usage_store_backend::operation(
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
        .map_err(|err| format!("prepare schema commit failure test marker failed: {err}"))?;
    }
    Ok(())
}

async fn ensure_account_snapshot_columns(conn: &Connection) -> Result<(), String> {
    let mut rows = jackin_usage_store_backend::operation(
        DbOperation::Select,
        conn.query("PRAGMA table_info(account_usage_snapshots)", ()),
    )
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
            jackin_usage_store_backend::operation(DbOperation::Update, conn.execute(ddl, ()))
                .await
                .map_err(|err| format!("upgrade telemetry snapshot column {name} failed: {err}"))?;
        }
    }
    Ok(())
}

async fn ensure_quota_bucket_column(conn: &Connection) -> Result<(), String> {
    let mut rows = jackin_usage_store_backend::operation(
        DbOperation::Select,
        conn.query("PRAGMA table_info(account_usage_snapshots)", ()),
    )
    .await
    .map_err(|err| format!("inspect typed quota snapshot schema failed: {err}"))?;
    let mut has_payload = false;
    while let Some(row) = rows
        .next()
        .await
        .map_err(|err| format!("read typed quota snapshot schema failed: {err}"))?
    {
        has_payload |= row_string(&row, 1, "column_name")? == "quota_bucket_json";
    }
    if !has_payload {
        jackin_usage_store_backend::operation(
            DbOperation::Update,
            conn.execute(
                "ALTER TABLE account_usage_snapshots ADD COLUMN quota_bucket_json TEXT",
                (),
            ),
        )
        .await
        .map_err(|err| format!("add typed quota bucket persistence column failed: {err}"))?;
    }
    Ok(())
}

async fn migrate_legacy_bucket_payloads(conn: &Connection) -> Result<(), String> {
    let mut rows = jackin_usage_store_backend::operation(
        DbOperation::Select,
        conn.query(
            "SELECT id, window_kind, used_label, limit_label, remaining_percent, reset_label,
                    resets_at, pace_label, status, quota_bucket_json
             FROM account_usage_snapshots
             ORDER BY id",
            (),
        ),
    )
    .await
    .map_err(|err| format!("read legacy usage snapshots for typed migration failed: {err}"))?;
    let mut payloads = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|err| format!("read legacy usage snapshot row failed: {err}"))?
    {
        let id = row_i64(&row, 0, "id")?;
        let existing = row_opt_string(&row, 9, "quota_bucket_json")?;
        if let Some(existing) = existing {
            serde_json::from_str::<QuotaBucketView>(&existing).map_err(|err| {
                format!("decode existing quota_bucket_json for snapshot row {id} failed: {err}")
            })?;
            continue;
        }
        let remaining_percent = row_opt_i64(&row, 4, "remaining_percent")?
            .map(legacy_remaining_percent)
            .transpose()
            .map_err(|err| format!("migrate snapshot row {id} remaining_percent failed: {err}"))?;
        let remaining_percent_origin = remaining_percent
            .as_ref()
            .map(|_| PercentObservationOrigin::HistoricalUnknown);
        let status_label = row_string(&row, 8, "status")?;
        let status = legacy_status(&status_label)
            .map_err(|err| format!("migrate snapshot row {id} status failed: {err}"))?;
        let bucket = QuotaBucketView {
            used_money: None,
            limit_money: None,
            severity: UsageSeverity::default(),
            label: row_string(&row, 1, "window_kind")?,
            used_label: row_opt_string(&row, 2, "used_label")?,
            limit_label: row_opt_string(&row, 3, "limit_label")?,
            used_percent: None,
            used_percent_origin: None,
            remaining_percent,
            remaining_percent_origin,
            count_quota: None,
            provider_quota_state: None,
            reset_label: row_opt_string(&row, 5, "reset_label")?,
            resets_at: row_opt_i64(&row, 6, "resets_at")?,
            status_slot: None,
            pace_label: row_opt_string(&row, 7, "pace_label")?,
            status,
        };
        let encoded = serde_json::to_string(&bucket)
            .map_err(|err| format!("encode migrated quota bucket for row {id} failed: {err}"))?;
        payloads.push((id, encoded));
    }
    for (id, encoded) in payloads {
        jackin_usage_store_backend::operation(
            DbOperation::Update,
            conn.execute(
                "UPDATE account_usage_snapshots SET quota_bucket_json = ?1
                 WHERE id = ?2 AND quota_bucket_json IS NULL",
                (encoded, id),
            ),
        )
        .await
        .map_err(|err| {
            format!("store migrated quota bucket for snapshot row {id} failed: {err}")
        })?;
    }
    Ok(())
}

fn legacy_remaining_percent(value: i64) -> Result<CanonicalRemainingPercent, String> {
    let historical_value = u8::try_from(value).map_err(|_| {
        format!("legacy integer {value} is outside the historical u8 percentage domain")
    })?;
    CanonicalRemainingPercent::new(f64::from(historical_value))
}

async fn validate_current_schema(conn: &Connection) -> Result<(), String> {
    let columns = snapshot_columns(conn).await?;
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
        "focused_provider",
        "plan_label",
        "remaining_percent",
        "used_label",
        "limit_label",
        "reset_label",
        "pace_label",
        "view_status",
        "updated_label",
        "status_bar_label",
        "quota_bucket_json",
    ] {
        if !columns.contains(required) {
            return Err(format!("schema 5 is missing {required}"));
        }
    }
    let mut rows = jackin_usage_store_backend::operation(
        DbOperation::Select,
        conn.query(
            "SELECT id, quota_bucket_json FROM account_usage_snapshots ORDER BY id",
            (),
        ),
    )
    .await
    .map_err(|err| format!("validate typed usage snapshot rows failed: {err}"))?;
    while let Some(row) = rows
        .next()
        .await
        .map_err(|err| format!("read typed usage snapshot rows failed: {err}"))?
    {
        let id = row_i64(&row, 0, "id")?;
        let encoded = row_opt_string(&row, 1, "quota_bucket_json")?
            .ok_or_else(|| format!("snapshot row {id} is missing quota_bucket_json"))?;
        let bucket = serde_json::from_str::<QuotaBucketView>(&encoded).map_err(|err| {
            format!("decode quota_bucket_json for snapshot row {id} failed: {err}")
        })?;
        bucket.validate().map_err(|err| {
            format!("validate quota_bucket_json for snapshot row {id} failed: {err}")
        })?;
    }
    Ok(())
}

fn legacy_status(label: &str) -> Result<jackin_protocol::control::UsageSnapshotStatus, String> {
    use jackin_protocol::control::UsageSnapshotStatus as Status;
    match label {
        "fresh" => Ok(Status::Fresh),
        "stale" => Ok(Status::Stale),
        "needs_login" => Ok(Status::NeedsLogin),
        "needs_secret" => Ok(Status::NeedsSecret),
        "unsupported" => Ok(Status::Unsupported),
        "error" => Ok(Status::Error),
        "unavailable" => Ok(Status::Unavailable),
        other => Err(format!("unknown legacy status {other:?}")),
    }
}

async fn snapshot_columns(conn: &Connection) -> Result<HashSet<String>, String> {
    let mut rows = jackin_usage_store_backend::operation(
        DbOperation::Select,
        conn.query("PRAGMA table_info(account_usage_snapshots)", ()),
    )
    .await
    .map_err(|err| format!("inspect usage snapshot columns failed: {err}"))?;
    let mut columns = HashSet::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|err| format!("read usage snapshot columns failed: {err}"))?
    {
        columns.insert(row_string(&row, 1, "column_name")?);
    }
    Ok(columns)
}

async fn stored_schema_version(conn: &Connection) -> Result<Option<String>, String> {
    if !table_exists(conn, "_meta").await? {
        return Ok(None);
    }
    let mut rows = jackin_usage_store_backend::operation(
        DbOperation::Select,
        conn.query("SELECT value FROM _meta WHERE key = 'schema_version'", ()),
    )
    .await
    .map_err(|err| format!("read usage snapshot schema version failed: {err}"))?;
    rows.next()
        .await
        .map_err(|err| format!("read usage snapshot schema version row failed: {err}"))?
        .map(|row| row_string(&row, 0, "schema_version"))
        .transpose()
}

async fn table_exists(conn: &Connection, table: &str) -> Result<bool, String> {
    let mut rows = jackin_usage_store_backend::operation(
        DbOperation::Select,
        conn.query(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table],
        ),
    )
    .await
    .map_err(|err| format!("inspect usage snapshot tables failed: {err}"))?;
    Ok(rows
        .next()
        .await
        .map_err(|err| format!("read usage snapshot table row failed: {err}"))?
        .is_some())
}

#[cfg(test)]
mod tests;
