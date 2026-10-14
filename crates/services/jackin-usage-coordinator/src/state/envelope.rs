// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::atomic::AtomicU64;

use jackin_protocol::control::FocusedUsageView;
use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCoordinationError, UsageRefreshPhase,
};

use serde::{Deserialize, Serialize};

pub(crate) const ACCOUNT_STATE_SCHEMA_VERSION: u32 = 3;
pub(crate) const PREVIOUS_ACCOUNT_STATE_SCHEMA_VERSION: u32 = 2;
pub(crate) const LEGACY_ACCOUNT_STATE_SCHEMA_VERSION: u32 = 1;
pub(crate) const MAX_ACCOUNT_STATE_BYTES: u64 = 512 * 1024;
pub(crate) const MAX_CLOCK_SKEW_SECS: i64 = 300;
pub(crate) const MAX_DISPLAY_CHARS: usize = 256;

pub(crate) const PROJECTION_STATE_SCHEMA_VERSION: u32 = 3;
pub(crate) const PREVIOUS_PROJECTION_STATE_SCHEMA_VERSION: u32 = 2;
pub(crate) static STATE_TMP_COUNTER: AtomicU64 = AtomicU64::new(0);
pub(crate) static STATE_QUARANTINE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccountStateEnvelope {
    pub schema_version: u32,

    pub capability: UsageAccountCapability,

    pub generation: u64,

    pub phase: UsageRefreshPhase,

    pub terminal_result: Option<FocusedUsageView>,

    pub last_good: Option<FocusedUsageView>,

    pub terminal_error: Option<UsageCoordinationError>,

    pub started_at_epoch: Option<i64>,

    pub provider_invoked_at_epoch: Option<i64>,

    pub reload_fence_required: bool,

    pub completed_at_epoch: Option<i64>,

    pub rate_limit_deadline_epoch: Option<i64>,

    pub retry_deadline_epoch: Option<i64>,

    pub success_deadline_epoch: Option<i64>,

    pub consecutive_failures: u32,
}

impl AccountStateEnvelope {
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
            reload_fence_required: false,
            completed_at_epoch: None,
            rate_limit_deadline_epoch: None,
            retry_deadline_epoch: None,
            success_deadline_epoch: None,
            consecutive_failures: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, thiserror::Error, PartialEq, Eq)]
pub enum StateStoreError {
    #[error("usage coordinator state is unavailable")]
    Unavailable,

    #[error("usage coordinator state is corrupt")]
    Corrupt,

    #[error("projection state schema {found} requires migration to schema {current}")]
    SchemaMigrationRequired { found: u64, current: u32 },
}
