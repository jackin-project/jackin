//! jackin-usage-provider-hermes: `Hermes` TUI attribution adapter.
//!
//! **Architecture Invariant:** T3.
//! Entry point: [`hermes_view`] — attributed usage view.
//!
//! No Hermes-native quota API exists: usage is attributed to underlying
//! provider accounts, or to the Nous Portal subscription for Portal-billed
//! usage. Local rate-limit tracker counters are display state, never a
//! subscription budget. Profiles are exclusively owned — concurrent processes
//! must never share one, and clones deliberately drop rotating OAuth grants.

mod adapter;

pub use adapter::{
    HermesRuntime, HermesSubscription, hermes_auth_status, hermes_renews_label,
    hermes_subscription_bucket, hermes_tracker_counter_buckets, hermes_view,
    parse_hermes_subscription,
};

#[cfg(test)]
mod tests;
