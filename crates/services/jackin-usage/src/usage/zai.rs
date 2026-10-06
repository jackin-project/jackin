// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `Z.AI` / `GLM` usage snapshot.
//!
//! Carved out of `usage.rs` for the file-size ratchet. Items in this module
//! are `pub(crate)` so the coordinator (`usage.rs`) can re-export them.
//!
//! Quota contract (see `ref-contracts-B.md` §2): `GET
//! {api.z.ai,open.bigmodel.cn}/api/monitor/usage/quota/limit` with
//! `data.limits[]` carrying new `CREDIT_LIMIT` and older `TOKENS_LIMIT`
//! windows plus separate `TIME_LIMIT` tool/MCP quotas. A 2xx
//! `success: false` envelope means a valid key with no GLM Coding Plan — a
//! distinct state, not a transport error. CN team scope appends `?type=2`
//! with `Bigmodel-Organization` / `Bigmodel-Project` headers; missing
//! selectors can return HTTP success with empty data.

mod buckets;
mod fetch;
mod quota;
mod snapshot;

pub(crate) use buckets::{zai_bucket, zai_count_line};
#[cfg(test)]
pub(crate) use buckets::{zai_credit_rate_note, zai_is_peak, zai_model_note};
#[cfg(test)]
pub(crate) use fetch::zai_team_scope_from;
pub(crate) use fetch::{
    fetch_zai_usage, json_epoch_seconds, resolve_zai_quota_url, resolve_zai_quota_url_from,
    resolve_zai_team_scope, zai_quota_host,
};
pub(crate) use quota::{ZaiLimitRaw, ZaiQuotaData, ZaiQuotaResponse};
pub(crate) use snapshot::provider_key_snapshot;

#[cfg(test)]
mod tests;
