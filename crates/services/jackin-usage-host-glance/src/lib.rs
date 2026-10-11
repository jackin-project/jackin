//! jackin-usage-host-glance: host label and glance-row builders.
//!
//! **Architecture Invariant:** T4.
//! Entry point: [`build_provider_glance_row`] — build one glance row.

mod render;

pub use render::{
    DrivingBucket, STATUS_BAR_MAX_CHIPS, account_descriptor, build_provider_glance_row,
    drive_label_prefix, driving_bucket_from_view, glance_bucket, selected_account_unavailable_view,
    status_bar_rank_key, view_is_auto_detected, worst_severity_label,
};
