// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `MiniMax` usage snapshot.
//!
//! Carved out of `usage.rs` for the file-size ratchet. Items in this module
//! are `pub(crate)` so the coordinator (`usage.rs`) can re-export them.
//!
//! Two products, selected by key shape (see `ref-contracts-B.md` §3): Token
//! Plan subscription keys read per-model interval/weekly remains from
//! `GET {apiBase}/v1/token_plan/remains` (legacy fallback
//! `.../v1/api/openplatform/coding_plan/remains`); secret `sk-api-*` keys
//! read PAYG balances from `GET {base}/account/query_balance`. A credential
//! is never sent to another region because the first request failed: the
//! default region is global, and CN hosts are used only when explicitly
//! selected.

mod balance;
mod buckets;
mod fetch;
mod region;
mod snapshot;
mod types;

#[cfg(test)]
use super::*;

pub(crate) use balance::MiniMaxBalanceResponse;
#[cfg(test)]
pub(crate) use balance::minimax_decimal_minor;
pub(crate) use buckets::{
    MiniMaxWindow, minimax_bucket, minimax_bucket_label, minimax_is_general_model,
    minimax_usage_count_line,
};
#[cfg(test)]
pub(crate) use buckets::{minimax_boost_note, minimax_effective_remaining};
pub(crate) use fetch::{
    fetch_minimax_usage, first_minimax_usage, minimax_operation_path, minimax_remains_host,
    minimax_reset_epoch, resolve_minimax_remains_urls, resolve_minimax_remains_urls_from,
};
#[cfg(test)]
pub(crate) use fetch::{minimax_duration_seconds, minimax_fetch_plan_from};
pub(crate) use region::{
    MiniMaxKeyProduct, MiniMaxRegion, minimax_key_product, minimax_region_from_value,
    resolve_minimax_region_from,
};
pub(crate) use snapshot::minimax_snapshot;
pub(crate) use types::{
    MiniMaxBaseResponse, MiniMaxComboCard, MiniMaxFetched, MiniMaxModelRemain, MiniMaxUsage,
    MiniMaxUsageData, MiniMaxUsageResponse,
};

#[cfg(test)]
mod tests;
