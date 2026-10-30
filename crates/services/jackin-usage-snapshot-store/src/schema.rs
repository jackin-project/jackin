// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, HashSet};

use jackin_protocol::control::{
    CanonicalRemainingPercent, PercentObservationOrigin, QuotaBucketView, UsageSeverity,
};
use jackin_usage_store_backend::{
    self, CommittedMigrationValidation, Connection, DbOperation, LocalStore, ManagedStoreKind,
};

use super::identity::{decode_account_capability, validate_identity_groups};
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

    let migration = prepare_schema_migration(store, conn).await;

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

async fn prepare_schema_migration(
    store: &LocalStore,
    conn: &Connection,
) -> Result<Option<jackin_usage_store_backend::BackupReceipt>, String> {
    let current_version = stored_schema_version(conn).await?;
    validate_existing_schema_shape(conn, current_version.as_deref()).await?;
    if let Some(version) = current_version.as_deref() {
        let parsed = version
            .parse::<u32>()
            .map_err(|_| format!("unsupported usage snapshot store schema version {version:?}"))?;
        if parsed > SCHEMA_VERSION.parse::<u32>().unwrap_or_default() {
            return Err(format!(
                "usage snapshot store schema {parsed} is newer than supported schema {SCHEMA_VERSION}"
            ));
        }
        if parsed == SCHEMA_VERSION.parse::<u32>().unwrap_or_default() {
            validate_current_schema(conn).await?;
            return Ok(None);
        }
    }

    let old_version_number = current_version
        .as_deref()
        .map(str::parse::<u64>)
        .transpose()
        .map_err(|_| "usage snapshot schema version is not numeric".to_owned())?;
    let backup_receipt = store
        .backup_before_migration(
            ManagedStoreKind::QuotaSnapshot,
            old_version_number,
            SCHEMA_VERSION.parse::<u64>().unwrap_or_default(),
        )
        .await
        .map_err(|error| error.to_string())?;

    create_snapshot_tables(conn).await?;
    ensure_account_snapshot_columns(conn).await?;
    ensure_quota_bucket_column(conn).await?;
    if old_version_number != Some(5) {
        migrate_legacy_bucket_payloads(conn).await?;
    }
    ensure_identity_columns(conn).await?;

    let old_version_text = current_version.as_deref().unwrap_or("unversioned");
    if old_version_number.is_none_or(|version| version < 5) {
        jackin_usage_store_backend::operation(
            DbOperation::Upsert,
            conn.execute(
                "INSERT OR IGNORE INTO _meta (key, value) VALUES ('schema_version_before_quota_v5', ?1)",
                [old_version_text],
            ),
        )
        .await
        .map_err(|err| format!("preserve previous usage snapshot schema version failed: {err}"))?;
    }
    jackin_usage_store_backend::operation(
        DbOperation::Upsert,
        conn.execute(
            "INSERT OR IGNORE INTO _meta (key, value)
             VALUES ('schema_version_before_capability_v6', ?1)",
            [old_version_text],
        ),
    )
    .await
    .map_err(|err| format!("preserve previous capability schema version failed: {err}"))?;
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
                capability_account_id TEXT,
                capability_surface_id TEXT,
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

    let parsed_version = version
        .map(|version| {
            version
                .parse::<u32>()
                .map_err(|_| format!("unsupported usage snapshot store schema version {version:?}"))
        })
        .transpose()?;
    let Some(parsed_version) = parsed_version else {
        return validate_pre_quota_schema(conn, None).await;
    };
    if parsed_version > SCHEMA_VERSION.parse::<u32>().unwrap_or_default() {
        return Ok(());
    }
    match parsed_version {
        0..=4 => validate_pre_quota_schema(conn, Some(parsed_version)).await,
        5 => validate_schema_v5(conn).await,
        6 => validate_current_schema(conn).await,
        _ => Err(format!(
            "unsupported usage snapshot store schema version {parsed_version}"
        )),
    }
}

async fn validate_pre_quota_schema(conn: &Connection, version: Option<u32>) -> Result<(), String> {
    let columns = snapshot_columns(conn).await?;
    if columns.contains("quota_bucket_json") {
        return Err("pre-v5 usage snapshot store contains a partial typed quota column".to_owned());
    }
    if columns.contains("capability_account_id") || columns.contains("capability_surface_id") {
        return Err(
            "pre-v6 usage snapshot store contains a partial account capability identity".to_owned(),
        );
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

    if version == Some(4) {
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
    if !has_snapshot_unique_key(conn).await? {
        return Err(
            "legacy usage snapshot store is missing UNIQUE(provider, account_key_hash, source, window_kind)"
                .to_owned(),
        );
    }
    Ok(())
}

async fn validate_schema_v5(conn: &Connection) -> Result<(), String> {
    let descriptions = snapshot_column_descriptions(conn).await?;
    let required_types = [
        ("id", "INTEGER"),
        ("provider", "TEXT"),
        ("account_key_hash", "TEXT"),
        ("account_label", "TEXT"),
        ("source", "TEXT"),
        ("confidence", "TEXT"),
        ("window_kind", "TEXT"),
        ("used_amount", "INTEGER"),
        ("used_unit", "TEXT"),
        ("limit_amount", "INTEGER"),
        ("limit_unit", "TEXT"),
        ("resets_at", "INTEGER"),
        ("fetched_at", "INTEGER"),
        ("expires_at", "INTEGER"),
        ("status", "TEXT"),
        ("last_error", "TEXT"),
        ("focused_provider", "TEXT"),
        ("plan_label", "TEXT"),
        ("remaining_percent", "INTEGER"),
        ("used_label", "TEXT"),
        ("limit_label", "TEXT"),
        ("reset_label", "TEXT"),
        ("pace_label", "TEXT"),
        ("view_status", "TEXT"),
        ("updated_label", "TEXT"),
        ("status_bar_label", "TEXT"),
        ("quota_bucket_json", "TEXT"),
    ];
    for (name, expected_type) in required_types {
        let Some((actual_type, _, _)) = descriptions.get(name) else {
            return Err(format!("schema 5 is missing {name}"));
        };
        if !actual_type.eq_ignore_ascii_case(expected_type) {
            return Err(format!(
                "schema 5 column {name} has type {actual_type:?}, expected {expected_type}"
            ));
        }
    }
    if !descriptions
        .get("id")
        .is_some_and(|(_, _, primary_key)| *primary_key)
    {
        return Err("schema 5 id column must be the primary key".to_owned());
    }
    if descriptions.contains_key("capability_account_id")
        || descriptions.contains_key("capability_surface_id")
    {
        return Err("schema 5 contains unsupported capability identity columns".to_owned());
    }
    for required_not_null in [
        "provider",
        "account_key_hash",
        "account_label",
        "source",
        "confidence",
        "window_kind",
        "fetched_at",
        "status",
        "view_status",
        "updated_label",
        "status_bar_label",
    ] {
        let Some((_, not_null, _)) = descriptions.get(required_not_null) else {
            return Err(format!("schema 5 is missing {required_not_null}"));
        };
        if !*not_null {
            return Err(format!(
                "schema 5 column {required_not_null} must be NOT NULL"
            ));
        }
    }
    if !has_snapshot_unique_key(conn).await? {
        return Err(
            "schema 5 is missing UNIQUE(provider, account_key_hash, source, window_kind)"
                .to_owned(),
        );
    }
    validate_typed_quota_payloads(conn).await?;
    validate_unbound_snapshot_keys(conn).await?;
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

async fn ensure_identity_columns(conn: &Connection) -> Result<(), String> {
    let columns = snapshot_columns(conn).await?;
    let has_account_id = columns.contains("capability_account_id");
    let has_surface_id = columns.contains("capability_surface_id");
    match (has_account_id, has_surface_id) {
        (false, false) => {
            for (name, ddl) in [
                (
                    "capability_account_id",
                    "ALTER TABLE account_usage_snapshots ADD COLUMN capability_account_id TEXT",
                ),
                (
                    "capability_surface_id",
                    "ALTER TABLE account_usage_snapshots ADD COLUMN capability_surface_id TEXT",
                ),
            ] {
                jackin_usage_store_backend::operation(DbOperation::Update, conn.execute(ddl, ()))
                    .await
                    .map_err(|error| {
                        format!("add snapshot account capability column {name} failed: {error}")
                    })?;
            }
        }
        (true, true) => {}
        _ => {
            return Err(
                "usage snapshot store has a partial account capability identity pair".to_owned(),
            );
        }
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
    let descriptions = snapshot_column_descriptions(conn).await?;
    for (required, expected_type) in [
        ("id", "INTEGER"),
        ("provider", "TEXT"),
        ("account_key_hash", "TEXT"),
        ("account_label", "TEXT"),
        ("source", "TEXT"),
        ("confidence", "TEXT"),
        ("window_kind", "TEXT"),
        ("used_amount", "INTEGER"),
        ("used_unit", "TEXT"),
        ("limit_amount", "INTEGER"),
        ("limit_unit", "TEXT"),
        ("resets_at", "INTEGER"),
        ("fetched_at", "INTEGER"),
        ("expires_at", "INTEGER"),
        ("status", "TEXT"),
        ("last_error", "TEXT"),
        ("focused_provider", "TEXT"),
        ("plan_label", "TEXT"),
        ("remaining_percent", "INTEGER"),
        ("used_label", "TEXT"),
        ("limit_label", "TEXT"),
        ("reset_label", "TEXT"),
        ("pace_label", "TEXT"),
        ("view_status", "TEXT"),
        ("updated_label", "TEXT"),
        ("status_bar_label", "TEXT"),
        ("quota_bucket_json", "TEXT"),
        ("capability_account_id", "TEXT"),
        ("capability_surface_id", "TEXT"),
    ] {
        let Some((actual_type, _, _)) = descriptions.get(required) else {
            return Err(format!("schema 6 is missing {required}"));
        };
        if !actual_type.eq_ignore_ascii_case(expected_type) {
            return Err(format!(
                "schema 6 column {required} has type {actual_type:?}, expected {expected_type}"
            ));
        }
    }
    if !descriptions
        .get("id")
        .is_some_and(|(_, _, primary_key)| *primary_key)
    {
        return Err("schema 6 id column must be the primary key".to_owned());
    }
    for identity_column in ["capability_account_id", "capability_surface_id"] {
        let Some((_, not_null, _)) = descriptions.get(identity_column) else {
            return Err(format!("schema 6 is missing {identity_column}"));
        };
        if *not_null {
            return Err(format!(
                "schema 6 column {identity_column} must remain nullable for legacy rows"
            ));
        }
    }
    for required_not_null in [
        "provider",
        "account_key_hash",
        "account_label",
        "source",
        "confidence",
        "window_kind",
        "fetched_at",
        "status",
        "view_status",
        "updated_label",
        "status_bar_label",
    ] {
        let Some((_, not_null, _)) = descriptions.get(required_not_null) else {
            return Err(format!("schema 6 is missing {required_not_null}"));
        };
        if !*not_null {
            return Err(format!(
                "schema 6 column {required_not_null} must be NOT NULL"
            ));
        }
    }
    if !has_snapshot_unique_key(conn).await? {
        return Err(
            "schema 6 is missing UNIQUE(provider, account_key_hash, source, window_kind)"
                .to_owned(),
        );
    }
    validate_typed_quota_payloads(conn).await?;

    let mut rows = jackin_usage_store_backend::operation(
        DbOperation::Select,
        conn.query(
            "SELECT provider, account_key_hash, capability_account_id, capability_surface_id
             FROM account_usage_snapshots ORDER BY id",
            (),
        ),
    )
    .await
    .map_err(|err| format!("validate snapshot account identities failed: {err}"))?;
    let mut identities = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|err| format!("read snapshot account identity row failed: {err}"))?
    {
        let provider = row_string(&row, 0, "provider")?;
        let account_key_hash = row_string(&row, 1, "account_key_hash")?;
        let account_capability = decode_account_capability(
            &provider,
            &account_key_hash,
            row_opt_string(&row, 2, "capability_account_id")?,
            row_opt_string(&row, 3, "capability_surface_id")?,
        )?;
        identities.push((provider, account_key_hash, account_capability));
    }
    validate_identity_groups(identities.iter().map(
        |(provider, account_key_hash, account_capability)| {
            (
                provider.as_str(),
                account_key_hash.as_str(),
                account_capability.as_ref(),
            )
        },
    ))?;
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

async fn snapshot_column_descriptions(
    conn: &Connection,
) -> Result<HashMap<String, (String, bool, bool)>, String> {
    let mut rows = jackin_usage_store_backend::operation(
        DbOperation::Select,
        conn.query("PRAGMA table_info(account_usage_snapshots)", ()),
    )
    .await
    .map_err(|err| format!("inspect usage snapshot column declarations failed: {err}"))?;
    let mut columns = HashMap::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|err| format!("read usage snapshot column declarations failed: {err}"))?
    {
        let name = row_string(&row, 1, "column_name")?;
        let data_type = row_string(&row, 2, "column_type")?;
        let not_null = row_i64(&row, 3, "column_not_null")? != 0;
        let primary_key = row_i64(&row, 5, "column_primary_key")? != 0;
        columns.insert(name, (data_type, not_null, primary_key));
    }
    Ok(columns)
}

async fn has_snapshot_unique_key(conn: &Connection) -> Result<bool, String> {
    let mut indexes = jackin_usage_store_backend::operation(
        DbOperation::Select,
        conn.query("PRAGMA index_list(account_usage_snapshots)", ()),
    )
    .await
    .map_err(|err| format!("inspect usage snapshot indexes failed: {err}"))?;
    let mut unique_names = Vec::new();
    while let Some(row) = indexes
        .next()
        .await
        .map_err(|err| format!("read usage snapshot index declaration failed: {err}"))?
    {
        if row_i64(&row, 2, "index_unique")? != 0 && row_i64(&row, 4, "index_partial")? == 0 {
            unique_names.push(row_string(&row, 1, "index_name")?);
        }
    }
    drop(indexes);

    let expected = &["provider", "account_key_hash", "source", "window_kind"];
    for name in unique_names {
        let quoted_name = name.replace('"', "\"\"");
        let mut columns = jackin_usage_store_backend::operation(
            DbOperation::Select,
            conn.query(&format!("PRAGMA index_info(\"{quoted_name}\")"), ()),
        )
        .await
        .map_err(|err| format!("inspect usage snapshot unique index {name:?} failed: {err}"))?;
        let mut indexed_columns = Vec::new();
        let mut has_only_named_columns = true;
        while let Some(row) = columns
            .next()
            .await
            .map_err(|err| format!("read usage snapshot unique index {name:?} failed: {err}"))?
        {
            let position = row_i64(&row, 0, "index_column_position")?;
            let Some(column) = row_opt_string(&row, 2, "index_column_name")? else {
                has_only_named_columns = false;
                break;
            };
            indexed_columns.push((position, column));
        }
        drop(columns);
        if !has_only_named_columns {
            continue;
        }
        indexed_columns.sort_by_key(|(position, _)| *position);
        if indexed_columns
            .into_iter()
            .map(|(_, column)| column)
            .eq(expected.iter().map(|column| (*column).to_owned()))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

async fn validate_typed_quota_payloads(conn: &Connection) -> Result<(), String> {
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
        .map_err(|err| format!("read typed usage snapshot row failed: {err}"))?
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

async fn validate_unbound_snapshot_keys(conn: &Connection) -> Result<(), String> {
    let mut rows = jackin_usage_store_backend::operation(
        DbOperation::Select,
        conn.query(
            "SELECT provider, account_key_hash FROM account_usage_snapshots
             ORDER BY provider, account_key_hash",
            (),
        ),
    )
    .await
    .map_err(|err| format!("validate legacy snapshot account keys failed: {err}"))?;
    let mut identities = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|err| format!("read legacy snapshot account key failed: {err}"))?
    {
        let provider = row_string(&row, 0, "provider")?;
        let account_key_hash = row_string(&row, 1, "account_key_hash")?;
        let capability = decode_account_capability(&provider, &account_key_hash, None, None)?;
        identities.push((provider, account_key_hash, capability));
    }
    validate_identity_groups(identities.iter().map(
        |(provider, account_key_hash, account_capability)| {
            (
                provider.as_str(),
                account_key_hash.as_str(),
                account_capability.as_ref(),
            )
        },
    ))
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
