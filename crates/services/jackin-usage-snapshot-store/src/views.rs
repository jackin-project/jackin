// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Account usage views.

use std::collections::HashMap;

use std::path::Path;

use jackin_usage_store_backend::{self, DbOperation};

use jackin_protocol::control::{
    FocusedAccountHeader, FocusedUsageView, QuotaBucketView, UsageConfidence, UsageSnapshotStatus,
    UsageSource,
};

use super::{
    AccountIdentitySummary, StoredAccountUsageSnapshot, block_on_store, open_store, row_i64,
    row_opt_i64, row_opt_string, row_string,
};

/// One durable account reconstructed from a single, source-consistent snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredAccountUsageView {
    /// Historical wire key written with the snapshot rows.
    pub account_key_hash: String,
    /// Reconstructed latest view. All buckets come from one selected source.
    pub view: FocusedUsageView,
}

/// List distinct accounts in the durable store (multi-account Desktop / host).
pub fn list_account_identities(path: &Path) -> Result<Vec<AccountIdentitySummary>, String> {
    let rows = load_all_account_snapshot_rows(path)?;
    let mut by_key: HashMap<String, AccountIdentitySummary> = HashMap::new();
    for row in rows {
        let entry = by_key
            .entry(row.account_key_hash.clone())
            .or_insert_with(|| AccountIdentitySummary {
                provider: row.provider.clone(),
                account_key_hash: row.account_key_hash.clone(),
                account_label: row.account_label.clone(),
                plan_label: row.plan_label.clone(),
                remaining_percent: None,
                fetched_at: row.fetched_at,
            });
        if row.fetched_at >= entry.fetched_at {
            entry.fetched_at = row.fetched_at;
            entry.account_label = row.account_label.clone();
            entry.plan_label = row.plan_label.clone();
            entry.provider = row.provider.clone();
        }
        if let Some(rem) = row
            .remaining_percent
            .and_then(|v| u8::try_from(v.clamp(0, 100)).ok())
        {
            entry.remaining_percent = Some(match entry.remaining_percent {
                Some(prev) => prev.min(rem),
                None => rem,
            });
        }
    }
    let mut out: Vec<_> = by_key.into_values().collect();
    out.sort_by(|a, b| {
        a.provider
            .cmp(&b.provider)
            .then(a.account_label.cmp(&b.account_label))
    });
    Ok(out)
}

/// Reconstruct a focused usage view for one account key from the durable store.
pub fn load_account_usage_view(
    path: &Path,
    account_key_hash: &str,
    now_epoch: i64,
) -> Result<Option<FocusedUsageView>, String> {
    Ok(load_all_account_usage_views(path, now_epoch)?
        .into_iter()
        .find(|stored| stored.account_key_hash == account_key_hash)
        .map(|stored| stored.view))
}

/// Reconstruct every durable account with one database scan.
///
/// Rows first pin the newest fetch generation, then one evidence source. This
/// prevents a same-timestamp provider API/CLI/local-log mix from duplicating
/// buckets or pairing one source's header with another source's limits.
pub fn load_all_account_usage_views(
    path: &Path,
    now_epoch: i64,
) -> Result<Vec<StoredAccountUsageView>, String> {
    let mut grouped: HashMap<String, Vec<StoredAccountUsageSnapshot>> = HashMap::new();
    for row in load_all_account_snapshot_rows(path)? {
        grouped
            .entry(row.account_key_hash.clone())
            .or_default()
            .push(row);
    }
    let mut views = Vec::with_capacity(grouped.len());
    for (account_key_hash, rows) in grouped {
        if let Some(view) = account_usage_view_from_rows(rows, now_epoch) {
            views.push(StoredAccountUsageView {
                account_key_hash,
                view,
            });
        }
    }
    views.sort_by(|a, b| {
        a.view
            .account
            .provider_label
            .cmp(&b.view.account.provider_label)
            .then(
                a.view
                    .account
                    .account_label
                    .cmp(&b.view.account.account_label),
            )
    });
    Ok(views)
}

pub(crate) fn account_usage_view_from_rows(
    mut matches: Vec<StoredAccountUsageSnapshot>,
    now_epoch: i64,
) -> Option<FocusedUsageView> {
    if matches.is_empty() {
        return None;
    }
    let latest = matches.iter().map(|r| r.fetched_at).max().unwrap_or(0);
    matches.retain(|r| r.fetched_at == latest);
    let selected_source = matches
        .iter()
        .max_by_key(|row| durable_source_priority(&row.source))?
        .source
        .clone();
    matches.retain(|row| row.source == selected_source);
    matches.sort_by(|a, b| a.window_kind.cmp(&b.window_kind));
    let first = matches.first()?;
    let provider = first.provider.clone();
    let status = usage_status_from_storage(&first.view_status);
    let source = usage_source_from_storage(&first.source);
    let confidence = usage_confidence_from_storage(&first.confidence);
    let mut buckets: Vec<QuotaBucketView> = matches
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
            status_slot: None,
            pace_label: row.pace_label.clone(),
            status: usage_status_from_storage(&row.status),
        })
        .collect();
    // Stable window order: session/weekly first when present.
    buckets.sort_by(|a, b| a.label.cmp(&b.label));
    buckets.dedup_by(|a, b| a.label == b.label);
    Some(FocusedUsageView {
        focused_agent: None,
        focused_provider: first.focused_provider.clone().or(Some(provider.clone())),
        account: FocusedAccountHeader {
            provider_label: provider,
            account_label: first.account_label.clone(),
            username: None,
            plan_label: first.plan_label.clone(),
            credential_origin: None,
        },
        buckets,
        status,
        source,
        confidence,
        fetched_at_epoch: latest,
        updated_label: if first.updated_label.trim().is_empty() {
            jackin_usage_provider_core::relative_updated_label(latest, now_epoch)
        } else {
            first.updated_label.clone()
        },
        status_bar_label: if first.status_bar_label.trim().is_empty() {
            "usage cached".to_owned()
        } else {
            first.status_bar_label.clone()
        },
        tabs: Vec::new(),
        last_error: first.last_error.clone(),
    })
}

pub(crate) fn durable_source_priority(source: &str) -> u8 {
    match source {
        "provider_api" => 4,
        "cli" => 3,
        "local_logs" => 2,
        "cache" => 1,
        _ => 0,
    }
}

pub(crate) fn usage_status_from_storage(label: &str) -> UsageSnapshotStatus {
    match label {
        "fresh" => UsageSnapshotStatus::Fresh,
        "stale" => UsageSnapshotStatus::Stale,
        "needs_login" => UsageSnapshotStatus::NeedsLogin,
        "needs_secret" => UsageSnapshotStatus::NeedsSecret,
        "unsupported" => UsageSnapshotStatus::Unsupported,
        "error" => UsageSnapshotStatus::Error,
        _ => UsageSnapshotStatus::Unavailable,
    }
}

pub(crate) fn usage_source_from_storage(label: &str) -> UsageSource {
    match label {
        "provider_api" => UsageSource::ProviderApi,
        "cli" => UsageSource::Cli,
        "local_logs" => UsageSource::LocalLogs,
        "cache" => UsageSource::Cache,
        _ => UsageSource::None,
    }
}

pub(crate) fn usage_confidence_from_storage(label: &str) -> UsageConfidence {
    match label {
        "authoritative" => UsageConfidence::Authoritative,
        "estimated" => UsageConfidence::Estimated,
        "presence_only" => UsageConfidence::PresenceOnly,
        _ => UsageConfidence::None,
    }
}

pub(crate) fn load_all_account_snapshot_rows(
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
