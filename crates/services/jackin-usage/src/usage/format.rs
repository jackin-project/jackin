// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Formatting, CLI, and JSON helpers shared by every usage provider.
//!
//! Extracted from `usage.rs` for the file-size ratchet. Lives in a sibling
//! module so the
//! provider-specific sections in `usage.rs` only carry their own logic,
//! not the shared display/parsing utilities every provider depends on.
//!
//! Helpers live in submodules and are re-exported here so the coordinator
//! can still call every helper directly; tests under `usage/tests.rs` see
//! them through `super::*` and do not need their own re-exports.

mod cli;
mod labels;
mod numbers;
mod presentation;
mod text;

pub(crate) use cli::{CliOutput, run_cli_with_timeout, run_cli_with_timeout_full};
#[cfg(test)]
pub(crate) use cli::{collect_cli_output, read_process_pipe};
#[cfg(test)]
pub(crate) use labels::local_timestamp_label;
pub use labels::{PercentStyle, ResetStyle, UsageFormatPrefs};
pub(crate) use labels::{
    codex_limit_label, compact_duration_label, exact_reset_parenthetical, expiry_label,
    humanize_plan_label, humanize_words_with, percent_headline, quota_pace_label, reset_label,
    reset_label_with_prefs, window_minutes_label,
};
pub(crate) use numbers::{
    PROCESS_OUTPUT_MAX, env_value, format_amount_with_unit, json_number, parse_iso_epoch,
    remaining_from_fraction, used_percent_from_fraction, used_percent_label,
};
pub use presentation::{
    UsageBucketPresentation, usage_bucket_presentation, usage_detail_presentation,
    usage_display_status_label, usage_identity_presentation,
};
pub(crate) use text::{
    codex_account_from_value, compact_count, dollar_amounts, first_string_key, format_cents,
    format_currency, home_path, oauth_origin, percent_before_used, titlecase_ascii,
};

#[cfg(test)]
mod tests;
