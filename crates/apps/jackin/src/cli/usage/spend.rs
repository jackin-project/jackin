// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use jackin_protocol::control::Money;
use jackin_protocol::usage_monitor::{SpendRecordInput, SpendRecordSource};
use serde::Deserialize;
use std::io::Read as _;

use super::usage_error;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SpendEvidenceFile {
    billing_period_start_epoch: i64,
    billing_period_end_epoch: i64,
    amount: Money,
    evidence_at_epoch: Option<i64>,
}

#[expect(
    clippy::disallowed_methods,
    reason = "bounded synchronous CLI input read outside render/runtime threads"
)]
pub(super) fn read_spend_record(
    path: &std::path::Path,
    account: &str,
    verified: bool,
) -> Result<SpendRecordInput> {
    const MAX_SPEND_FILE_BYTES: usize = 16 * 1024;
    let file = std::fs::File::open(path)
        .with_context(|| format!("open spend evidence file {}", path.display()))
        .map_err(|error| usage_error("spend_file_invalid", &format!("{error:#}"), 3))?;
    let mut bytes = Vec::with_capacity(MAX_SPEND_FILE_BYTES + 1);
    file.take((MAX_SPEND_FILE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| usage_error("spend_file_invalid", &error.to_string(), 3))?;
    if bytes.len() > MAX_SPEND_FILE_BYTES {
        return Err(usage_error(
            "spend_file_too_large",
            "spend evidence file exceeds 16 KiB",
            3,
        ));
    }
    let evidence: SpendEvidenceFile = serde_json::from_slice(&bytes)
        .map_err(|error| usage_error("spend_file_invalid", &error.to_string(), 3))?;
    if account.trim().is_empty()
        || evidence.billing_period_start_epoch >= evidence.billing_period_end_epoch
        || evidence.amount.amount_minor < 0
        || evidence.amount.currency.trim().is_empty()
    {
        return Err(usage_error(
            "spend_file_invalid",
            "account, billing period, and nonnegative monetary amount must be valid",
            3,
        ));
    }
    Ok(SpendRecordInput {
        account_id: account.to_owned(),
        billing_period_start_epoch: evidence.billing_period_start_epoch,
        billing_period_end_epoch: evidence.billing_period_end_epoch,
        amount: evidence.amount,
        evidence_at_epoch: evidence.evidence_at_epoch,
        verified,
        source: SpendRecordSource::OperatorReceipt,
    })
}

pub(super) fn parse_sgd_budget(value: &str) -> Result<Money> {
    let mut parts = value.split('.');
    let major = parts.next().unwrap_or_default();
    let minor = parts.next();
    if parts.next().is_some()
        || major.is_empty()
        || !major.bytes().all(|byte| byte.is_ascii_digit())
        || minor
            .is_some_and(|part| part.len() > 2 || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Err(usage_error(
            "invalid_budget",
            "SGD budget must be a positive amount with at most two decimal places",
            3,
        ));
    }
    let major = major
        .parse::<i64>()
        .map_err(|_| usage_error("invalid_budget", "SGD budget is out of range", 3))?;
    let minor = minor.unwrap_or_default();
    let minor = if minor.is_empty() {
        0
    } else if minor.len() == 1 {
        minor.parse::<i64>().unwrap_or(0) * 10
    } else {
        minor.parse::<i64>().unwrap_or(0)
    };
    let amount_minor = major
        .checked_mul(100)
        .and_then(|amount| amount.checked_add(minor))
        .ok_or_else(|| usage_error("invalid_budget", "SGD budget is out of range", 3))?;
    if amount_minor == 0 {
        return Err(usage_error(
            "invalid_budget",
            "SGD budget must be a positive amount with at most two decimal places",
            3,
        ));
    }
    Ok(Money::new(amount_minor, "SGD", 2))
}

pub(super) fn parse_sgd_budget_arg(value: &str) -> std::result::Result<String, String> {
    parse_sgd_budget(value)
        .map(|_| value.to_owned())
        .map_err(|_| {
            "SGD budget must be a positive amount with at most two decimal places".to_owned()
        })
}
