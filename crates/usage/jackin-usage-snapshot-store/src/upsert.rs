// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::StoredAccountUsageSnapshot;
use super::identity::{account_key_for_view, validate_identity_groups};
use jackin_protocol::control::FocusedUsageView;
use jackin_telemetry::ResultTelemetryExt as _;
use jackin_usage_store_backend::{self, Connection, DbOperation, params};

pub(crate) async fn upsert_account_snapshot_rows(
    conn: &Connection,
    rows: Vec<StoredAccountUsageSnapshot>,
) -> Result<(), String> {
    let rows = rows
        .into_iter()
        .map(|row| {
            super::identity::validate_account_identity(
                &row.provider,
                &row.account_key_hash,
                row.account_capability.as_ref(),
            )?;
            row.quota_bucket.validate().map_err(|err| {
                format!("validate typed quota bucket for snapshot row failed: {err}")
            })?;
            let bucket_json = serde_json::to_string(&row.quota_bucket).map_err(|err| {
                format!("encode typed quota bucket for snapshot row failed: {err}")
            })?;
            Ok((row, bucket_json))
        })
        .collect::<Result<Vec<_>, String>>()?;
    validate_identity_groups(rows.iter().map(|(row, _)| {
        (
            row.provider.as_str(),
            row.account_key_hash.as_str(),
            row.account_capability.as_ref(),
        )
    }))?;
    jackin_diagnostics::incr_db_statement("begin");
    jackin_usage_store_backend::operation(DbOperation::Begin, conn.execute("BEGIN IMMEDIATE", ()))
        .await
        .map_err(|err| format!("begin telemetry snapshot transaction failed: {err}"))?;
    if let Err(error) = validate_existing_identity_assignments(conn, &rows).await {
        let rollback = jackin_usage_store_backend::operation(
            DbOperation::Rollback,
            conn.execute("ROLLBACK", ()),
        )
        .await;
        return match rollback {
            Ok(_) => Err(error),
            Err(rollback_error) => Err(format!("{error}; rollback failed: {rollback_error}")),
        };
    }
    for (row, bucket_json) in rows {
        if let Err(err) = execute_account_snapshot_row(conn, row, bucket_json).await {
            let _rollback = jackin_usage_store_backend::operation(
                DbOperation::Rollback,
                conn.execute("ROLLBACK", ()),
            )
            .await
            .record_telemetry_error(jackin_telemetry::schema::enums::ErrorType::DbError);
            return Err(format!("upsert telemetry account snapshot failed: {err}"));
        }
    }
    match jackin_usage_store_backend::operation(DbOperation::Commit, conn.execute("COMMIT", ()))
        .await
    {
        Ok(_) => {}
        Err(commit_error) => {
            let rollback = jackin_usage_store_backend::operation(
                DbOperation::Rollback,
                conn.execute("ROLLBACK", ()),
            )
            .await;
            return match rollback {
                Ok(_) => Err(format!(
                    "commit telemetry snapshot transaction failed: {commit_error}"
                )),
                Err(rollback_error) => Err(format!(
                    "commit telemetry snapshot transaction failed: {commit_error}; rollback failed: {rollback_error}"
                )),
            };
        }
    }
    Ok(())
}

async fn execute_account_snapshot_row(
    conn: &Connection,
    row: StoredAccountUsageSnapshot,
    bucket_json: String,
) -> Result<u64, impl std::fmt::Display> {
    jackin_diagnostics::incr_db_statement("upsert_account_usage_snapshot");
    jackin_usage_store_backend::operation(
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
                focused_provider,
                plan_label,
                capability_account_id,
                capability_surface_id,
                used_label,
                limit_label,
                reset_label,
                pace_label,
                view_status,
                updated_label,
                status_bar_label,
                quota_bucket_json
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27)
            ON CONFLICT(provider, account_key_hash, source, window_kind) DO UPDATE SET
                account_label = excluded.account_label,
                confidence = excluded.confidence,
                resets_at = excluded.resets_at,
                fetched_at = excluded.fetched_at,
                expires_at = excluded.expires_at,
                status = excluded.status,
                last_error = excluded.last_error,
                focused_provider = excluded.focused_provider,
                plan_label = excluded.plan_label,
                capability_account_id = excluded.capability_account_id,
                capability_surface_id = excluded.capability_surface_id,
                used_label = excluded.used_label,
                limit_label = excluded.limit_label,
                reset_label = excluded.reset_label,
                pace_label = excluded.pace_label,
                view_status = excluded.view_status,
                updated_label = excluded.updated_label,
                status_bar_label = excluded.status_bar_label,
                quota_bucket_json = excluded.quota_bucket_json
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
                row.focused_provider,
                row.plan_label,
                row.account_capability.as_ref().map(|capability| capability.account_id.as_str()),
                row.account_capability.as_ref().map(|capability| capability.surface_id.as_str()),
                row.used_label,
                row.limit_label,
                row.reset_label,
                row.pace_label,
                row.view_status,
                row.updated_label,
                row.status_bar_label,
                bucket_json,
            ],
        ),
    )
    .await
}

async fn validate_existing_identity_assignments(
    conn: &Connection,
    rows: &[(StoredAccountUsageSnapshot, String)],
) -> Result<(), String> {
    let mut existing = Vec::new();
    let account_keys = rows
        .iter()
        .map(|(row, _)| row.account_key_hash.clone())
        .collect::<std::collections::BTreeSet<_>>();
    for account_key_hash in account_keys {
        let mut stored = jackin_usage_store_backend::operation(
            DbOperation::Select,
            conn.query(
                "SELECT provider, account_key_hash, capability_account_id,
                        capability_surface_id
                 FROM account_usage_snapshots WHERE account_key_hash = ?1",
                [account_key_hash.as_str()],
            ),
        )
        .await
        .map_err(|error| format!("read snapshot identity before upsert failed: {error}"))?;
        while let Some(row) = stored
            .next()
            .await
            .map_err(|error| format!("read snapshot identity row before upsert failed: {error}"))?
        {
            let provider = super::row_string(&row, 0, "provider")?;
            let account_key_hash = super::row_string(&row, 1, "account_key_hash")?;
            let capability = super::identity::decode_account_capability(
                &provider,
                &account_key_hash,
                super::row_opt_string(&row, 2, "capability_account_id")?,
                super::row_opt_string(&row, 3, "capability_surface_id")?,
            )?;
            existing.push((provider, account_key_hash, capability));
        }
    }
    let assignments = existing
        .iter()
        .map(|(provider, account_key_hash, capability)| {
            (
                provider.as_str(),
                account_key_hash.as_str(),
                capability.as_ref(),
            )
        })
        .chain(rows.iter().map(|(row, _)| {
            (
                row.provider.as_str(),
                row.account_key_hash.as_str(),
                row.account_capability.as_ref(),
            )
        }));
    validate_identity_groups(assignments)
}

pub(crate) fn account_snapshot_rows(
    view: &FocusedUsageView,
) -> Result<Vec<StoredAccountUsageSnapshot>, String> {
    let provider = view.account.provider_label.clone();
    let account_label = view.account.account_label.clone();
    let account_key_hash = account_key_for_view(
        &provider,
        &account_label,
        view.account.account_capability.as_ref(),
    )?;
    let account_capability = view.account.account_capability.clone();
    let source = jackin_usage_provider_core::usage_source_storage_label(view.source).to_owned();
    let confidence =
        jackin_usage_provider_core::usage_confidence_storage_label(view.confidence).to_owned();
    let fetched_at = view.fetched_at_epoch;
    let last_error = view.last_error.clone();
    Ok(view
        .buckets
        .iter()
        .map(|bucket| StoredAccountUsageSnapshot {
            provider: provider.clone(),
            account_key_hash: account_key_hash.clone(),
            account_capability: account_capability.clone(),
            account_label: account_label.clone(),
            source: source.clone(),
            confidence: confidence.clone(),
            window_kind: bucket.label.clone(),
            used_amount: None,
            used_unit: None,
            limit_amount: None,
            limit_unit: None,
            resets_at: bucket.resets_at,
            fetched_at,
            expires_at: None,
            status: jackin_usage_provider_core::usage_status_storage_label(bucket.status)
                .to_owned(),
            last_error: last_error.clone(),
            focused_provider: view.focused_provider.clone(),
            plan_label: view.account.plan_label.clone(),
            quota_bucket: bucket.clone(),
            used_label: bucket.used_label.clone(),
            limit_label: bucket.limit_label.clone(),
            reset_label: bucket.reset_label.clone(),
            pace_label: bucket.pace_label.clone(),
            view_status: jackin_usage_provider_core::usage_status_storage_label(view.status)
                .to_owned(),
            updated_label: view.updated_label.clone(),
            status_bar_label: view.status_bar_label.clone(),
        })
        .collect())
}
