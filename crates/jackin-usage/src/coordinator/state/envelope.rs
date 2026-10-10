// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! State envelopes, schema versions, and store errors.

use std::sync::atomic::AtomicU64;

use jackin_protocol::control::FocusedUsageView;
use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCoordinationError, UsageRefreshPhase,
};

use serde::{Deserialize, Serialize};

pub(crate) const ACCOUNT_STATE_SCHEMA_VERSION: u32 = 2;
pub(crate) const PREVIOUS_ACCOUNT_STATE_SCHEMA_VERSION: u32 = 1;
pub(crate) const MAX_ACCOUNT_STATE_BYTES: u64 = 512 * 1024;
pub(crate) const MAX_CLOCK_SKEW_SECS: i64 = 300;
pub(crate) const MAX_DISPLAY_CHARS: usize = 256;
/// Exact durable projection envelope schema.
///
/// Schema v1 is deliberately not migrated: it did not carry the admitted
/// catalog required to fence removed credentials. Loading v1 quarantines the
/// file and lets the broker rebuild an empty projection from the current host
/// catalog. This is the migration contract; no serde default may hide a
/// missing or unknown catalog.
pub(crate) const PROJECTION_STATE_SCHEMA_VERSION: u32 = 2;
pub(crate) static STATE_TMP_COUNTER: AtomicU64 = AtomicU64::new(0);
pub(crate) static STATE_QUARANTINE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Complete durable state for one canonical account.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccountStateEnvelope {
    /// Persisted schema version.
    pub schema_version: u32,
    /// Canonical account authority.
    pub capability: UsageAccountCapability,
    /// Monotonic account generation.
    pub generation: u64,
    /// Refresh lifecycle phase.
    pub phase: UsageRefreshPhase,
    /// Terminal result for this generation, when data-bearing.
    pub terminal_result: Option<FocusedUsageView>,
    /// Last independently data-bearing provider result.
    pub last_good: Option<FocusedUsageView>,
    /// Sanitized terminal failure.
    pub terminal_error: Option<UsageCoordinationError>,
    /// Generation admission timestamp, before queue dispatch.
    pub started_at_epoch: Option<i64>,
    /// Provider invocation start timestamp, persisted separately from queue
    /// admission so Claude's minimum interval begins when work actually runs.
    pub provider_invoked_at_epoch: Option<i64>,
    /// Generation completion timestamp.
    pub completed_at_epoch: Option<i64>,
    /// Provider-mandated rate-limit deadline.
    pub rate_limit_deadline_epoch: Option<i64>,
    /// Provider-supplied general retry deadline.
    pub retry_deadline_epoch: Option<i64>,
    /// Ambient success-cooldown deadline.
    pub success_deadline_epoch: Option<i64>,
    /// Consecutive provider failure count.
    pub consecutive_failures: u32,
}

impl AccountStateEnvelope {
    /// Initial idle state for a newly discovered account.
    #[must_use]
    pub fn idle(capability: UsageAccountCapability) -> Self {
        Self {
            schema_version: ACCOUNT_STATE_SCHEMA_VERSION,
            capability,
            generation: 0,
            phase: UsageRefreshPhase::Idle,
            terminal_result: None,
            last_good: None,
            terminal_error: None,
            started_at_epoch: None,
            provider_invoked_at_epoch: None,
            completed_at_epoch: None,
            rate_limit_deadline_epoch: None,
            retry_deadline_epoch: None,
            success_deadline_epoch: None,
            consecutive_failures: 0,
        }
    }
}

/// Sanitized persistence failure.
#[derive(Debug, Clone, Copy, thiserror::Error, PartialEq, Eq)]
pub enum StateStoreError {
    /// Host state path, owner, or permissions are unavailable.
    #[error("usage coordinator state is unavailable")]
    Unavailable,
    /// Envelope bytes or schema failed validation.
    #[error("usage coordinator state is corrupt")]
    Corrupt,
}
