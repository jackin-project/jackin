// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Usage-dialog rendering helpers extracted from `dialog_widgets.rs`.
//!
//! `dialog_widgets.rs` is the coordinator; this sub-module owns the per-line /
//! per-section content composition for the usage dialog. The helpers are
//! consumed by `render_usage_info` (in the coordinator) and the test helpers
//! in `tests.rs`; the API surface is re-exported at the parent so the test
//! glob continues to read `dialog_widgets::usage_xxx`.

mod buckets;
mod details;
mod geometry;
mod lines;
mod providers;
pub(crate) use buckets::{usage_quota_bucket_lines, usage_separator_line};
pub(crate) use details::{
    is_quota_bucket_row, usage_meter_parts, usage_quota_bucket_compact_lines,
    usage_quota_bucket_detail_parts, usage_stacked_bucket_detail_rows,
};
pub(crate) use geometry::{
    usage_body_rect, usage_dialog_inner_area, usage_info_required_height, usage_panel_title,
    usage_provider_display_label, usage_scroll_inputs, usage_tab_strip_area,
    usage_tab_strip_index_at, usage_tab_strip_labels, usage_tab_strip_width,
};
pub(crate) use lines::{
    USAGE_CONTENT_PAD_LEFT, USAGE_CONTENT_PAD_RIGHT, USAGE_METER_EMPTY, USAGE_METER_FILLED,
    usage_content_indent, usage_content_width, usage_info_lines, usage_info_lines_for_width,
    usage_line_width, usage_meter_char, usage_row_value,
};
pub(crate) use providers::{
    is_overview_provider_label, is_overview_provider_row, usage_header_two_column,
    usage_identity_lines, usage_legacy_overview_provider_lines, usage_overview_provider_lines,
};
