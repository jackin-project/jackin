#![expect(
    dead_code,
    reason = "provider-adapter fixtures remain testable while production dispatch is broker-only"
)]

mod buckets;
mod fetch;
mod local;
mod snapshot;
mod types;

pub use buckets::{kimi_bucket, kimi_window_seconds};
pub use fetch::fetch_kimi_usage;

pub use local::{
    kimi_local_token_from_value, load_kimi_local_token, load_kimi_local_token_from_home,
};

pub use snapshot::kimi_snapshot;

pub use types::{
    KimiCount, KimiMembership, KimiPool, KimiPools, KimiRateLimit, KimiReset, KimiUsageDetail,
    KimiUsageItem, KimiUsageResponse, KimiUsages, KimiUser, KimiWindow,
};
