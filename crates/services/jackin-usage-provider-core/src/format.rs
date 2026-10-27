// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod cli;
mod labels;
mod numbers;
mod presentation;
mod text;

pub use cli::{
    CliOutput, collect_cli_output, read_process_pipe, run_cli_with_timeout,
    run_cli_with_timeout_full,
};
pub use labels::{PercentStyle, ResetStyle, UsageFormatPrefs};
pub use labels::{
    codex_limit_label, compact_duration_label, exact_reset_parenthetical, expiry_label,
    humanize_plan_label, humanize_words_with, local_timestamp_label, percent_headline,
    quota_pace_label, reset_label, reset_label_with_prefs, window_minutes_label,
};
pub use numbers::PROCESS_OUTPUT_MAX;
pub use numbers::{
    derived_remaining_from_used_fraction, env_value, format_amount_with_unit,
    format_percent_points, json_epoch_seconds, json_number, parse_iso_epoch,
    used_percent_from_fraction,
};
pub use presentation::{
    UsageBucketPresentation, count_observation_detail_label, usage_bucket_presentation,
    usage_detail_presentation, usage_display_status_label, usage_identity_presentation,
};
pub use text::percent_before_used;
pub use text::{
    codex_account_from_value, compact_count, dollar_amounts, first_string_key, format_cents,
    format_currency, home_path, oauth_origin, titlecase_ascii,
};
