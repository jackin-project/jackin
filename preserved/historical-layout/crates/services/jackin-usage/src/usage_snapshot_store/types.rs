// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Snapshot row types and schema version.

#[cfg(test)]
use std::collections::HashMap;

#[cfg(test)]
use std::sync::{Mutex, OnceLock};

pub(crate) const SCHEMA_VERSION: &str = "4";

#[cfg(test)]
pub(crate) static CONNECTION_BUILDS: OnceLock<Mutex<HashMap<String, usize>>> = OnceLock::new();

/// Distinct account identity known to the durable snapshot store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountIdentitySummary {
    /// Provider label as stored (`Anthropic / Claude`, `OpenAI / Codex`, …).
    pub provider: String,
    /// `account_key_hash` (stable multi-account id).
    pub account_key_hash: String,
    /// Operator-visible account label.
    pub account_label: String,
    /// Plan when last stored.
    pub plan_label: Option<String>,
    /// Tightest remaining % among latest windows for this account.
    pub remaining_percent: Option<u8>,
    /// Latest `fetched_at` epoch among rows for this account.
    pub fetched_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredAccountUsageSnapshot {
    pub provider: String,
    pub account_key_hash: String,
    pub account_label: String,
    pub source: String,
    pub confidence: String,
    pub window_kind: String,
    pub used_amount: Option<i64>,
    pub used_unit: Option<String>,
    pub limit_amount: Option<i64>,
    pub limit_unit: Option<String>,
    pub resets_at: Option<i64>,
    pub fetched_at: i64,
    pub expires_at: Option<i64>,
    pub status: String,
    pub last_error: Option<String>,
    pub focused_provider: Option<String>,
    pub plan_label: Option<String>,
    pub remaining_percent: Option<i64>,
    pub used_label: Option<String>,
    pub limit_label: Option<String>,
    pub reset_label: Option<String>,
    pub pace_label: Option<String>,
    pub view_status: String,
    pub updated_label: String,
    pub status_bar_label: String,
}
