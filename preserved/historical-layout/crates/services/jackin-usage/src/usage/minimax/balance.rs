// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `MiniMax` PAYG balance parsing.

use super::super::{QuotaBucketView, UsageSnapshotStatus, bucket};
use super::{MiniMaxBaseResponse, MiniMaxRegion};
use serde::Deserialize;

/// PAYG balance (`GET {base}/account/query_balance`, `sk-api-*` keys only).
/// Amounts are decimal strings; a balance is always shown with its region
/// currency, never bare.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct MiniMaxBalanceResponse {
    #[serde(rename = "base_resp")]
    pub(crate) base_resp: Option<MiniMaxBaseResponse>,
    #[serde(rename = "available_amount")]
    pub(crate) available_amount: Option<String>,
    #[serde(rename = "cash_balance")]
    pub(crate) cash_balance: Option<String>,
    #[serde(rename = "voucher_balance")]
    pub(crate) voucher_balance: Option<String>,
    #[serde(rename = "credit_balance")]
    pub(crate) credit_balance: Option<String>,
    #[serde(rename = "owed_amount")]
    pub(crate) owed_amount: Option<String>,
}

impl MiniMaxBalanceResponse {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if let Some(status) = self.base_resp.as_ref().and_then(|base| base.status_code)
            && status != 0
        {
            return Err(self
                .base_resp
                .as_ref()
                .and_then(|base| base.status_msg.clone())
                .unwrap_or_else(|| format!("status_code {status}")));
        }
        if self.available_amount.is_none()
            && self.cash_balance.is_none()
            && self.voucher_balance.is_none()
            && self.credit_balance.is_none()
            && self.owed_amount.is_none()
        {
            return Err("missing MiniMax balance data".to_owned());
        }
        Ok(())
    }

    pub(crate) fn buckets(&self, region: MiniMaxRegion) -> Vec<QuotaBucketView> {
        let currency = region.currency();
        let mut parts = vec![currency.to_owned()];
        for (label, amount) in [
            ("cash", self.cash_balance.as_deref()),
            ("voucher", self.voucher_balance.as_deref()),
            ("credit", self.credit_balance.as_deref()),
            ("owed", self.owed_amount.as_deref()),
        ] {
            if let Some(amount) = amount.map(str::trim).filter(|value| !value.is_empty()) {
                parts.push(format!("{label} {amount}"));
            }
        }
        // A balance is funds, not used-of-limit: amounts ride in labels only,
        // never as mislabeled `Money` spend.
        vec![bucket(
            "Balance",
            self.available_amount
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned),
            None,
            None,
            None,
            Some(&parts.join(" · ")),
            UsageSnapshotStatus::Fresh,
        )]
    }
}

/// Parse a decimal balance string to minor units (exponent 2) without float
/// rounding: `12.5` → `1250`, `-3.456` → `-345` (truncated, never rounded
/// up across the owed boundary).
pub(crate) fn minimax_decimal_minor(text: &str) -> Option<i64> {
    let text = text.trim();
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let (major, minor) = match digits.split_once('.') {
        Some((major, minor)) => (major, minor),
        None => (digits, ""),
    };
    if major.is_empty() || !major.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let mut minor_digits: String = minor
        .bytes()
        .take_while(u8::is_ascii_digit)
        .map(|byte| byte as char)
        .collect();
    if minor_digits.len() != minor.len() {
        return None;
    }
    while minor_digits.len() < 2 {
        minor_digits.push('0');
    }
    minor_digits.truncate(2);
    let major_value: i64 = major.parse().ok()?;
    let minor_value: i64 = minor_digits.parse().ok()?;
    let total = major_value.checked_mul(100)?.checked_add(minor_value)?;
    Some(if negative { -total } else { total })
}
