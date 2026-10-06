// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Capsule-local structured usage telemetry cache.
//!
//! This is a daemon-owned store under `/jackin/state/`: Capsule writes quota
//! snapshots after provider refresh and renderers read through the daemon cache,
//! not by opening this database. The schema mirrors the roadmap V1 account
//! snapshot shape so the later host-daemon store can reuse the same rows.

mod buckets;
mod read;
mod schema;
mod types;
mod upsert;
mod views;
mod write;

#[cfg(test)]
use crate::store_backend::connect_local;
#[cfg(test)]
use jackin_core::account_key_hash;

#[cfg(test)]
pub(crate) use buckets::{
    lifecycle_status_bar_label, select_provider_rows, usage_bucket_order,
    usage_confidence_from_label, usage_provider_tabs_from_rows, usage_source_from_label,
    usage_status_from_label,
};
#[cfg(test)]
pub(crate) use read::stored_account_snapshots;
#[cfg(test)]
pub use read::{focused_usage_view, schema_version};
pub(crate) use read::{row_i64, row_opt_i64, row_opt_string, row_string};
pub(crate) use schema::initialize_schema;
#[cfg(test)]
pub(crate) use types::CONNECTION_BUILDS;
pub(crate) use types::SCHEMA_VERSION;
pub use types::{AccountIdentitySummary, StoredAccountUsageSnapshot};
pub(crate) use upsert::{account_snapshot_rows, upsert_account_snapshot_rows};
pub use views::{
    StoredAccountUsageView, list_account_identities, load_account_usage_view,
    load_all_account_usage_views,
};
#[cfg(test)]
pub(crate) use write::connection_build_count;
#[cfg(test)]
pub use write::store_usage_snapshot;
pub use write::store_usage_snapshots;
pub(crate) use write::{block_on_store, open_store};

#[cfg(test)]
mod tests;
