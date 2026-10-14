#![expect(
    dead_code,
    reason = "provider-adapter fixtures remain testable while production dispatch is broker-only"
)]

mod buckets;
mod fetch;
mod local;
mod snapshot;
mod types;

#[cfg(test)]
pub(crate) use buckets::kimi_over_cap_label;
pub use buckets::{kimi_bucket, kimi_window_seconds};
pub use fetch::fetch_kimi_usage;
#[cfg(test)]
pub(crate) use fetch::kimi_usages_url_from_base;
#[cfg(test)]
pub(crate) use local::{KimiLocalUsage, kimi_extra_usage_bucket};
pub use local::{
    kimi_local_token_from_value, load_kimi_local_token, load_kimi_local_token_from_home,
};
pub use snapshot::kimi_snapshot;
#[cfg(test)]
pub(crate) use snapshot::{kimi_account_identity, kimi_membership_plan};
pub use types::{
    KimiCount, KimiMembership, KimiPool, KimiPools, KimiRateLimit, KimiReset, KimiUsageDetail,
    KimiUsageItem, KimiUsageResponse, KimiUsages, KimiUser, KimiWindow,
};

#[cfg(test)]
mod tests;
