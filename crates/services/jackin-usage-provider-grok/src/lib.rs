//! jackin-usage-provider-grok: `Grok` / `xAI` usage snapshot collection.
//!
//! **Architecture Invariant:** T3.
//! Entry point: [`grok_snapshot`] — `Grok` usage snapshot.

mod billing;
mod rpc;
mod snapshot;
mod types;
mod views;

#[cfg(test)]
pub(crate) use billing::grok_tier_from_settings;
pub use billing::{
    fetch_grok_billing, fetch_grok_rest_billing, fetch_grok_rpc_billing,
    parse_grok_rest_billing_response,
};
pub use rpc::{
    grok_bearer_token, grok_bearer_token_from_entry, grok_binary_path, grok_rpc_request,
    grok_rpc_request_payload, grpc_web_data_frames, parse_grok_web_billing_response, scan_protobuf,
};
pub use snapshot::{
    grok_account_label, grok_account_label_or_presence, grok_snapshot,
    grok_snapshot_from_rpc_result, grok_snapshot_from_rpc_result_with_rate_limit,
};
pub use types::{
    GrokBillingAuth, GrokBillingConfig, GrokBillingResponse, GrokBillingSnapshot, GrokCent,
    GrokCurrentPeriod, GrokWebBillingSnapshot, grok_period_label, positive_cent_value,
    resolve_grok_billing_auth,
};
pub use views::{grok_cycle_label_from_minutes, grok_cycle_label_from_reset};

#[cfg(test)]
mod tests;
