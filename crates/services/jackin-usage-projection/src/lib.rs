//! jackin-usage-projection: surface-neutral canonical usage projection.
//!
//! **Architecture Invariant:** T5.
//! Entry point: [`build_canonical_projection`] — build the V1 publication.

mod account;
mod canonical;

#[cfg(test)]
use icu_collator::{Collator, options::CollatorOptions, options::Strength};
#[cfg(test)]
use icu_locale::Locale;
#[cfg(test)]
use jackin_protocol::control::{
    Money, QuotaBucketView, StatusSlot, UsageConfidence, UsageSeverity, UsageSnapshotStatus,
};
#[cfg(test)]
use jackin_protocol::usage_broker::{
    UsageAccountV1, UsageFreshnessPhaseV1, UsageIdentityKindV1, UsageLifecycleV1,
    UsageMembershipStateV1, UsageMetricGroupKindV1, UsageMetricGroupV1, UsageMetricScopeV1,
    UsageMetricValueV1, UsagePercent, UsageProjectionRefreshStateV1, UsageProjectionSchemaV1,
    UsageProjectionV1, UsageProviderV1, UsageQuotaStateV1, UsageUnresolvedV1,
};

pub(crate) use account::{apply_generation_metadata, discovery_issue, project_account};
pub use canonical::build_canonical_projection;
pub(crate) use jackin_usage_broker_publish::{failure_lifecycle, lifecycle};
pub(crate) use jackin_usage_broker_publish::{
    freshness, project_groups, project_window, provider_freshness, status_label, view_is_usable,
};
#[cfg(test)]
pub(crate) use jackin_usage_broker_publish::{quota_state, spend_quota_state};
pub(crate) use jackin_usage_destination::ProjectionMetadata;

#[cfg(test)]
mod tests;
