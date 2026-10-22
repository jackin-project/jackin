mod buckets;
mod cli;
mod credits;
mod parse;
mod snapshot;
mod types;

pub use buckets::{
    antigravity_buckets, antigravity_identity_from_value, antigravity_plan_from_value,
};
pub use cli::{
    ANTIGRAVITY_KEYCHAIN_SERVICE, ANTIGRAVITY_MIN_JSON_VERSION, agy_version_supports_json,
    antigravity_cli_version, fetch_antigravity_cli_credits, fetch_antigravity_cli_usage,
    parse_agy_version,
};
pub use credits::{
    AntigravityCredits, antigravity_credits_bucket, parse_antigravity_credits_output,
};
pub(crate) use parse::{
    antigravity_percentages_from_entry, antigravity_unclassified_utilization_from_entry,
};
pub use parse::{antigravity_remaining_from_entry, parse_antigravity_usage_output};
pub use snapshot::antigravity_snapshot;
#[cfg(test)]
pub(crate) use snapshot::{
    antigravity_snapshot_status, antigravity_status_view, antigravity_version_error_status,
};
pub(crate) use types::AntigravityUtilizationObservation;
pub use types::{AntigravityFamily, AntigravityPool, AntigravityUsage, AntigravityWindow};

#[cfg(test)]
mod tests;
