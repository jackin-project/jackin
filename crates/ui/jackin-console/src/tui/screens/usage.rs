// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Simple Console Usage route.
//!
//! Rust supplies already ordered account/window/group values. This module owns
//! only the Console split, focus, and Capsule-shaped meter adaptation.
mod body;
mod detail;
mod labels_metric;
mod labels_time;
mod list;
mod model;
mod publication;
mod refresh;
mod screen;
mod selection;
mod state;
#[cfg(test)]
mod tests;
pub(crate) use body::{
    append_account_full_body, append_account_summary_body, append_overview_account, meter_line,
    meter_style, panel, refreshing_line, row_style,
};
pub(crate) use detail::render_detail;
pub use labels_metric::freshness_age_label;
pub(crate) use labels_metric::{
    issue_text, metric_group_value_summary, metric_scope_summary, non_empty_label, now_epoch,
    raw_percent_note, well_known_provider_name,
};
pub use labels_time::group_freshness_label;
pub(crate) use labels_time::{
    credential_expiry_label, identity_kind_label, lifecycle_label, metric_group_kind_label,
    past_age_label, quota_state_label, relative_time_label, summary_category_rank,
    updated_age_label,
};
pub(crate) use list::render_account_list;
pub use model::{
    USAGE_HEARTBEAT_INTERVAL, UsageAccount, UsageFilter, UsageMetricGroup, UsageRefreshOutcome,
    UsageRefreshRequest, UsageSort, UsageWindow,
};
pub(crate) use publication::{
    append_publication_age, append_publication_issues, empty_publication_lines,
};
pub use screen::{handle_key, render, render_at};
pub use state::UsageScreenState;
