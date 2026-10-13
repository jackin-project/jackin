//! jackin-usage-provider-amp: `Amp` usage snapshot collection.
//!
//! **Architecture Invariant:** T3.
//! Entry point: [`amp_snapshot`] — `Amp` usage snapshot.

mod fetch;
mod parse;
mod snapshot;
mod types;
mod views;

pub use fetch::{fetch_amp_api_usage, fetch_amp_cli_usage, load_amp_api_key};
pub use parse::parse_amp_usage_output;
pub use snapshot::{amp_api_key_snapshot, amp_snapshot};
pub use types::{AmpRenewal, AmpSubscription, AmpSubscriptionKind, AmpUsage, AmpWorkspaceBalance};
pub use views::{AmpSuccessContext, amp_view_from_usage};
