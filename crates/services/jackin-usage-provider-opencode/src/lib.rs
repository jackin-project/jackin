//! jackin-usage-provider-opencode: `OpenCode` Go subscription-limit adapter.
//!
//! **Architecture Invariant:** T3.
//! Entry point: [`opencode_profile_snapshot`] — `OpenCode` profile snapshot.
//!
//! `OpenCode` exposes one API credential in `auth.json` and a provider-owned
//! rolling/weekly/monthly response. The account stays provisional — identity is
//! never derived from the Bearer [REDACTED] A valid key without a Go subscription is a
//! typed entitlement error, never a key failure.

mod adapter;

pub use adapter::{
    OpenCodeQuota, OpenCodeUsageError, classify_opencode_http_error, fetch_opencode_usage,
    load_opencode_api_key, opencode_profile_snapshot, parse_opencode_usage,
};

#[cfg(test)]
mod tests;
