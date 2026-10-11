//! jackin-usage-broker-publish: canonical broker projection publication.
//!
//! **Architecture Invariant:** T3.
//! Entry point: [`ProjectionPublisher`] — incremental per-account publication.
//!
//! One immutable [`UsageProjectionV1`](jackin_protocol::usage_broker::UsageProjectionV1)
//! per change: account/window/group projection builders plus the publisher
//! that merges per-account generations, checkpoints them through the
//! coordinator stores, and keeps the last-good publication on failure.

mod account_windows;
mod catalog_diagnostics;
mod errors;
mod freshness;
mod groups;
mod merge;
mod publisher;
mod quota;
mod revoke;
mod windows;

pub use account_windows::{failure_lifecycle, project_window, window_category};
pub(crate) use catalog_diagnostics::apply_catalog_diagnostics;
pub use catalog_diagnostics::{CatalogDiagnosticCode, CatalogDiagnostics};
pub use errors::{
    catalog_revision_conflict, first_publisher_rollback_error, issue_recoverability,
    preserve_publisher_error, projection_store_error, publisher_corrupt_state,
    publisher_unavailable,
};
pub use freshness::{
    bucket_has_quantity, freshness, money_is_exhausted, provider_freshness, quota_state,
    status_label,
};
pub use groups::{
    group_epochs, group_id, group_period, group_phase, group_rank, lifecycle,
    metric_groups_for_view, project_groups, project_plan_group, project_spend_group,
    project_window_group, spend_quota_state, spend_ratio_state, spend_remaining, view_is_usable,
};
pub use merge::{aggregate_freshness, merge_views};
pub use publisher::{AccountIdentityMetadata, ProjectionPublisher};
pub use quota::{issue_code, money_used_raw_percent, quota_state_for_bucket};
pub use revoke::{catalog_entries, retain_revoked_accounts};
pub use windows::{account_for_view, window_for_bucket, windows_for_snapshot};

#[cfg(test)]
use jackin_protocol::control::{
    FocusedUsageView, QuotaBucketView, StatusSlot, UsageSnapshotStatus,
};
#[cfg(test)]
use jackin_protocol::usage_broker::{
    UsageAccountV1, UsageCatalogEntry, UsageCoordinationError, UsageCoordinationErrorKind,
    UsageFreshnessV1, UsageGenerationView, UsagePercent, UsageProjectionV1, UsageRefreshPhase,
};
#[cfg(test)]
use jackin_usage_coordinator::{FileProjectionStateStore, UsageCoordinator};

#[cfg(test)]
mod tests;
