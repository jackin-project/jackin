// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `Amp` usage snapshot.
//!
//! Carved out of `usage.rs` for the file-size ratchet. Items in this module
//! are `pub(crate)` so the coordinator (`usage.rs`) can re-export them.

mod fetch;
mod parse;
mod snapshot;
mod types;
mod views;

pub(crate) use fetch::{fetch_amp_api_usage, fetch_amp_cli_usage, load_amp_api_key};
pub(crate) use parse::parse_amp_usage_output;
pub(crate) use snapshot::{amp_api_key_snapshot, amp_snapshot};
pub(crate) use types::{
    AmpRenewal, AmpSubscription, AmpSubscriptionKind, AmpUsage, AmpWorkspaceBalance,
};
pub(crate) use views::{AmpSuccessContext, amp_view_from_usage};
