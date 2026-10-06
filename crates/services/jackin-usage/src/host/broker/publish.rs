// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Incremental per-account publication of the canonical broker projection.
//!
//! The broker publishes one immutable [`UsageProjectionV1`] per change. Each
//! account merges independently as its generation completes: one stalled
//! account never blocks healthy accounts, and a stalled account keeps its
//! loading/refreshing state instead of receiving fabricated data.
//!
//! Publication rules:
//!
//! - The catalog revision (`discovery_revision`) is fixed for the broker
//!   process lifetime; every publication carries the same revision while
//!   `broker_generation` increases monotonically.
//! - Only capabilities observed on broker traffic are merged. Unknown or
//!   never-requested accounts are never fabricated, and per-account published
//!   generations only move forward, so older generations can never regress a
//!   publication.
//! - A publication that fails [`UsageProjectionV1::validate`] is discarded and
//!   the last-good publication is kept.

mod errors;
mod merge;
mod publisher;
mod quota;
mod revoke;
mod windows;

#[cfg(test)]
use crate::coordinator::{FileProjectionStateStore, UsageCoordinator};
#[cfg(test)]
use jackin_protocol::control::{
    FocusedUsageView, QuotaBucketView, StatusSlot, UsageSnapshotStatus,
};
#[cfg(test)]
use jackin_protocol::usage_broker::{
    UsageAccountV1, UsageCatalogEntry, UsageCoordinationError, UsageCoordinationErrorKind,
    UsageFreshnessV1, UsageGenerationView, UsagePercent, UsageProjectionV1, UsageRefreshPhase,
};

pub(in crate::host) use errors::issue_recoverability;
pub(crate) use errors::{
    catalog_revision_conflict, first_publisher_rollback_error, preserve_publisher_error,
    projection_store_error, publisher_corrupt_state, publisher_unavailable,
};
#[cfg(test)]
pub(crate) use merge::aggregate_freshness;
pub(crate) use merge::merge_views;
pub(crate) use publisher::{AccountIdentityMetadata, ProjectionPublisher};
pub(in crate::host) use quota::issue_code;
pub(crate) use quota::{money_used_raw_percent, quota_state_for_bucket};
pub(crate) use revoke::{catalog_entries, retain_revoked_accounts};
pub(crate) use windows::account_for_view;

#[cfg(test)]
mod tests;
