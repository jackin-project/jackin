// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `OpenRouter` key/account usage snapshot.
//!
//! An ordinary inference key reads `GET {base}/key` (key cap, remaining,
//! period spend, BYOK attribution, expiry). Account balance reads
//! `GET {base}/credits` and needs a Management key: its 403 is a typed scope
//! mismatch that never suppresses the `/key` rows. Model IDs validate against
//! the public `GET {base}/models` catalog; a stale omission is `Unverified`,
//! never a rejection. A null key cap means no configured cap, not infinite
//! credit — no percentage bar is drawn without a matching denominator.
//!
//! Completed activity history (`GET {base}/activity`) is Management-key-only.
//! The current account credential contract supplies an inference key, not a
//! separate Management key, so history remains explicitly unavailable rather
//! than being fetched with the wrong scope or inferred from live key usage.

mod fetch;
mod parse;
mod snapshot;
mod types;

#[cfg(test)]
use super::*;
#[cfg(test)]
use jackin_usage_provider_core::ProviderError;

pub(crate) use fetch::openrouter_key_error_status;
pub(crate) use fetch::{
    OPENROUTER_DEFAULT_BASE_URL, check_openrouter_model_in_catalog, fetch_openrouter_credits,
    fetch_openrouter_key_usage, fetch_openrouter_model_check, openrouter_base_url,
    openrouter_base_url_from,
};
pub(crate) use parse::{
    OpenRouterKeyQuota, openrouter_credits_bucket, parse_openrouter_credits,
    parse_openrouter_key_usage,
};
#[cfg(test)]
pub(crate) use snapshot::openrouter_snapshot_with_key_fetch;
pub(crate) use snapshot::{
    openrouter_snapshot, openrouter_snapshot_with_base, openrouter_snapshot_with_rate_limit,
};
pub(crate) use types::{OpenRouterCreditsOutcome, OpenRouterModelCheck};
pub(crate) use types::{OpenRouterCreditsResponse, OpenRouterKeyData};

#[cfg(test)]
mod tests;
