// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Row upsert and quota mapping.

use super::StoredAccountUsageSnapshot;
use jackin_core::account_key_hash;
use jackin_protocol::control::{FocusedUsageView, QuotaBucketView};
use jackin_telemetry::ResultTelemetryExt as _;
use jackin_usage_store_backend::{self, Connection, DbOperation, params};

pub(crate) async fn upsert_account_snapshot_rows(
    conn: &Connection,
    rows: Vec<StoredAccountUsageSnapshot>,
) -> Result<(), String> {
    jackin_diagnostics::incr_db_statement("begin");
    jackin_usage_store_backend::operation(DbOperation::Begin, conn.execute("BEGIN", ()))
        .await
        .map_err(|err| format!("begin telemetry snapshot transaction failed: {err}"))?;
    for row in rows {
        jackin_diagnostics::incr_db_statement("upsert_account_usage_snapshot");
        if let Err(err) = jackin_usage_store_backend::operation(
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
                remaining_percent,
                used_label,
                limit_label,
                reset_label,
                pace_label,
                view_status,
                updated_label,
                status_bar_label
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25)
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
                last_error = excluded.last_error,
                focused_provider = excluded.focused_provider,
                plan_label = excluded.plan_label,
                remaining_percent = excluded.remaining_percent,
                used_label = excluded.used_label,
                limit_label = excluded.limit_label,
                reset_label = excluded.reset_label,
                pace_label = excluded.pace_label,
                view_status = excluded.view_status,
                updated_label = excluded.updated_label,
                status_bar_label = excluded.status_bar_label
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
                row.remaining_percent,
                row.used_label,
                row.limit_label,
                row.reset_label,
                row.pace_label,
                row.view_status,
                row.updated_label,
                row.status_bar_label,
            ],
            ),
        )
        .await
        {
            // Roll the whole batch back so a mid-batch failure never leaves a
            // partially-written snapshot set; surface the original row error.
            let _rollback =
                jackin_usage_store_backend::operation(DbOperation::Rollback, conn.execute("ROLLBACK", ()))
                    .await
                    .record_telemetry_error(jackin_telemetry::schema::enums::ErrorType::DbError);
            return Err(format!("upsert telemetry account snapshot failed: {err}"));
        }
    }
    jackin_usage_store_backend::operation(DbOperation::Commit, conn.execute("COMMIT", ()))
        .await
        .map_err(|err| format!("commit telemetry snapshot transaction failed: {err}"))?;
    Ok(())
}

pub(crate) fn account_snapshot_rows(view: &FocusedUsageView) -> Vec<StoredAccountUsageSnapshot> {
    let provider = view.account.provider_label.clone();
    let account_label = view.account.account_label.clone();
    let account_key_hash = account_key_hash(&provider, &account_label);
    let source = jackin_usage_provider_core::usage_source_storage_label(view.source).to_owned();
    let confidence =
        jackin_usage_provider_core::usage_confidence_storage_label(view.confidence).to_owned();
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
                resets_at: bucket.resets_at,
                fetched_at,
                expires_at: None,
                status: jackin_usage_provider_core::usage_status_storage_label(bucket.status)
                    .to_owned(),
                last_error: last_error.clone(),
                focused_provider: view.focused_provider.clone(),
                plan_label: view.account.plan_label.clone(),
                remaining_percent: bucket.remaining_percent.map(i64::from),
                used_label: bucket.used_label.clone(),
                limit_label: bucket.limit_label.clone(),
                reset_label: bucket.reset_label.clone(),
                pace_label: bucket.pace_label.clone(),
                view_status: jackin_usage_provider_core::usage_status_storage_label(view.status)
                    .to_owned(),
                updated_label: view.updated_label.clone(),
                status_bar_label: view.status_bar_label.clone(),
            }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct QuotaAmounts {
    used_amount: Option<i64>,
    used_unit: Option<String>,
    limit_amount: Option<i64>,
    limit_unit: Option<String>,
}

pub(crate) fn quota_amounts(bucket: &QuotaBucketView) -> QuotaAmounts {
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
