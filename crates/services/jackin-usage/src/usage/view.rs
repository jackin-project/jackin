// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! View-building and rendering helpers shared by all providers.
//!
//! Carved out of `usage.rs` for the file-size ratchet. Items in this module
//! are `pub(crate)` so the coordinator (`usage.rs`) can re-export them.

mod cache;
mod compose;
mod meta;
mod refresh;
mod snapshots;
mod status;
mod tabs;

pub(crate) use compose::{UsageViewInput, usage_view};
pub(crate) use meta::{
    broker_account_id_from_cache_key, bucket, timed_bucket, usage_tab_source_label,
    usage_tab_status_label, with_status_slot,
};
pub(crate) use refresh::{
    cached_refreshing_view, cached_unavailable_view, decorate_surface_view, mark_active_tab,
    preserve_cached_quota_on_failed_refresh, refresh_failed_view_presentation,
};
pub(crate) use snapshots::{account_snapshot_views_from_cache, quota_amounts_for_account_snapshot};
pub(crate) use status::{
    amp_status_bar_headline, compact_account_identity, spend_headline_label,
    status_bar_fresh_or_stale, status_bar_headline_for_surface, status_bar_label,
    status_bar_quota_labels, summary_bucket,
};
pub(crate) use tabs::{
    account_tab_label_for_parts, enrich_provider_tabs, provider_tabs, usage_account_tab_id,
};
