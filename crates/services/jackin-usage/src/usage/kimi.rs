// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `Kimi` usage snapshot.
//!
//! Carved out of `usage.rs` for the file-size ratchet. Items in this module
//! are `pub(crate)` so the coordinator (`usage.rs`) can re-export them.
//!
//! Response families (see `ref-contracts-B.md` §1):
//!
//! * Code API `GET {base}/coding/v1/usages`: `usage` summary + `limits[]`
//!   rate windows + `usages` rolling/weekly/monthly pools object +
//!   `user.membership.level` + `version`.
//! * Web gateway `BillingService/GetUsages`: `usages` list with a
//!   `FEATURE_CODING` entry (same `detail`/`limits[]` shapes).
//! * Local server `GET /api/v1/oauth/usage`: `summary`/`limits` plus the
//!   Extra Usage wallet (`KimiLocalUsage`); `/api/v1/oauth/userinfo` carries
//!   the shared billing identity also present as Code API `user`.

mod buckets;
mod fetch;
mod local;
mod snapshot;
mod types;

#[cfg(test)]
use super::*;

#[cfg(test)]
pub(crate) use buckets::kimi_over_cap_label;
pub(crate) use buckets::{kimi_bucket, kimi_window_seconds};
pub(crate) use fetch::fetch_kimi_usage;
#[cfg(test)]
pub(crate) use fetch::kimi_usages_url_from_base;
#[cfg(test)]
pub(crate) use local::{KimiLocalUsage, kimi_extra_usage_bucket};
pub(crate) use local::{
    kimi_local_token_from_value, load_kimi_local_token, load_kimi_local_token_from_home,
};
pub(crate) use snapshot::kimi_snapshot;
#[cfg(test)]
pub(crate) use snapshot::{kimi_account_identity, kimi_membership_plan};
pub(crate) use types::{
    KimiCount, KimiPool, KimiPools, KimiRateLimit, KimiReset, KimiUsageDetail, KimiUsageItem,
    KimiUsageResponse, KimiUsages, KimiWindow,
};

#[cfg(test)]
mod tests;
