#![expect(
    dead_code,
    reason = "provider-adapter fixtures remain testable while production dispatch is broker-only"
)]

mod fetch;
mod money;
mod parse;
mod snapshot;
mod types;

pub use fetch::openrouter_key_error_status;
pub use fetch::{
    OPENROUTER_DEFAULT_BASE_URL, check_openrouter_model_in_catalog, fetch_openrouter_credits,
    fetch_openrouter_key_usage_raw, fetch_openrouter_model_check, openrouter_base_url,
    openrouter_base_url_from,
};
pub use parse::{
    OpenRouterKeyQuota, openrouter_credits_bucket, parse_openrouter_credits,
    parse_openrouter_key_usage_raw,
};
pub use snapshot::{
    openrouter_snapshot, openrouter_snapshot_with_base, openrouter_snapshot_with_rate_limit,
};
pub use types::{OpenRouterCreditsOutcome, OpenRouterModelCheck};
pub use types::{OpenRouterCreditsResponse, OpenRouterKeyData};

#[cfg(test)]
#[path = "numeric_tests.rs"]
mod tests;
