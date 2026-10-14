#![expect(
    dead_code,
    reason = "provider-adapter fixtures remain testable while production dispatch is broker-only"
)]

mod balance;
mod buckets;
mod fetch;
mod region;
mod snapshot;
mod types;

pub use balance::MiniMaxBalanceResponse;
#[cfg(test)]
pub(crate) use balance::minimax_decimal_minor;
pub use buckets::{
    MiniMaxWindow, minimax_bucket, minimax_bucket_label, minimax_is_general_model,
    minimax_usage_count_line,
};
#[cfg(test)]
pub(crate) use buckets::{minimax_boost_note, minimax_effective_remaining};
pub use fetch::{
    fetch_minimax_usage, first_minimax_usage, minimax_operation_path, minimax_remains_host,
    minimax_reset_epoch, resolve_minimax_remains_urls, resolve_minimax_remains_urls_from,
};
#[cfg(test)]
pub(crate) use fetch::{minimax_duration_seconds, minimax_fetch_plan_from};
pub use region::{
    MiniMaxKeyProduct, MiniMaxRegion, minimax_key_product, minimax_region_from_value,
    resolve_minimax_region_from,
};
pub use snapshot::minimax_snapshot;
pub use types::{
    MiniMaxBaseResponse, MiniMaxComboCard, MiniMaxFetched, MiniMaxModelRemain, MiniMaxUsage,
    MiniMaxUsageData, MiniMaxUsageResponse,
};

#[cfg(test)]
mod tests;
