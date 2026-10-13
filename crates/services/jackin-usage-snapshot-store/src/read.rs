// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Snapshot reads and row decoding.

#[cfg(test)]
use std::path::Path;

use jackin_usage_store_backend::Row;
#[cfg(test)]
use jackin_usage_store_backend::{self, DbOperation};

#[cfg(test)]
use jackin_protocol::control::{
    FocusedAccountHeader, FocusedUsageView, QuotaBucketView, UsageSnapshotStatus,
};

#[cfg(test)]
use super::*;

#[cfg(test)]
pub fn focused_usage_view(
    path: &Path,
    focused_agent: Option<&str>,
    focused_provider: Option<&str>,
    now_epoch: i64,
) -> Result<Option<FocusedUsageView>, String> {
    let rows = stored_account_snapshots(path)?;
    let tabs = usage_provider_tabs_from_rows(&rows);
    let resolved_provider = focused_provider.or_else(|| {
        focused_agent.and_then(|agent| {
            jackin_usage_provider_core::resolved_usage_provider_label(agent, None)
        })
    });
    let Some((provider, rows)) = select_provider_rows(rows, resolved_provider) else {
        return Ok(None);
    };
    let Some(first) = rows.first() else {
        return Ok(None);
    };
    let status = usage_status_from_label(&first.view_status);
    let source = usage_source_from_label(&first.source);
    let confidence = usage_confidence_from_label(&first.confidence);
    let fetched_at = rows.iter().map(|row| row.fetched_at).max().unwrap_or(0);
    let mut buckets = rows
        .iter()
        .map(|row| QuotaBucketView {
            used_money: None,
            limit_money: None,
            severity: jackin_protocol::control::UsageSeverity::default(),
            label: row.window_kind.clone(),
            used_label: row.used_label.clone(),
            limit_label: row.limit_label.clone(),
            remaining_percent: row
                .remaining_percent
                .and_then(|value| u8::try_from(value.clamp(0, 100)).ok()),
            reset_label: row.reset_label.clone(),
            resets_at: row.resets_at,
            // The headline is persisted as `status_bar_label`, so the slot tag is
            // not stored; a store-restored bucket carries none and the live
            // refresh re-tags it.
            status_slot: None,
            pace_label: row.pace_label.clone(),
            status: usage_status_from_label(&row.status),
        })
        .collect::<Vec<_>>();
    buckets.sort_by_key(|bucket| usage_bucket_order(&provider, &bucket.label));
    Ok(Some(FocusedUsageView {
        focused_agent: focused_agent.map(str::to_owned),
        focused_provider: first
            .focused_provider
            .clone()
            .or_else(|| resolved_provider.map(str::to_owned))
            .or_else(|| Some(provider.clone())),
        account: FocusedAccountHeader {
            provider_label: provider,
            account_label: first.account_label.clone(),
            // username + credential_origin are live-snapshot fields, not
            // persisted in the store yet; a store-restored header gets them
            // on the next refresh.
            username: None,
            plan_label: first.plan_label.clone(),
            credential_origin: None,
        },
        buckets,
        status,
        source,
        confidence,
        fetched_at_epoch: fetched_at,
        updated_label: if matches!(
            status,
            UsageSnapshotStatus::Fresh | UsageSnapshotStatus::Stale
        ) || first.updated_label.trim().is_empty()
        {
            jackin_usage_provider_core::relative_updated_label(fetched_at, now_epoch)
        } else {
            first.updated_label.clone()
        },
        status_bar_label: if first.status_bar_label.trim().is_empty() {
            lifecycle_status_bar_label(status)
        } else {
            first.status_bar_label.clone()
        },
        tabs,
        last_error: first.last_error.clone(),
    }))
}

#[cfg(test)]
pub(crate) fn stored_account_snapshots(
    path: &Path,
) -> Result<Vec<StoredAccountUsageSnapshot>, String> {
    let path = path.to_path_buf();
    block_on_store(async move {
        let conn = open_store(&path).await?;
        let mut rows = jackin_usage_store_backend::operation(
            DbOperation::Select,
            conn.query(
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
                FROM account_usage_snapshots
                ORDER BY provider, account_key_hash, source, window_kind
                ",
                (),
            ),
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
                focused_provider: row_opt_string(&row, 15, "focused_provider")?,
                plan_label: row_opt_string(&row, 16, "plan_label")?,
                remaining_percent: row_opt_i64(&row, 17, "remaining_percent")?,
                used_label: row_opt_string(&row, 18, "used_label")?,
                limit_label: row_opt_string(&row, 19, "limit_label")?,
                reset_label: row_opt_string(&row, 20, "reset_label")?,
                pace_label: row_opt_string(&row, 21, "pace_label")?,
                view_status: row_string(&row, 22, "view_status")?,
                updated_label: row_string(&row, 23, "updated_label")?,
                status_bar_label: row_string(&row, 24, "status_bar_label")?,
            });
        }
        Ok(snapshots)
    })
}

#[cfg(test)]
pub fn schema_version(path: &Path) -> Result<Option<String>, String> {
    let path = path.to_path_buf();
    block_on_store(async move {
        let conn = open_store(&path).await?;
        let mut rows = jackin_usage_store_backend::operation(
            DbOperation::Select,
            conn.query("SELECT value FROM _meta WHERE key = 'schema_version'", ()),
        )
        .await
        .map_err(|err| format!("query telemetry schema version failed: {err}"))?;
        rows.next()
            .await
            .map_err(|err| format!("read telemetry schema version failed: {err}"))?
            .map(|row| row_string(&row, 0, "schema_version"))
            .transpose()
    })
}

pub(crate) fn row_i64(row: &Row, idx: usize, name: &str) -> Result<i64, String> {
    row.get(idx)
        .map_err(|err| format!("decode telemetry {name} failed: {err}"))
}

pub(crate) fn row_opt_i64(row: &Row, idx: usize, name: &str) -> Result<Option<i64>, String> {
    row.get(idx)
        .map_err(|err| format!("decode telemetry {name} failed: {err}"))
}

pub(crate) fn row_string(row: &Row, idx: usize, name: &str) -> Result<String, String> {
    row.get(idx)
        .map_err(|err| format!("decode telemetry {name} failed: {err}"))
}

pub(crate) fn row_opt_string(row: &Row, idx: usize, name: &str) -> Result<Option<String>, String> {
    row.get(idx)
        .map_err(|err| format!("decode telemetry {name} failed: {err}"))
}
