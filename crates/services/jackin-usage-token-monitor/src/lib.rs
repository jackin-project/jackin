pub mod amp;
pub mod claude;
pub mod codex;
pub mod kimi;
pub mod opencode;
pub mod pricing;

mod discover;
mod monitor;
mod record;
mod session;
mod status;
mod totals;

#[cfg(test)]
use jackin_core::Agent;
#[cfg(test)]
use jackin_telemetry::schema;

pub use discover::{
    PROVIDER_WALK_DEPTH, find_provider_files, json_u64, read_file_text, recompute_spend,
};
pub use monitor::TokenMonitor;
pub(crate) use record::record_token_usage;
#[cfg(test)]
pub(crate) use record::{provider_name, token_usage_delta};
pub use session::TokenSession;
pub(crate) use status::PollStatus;
pub use status::ProviderReadDegraded;
pub use totals::{PollReport, SpendAcc, TokenTotals};

#[cfg(test)]
mod tests;
