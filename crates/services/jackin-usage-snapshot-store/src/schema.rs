// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Schema initialization and migration.

use std::collections::HashSet;

use jackin_usage_store_backend::{self, Connection, DbOperation};

use super::{SCHEMA_VERSION, row_string};

pub(crate) async fn initialize_schema(conn: &Connection) -> Result<(), String> {
    jackin_usage_store_backend::operation(
        DbOperation::Update,
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
        ),
    )
    .await
    .map_err(|err| format!("initialize usage snapshot store schema failed: {err}"))?;
    ensure_account_snapshot_columns(conn).await?;
    jackin_usage_store_backend::operation(
        DbOperation::Upsert,
        conn.execute(
            "INSERT INTO _meta (key, value) VALUES ('schema_version', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [SCHEMA_VERSION],
        ),
    )
    .await
    .map_err(|err| format!("record usage snapshot store schema version failed: {err}"))?;
    Ok(())
}

pub(crate) async fn ensure_account_snapshot_columns(conn: &Connection) -> Result<(), String> {
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
                .map_err(|err| format!("upgrade telemetry snapshot schema failed: {err}"))?;
        }
    }
    Ok(())
}
