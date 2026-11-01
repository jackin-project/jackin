// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::StoredAccountUsageSnapshot;
use super::identity;
use jackin_protocol::control::QuotaBucketView;
use jackin_usage_store_backend::Row;

pub(crate) fn decode_quota_bucket(
    encoded: Option<String>,
    window_kind: &str,
) -> Result<QuotaBucketView, String> {
    let encoded = encoded.ok_or_else(|| {
        format!("telemetry snapshot {window_kind:?} is missing quota_bucket_json")
    })?;
    let bucket: QuotaBucketView = serde_json::from_str(&encoded).map_err(|err| {
        format!("decode telemetry quota_bucket_json for {window_kind:?} failed: {err}")
    })?;
    bucket.validate().map_err(|err| {
        format!("validate telemetry quota_bucket_json for {window_kind:?} failed: {err}")
    })?;
    Ok(bucket)
}

pub(super) fn decode_stored_account_snapshot(
    row: &Row,
) -> Result<StoredAccountUsageSnapshot, String> {
    let provider = row_string(row, 0, "provider")?;
    let (account_key_hash, account_capability) = decode_stored_account_identity(row, &provider)?;
    Ok(StoredAccountUsageSnapshot {
        provider,
        account_key_hash,
        account_capability,
        account_label: row_string(row, 2, "account_label")?,
        source: row_string(row, 3, "source")?,
        confidence: row_string(row, 4, "confidence")?,
        window_kind: row_string(row, 5, "window_kind")?,
        used_amount: row_opt_i64(row, 6, "used_amount")?,
        used_unit: row_opt_string(row, 7, "used_unit")?,
        limit_amount: row_opt_i64(row, 8, "limit_amount")?,
        limit_unit: row_opt_string(row, 9, "limit_unit")?,
        resets_at: row_opt_i64(row, 10, "resets_at")?,
        fetched_at: row_i64(row, 11, "fetched_at")?,
        expires_at: row_opt_i64(row, 12, "expires_at")?,
        status: row_string(row, 13, "status")?,
        last_error: row_opt_string(row, 14, "last_error")?,
        focused_provider: row_opt_string(row, 15, "focused_provider")?,
        plan_label: row_opt_string(row, 16, "plan_label")?,
        quota_bucket: decode_quota_bucket(
            row_opt_string(row, 17, "quota_bucket_json")?,
            &row_string(row, 5, "window_kind")?,
        )?,
        used_label: row_opt_string(row, 18, "used_label")?,
        limit_label: row_opt_string(row, 19, "limit_label")?,
        reset_label: row_opt_string(row, 20, "reset_label")?,
        pace_label: row_opt_string(row, 21, "pace_label")?,
        view_status: row_string(row, 22, "view_status")?,
        updated_label: row_string(row, 23, "updated_label")?,
        status_bar_label: row_string(row, 24, "status_bar_label")?,
    })
}

fn decode_stored_account_identity(
    row: &Row,
    provider: &str,
) -> Result<
    (
        String,
        Option<jackin_protocol::usage_broker::UsageAccountCapability>,
    ),
    String,
> {
    let account_key_hash = row_string(row, 1, "account_key_hash")?;
    let account_capability = identity::decode_account_capability(
        provider,
        &account_key_hash,
        row_opt_string(row, 25, "capability_account_id")?,
        row_opt_string(row, 26, "capability_surface_id")?,
    )?;
    Ok((account_key_hash, account_capability))
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
