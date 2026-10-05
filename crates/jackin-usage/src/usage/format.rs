// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Formatting, CLI, and JSON helpers shared by every usage provider.
//!
//! Extracted from `usage.rs` for the file-size ratchet. Lives in a sibling
//! module so the
//! provider-specific sections in `usage.rs` only carry their own logic,
//! not the shared display/parsing utilities every provider depends on.
//!
//! Visibility is `pub(super)` so the coordinator can still call every
//! helper directly; tests under `usage/tests.rs` see them through
//! `super::*` and do not need their own re-exports.

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::mpsc::SyncSender;
use std::time::Duration;

use chrono::{DateTime, Local, TimeZone, Utc};

use super::process_telemetry;

pub(super) const PROCESS_OUTPUT_MAX: usize = 1024 * 1024;

pub(super) fn env_value(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

/// Clamp a raw provider utilization into a `0..=100` "used" percentage.
///
/// Accepts both fraction form (`0.0..=1.0`) and already-percent form (`>1.0`).
/// Returns `None` for non-finite or negative inputs: several providers use a
/// negative sentinel (e.g. `-1`) for "unknown/unlimited", which must be omitted,
/// never fabricated into a full meter (`remaining_from_fraction(-0.5)` would
/// otherwise yield `Some(100)` — a "100% left" row for data that is absent).
pub(super) fn used_percent_from_fraction(value: f64) -> Option<u8> {
    // The clamped-to-100 sibling of `used_percent_uncapped`: same fraction/percent
    // heuristic and absent-value guard, capped at 100 for the `% left` meter.
    used_percent_uncapped(value).map(|used| used.min(100) as u8)
}

pub(super) fn remaining_from_fraction(value: f64) -> Option<u8> {
    used_percent_from_fraction(value).map(|used| 100u8.saturating_sub(used))
}

pub(super) fn used_percent_label(value: f64) -> Option<String> {
    // Surface over-cap usage truthfully: a window the API reports above its limit
    // renders e.g. `150% used` rather than being clamped to `100% used` (Bug 11 —
    // the clamp silently discarded the overage the API provided). `remaining`
    // stays clamped at 0 (nothing left / bar full); only the used side carries the
    // overage.
    used_percent_uncapped(value).map(|used| format!("{used}% used"))
}

/// Used-percent without the upper clamp `used_percent_from_fraction` applies, so
/// an over-cap window keeps its true figure (e.g. `150`). Treats a value `<= 1.0`
/// as a fraction (×100) and a larger value as an already-scaled percent, matching
/// the fraction/percent heuristic the rest of the module uses.
pub(super) fn used_percent_uncapped(value: f64) -> Option<u16> {
    if !value.is_finite() || value < 0.0 {
        return None;
    }
    let used = if value <= 1.0 { value * 100.0 } else { value };
    #[expect(
        clippy::cast_sign_loss,
        reason = "value filtered non-negative above; clamp bounds the f64→u16 cast"
    )]
    {
        Some(used.round().clamp(0.0, f64::from(u16::MAX)) as u16)
    }
}

pub(super) fn parse_iso_epoch(value: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|date| date.with_timezone(&Utc).timestamp())
}

/// Percent display preference for presentation-time labels (not persisted).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PercentStyle {
    /// Remaining percent with a `left` suffix where full phrases are used.
    #[default]
    Left,
    /// Used percent with a `used` suffix where full phrases are used.
    Used,
}

/// Reset-time display preference for presentation-time labels (not persisted).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResetStyle {
    /// `Resets in {countdown} ({local clock})` — shipped default.
    #[default]
    Countdown,
    /// `Resets {local clock}` — clock-led form.
    ExactClock,
}

/// Presentation-time format prefs. Defaults are byte-identical to shipped labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct UsageFormatPrefs {
    /// How percent-bearing quota headlines render.
    pub percent_style: PercentStyle,
    /// How reset lines render.
    pub reset_style: ResetStyle,
}

pub(super) fn reset_label(reset_at: i64, now: i64) -> String {
    reset_label_with_prefs(reset_at, now, UsageFormatPrefs::default())
}

/// Reset label shaped by presentation prefs (Countdown is the shipped form).
pub(crate) fn reset_label_with_prefs(reset_at: i64, now: i64, prefs: UsageFormatPrefs) -> String {
    if reset_at <= now {
        return "Resets now".to_owned();
    }
    match prefs.reset_style {
        ResetStyle::Countdown => {
            let remaining = reset_at.saturating_sub(now);
            let countdown = if remaining < 60 {
                "under a minute".to_owned()
            } else {
                compact_duration_label(remaining)
            };
            format!(
                "Resets in {countdown} ({})",
                local_timestamp_label(reset_at)
            )
        }
        ResetStyle::ExactClock => format!("Resets {}", local_timestamp_label(reset_at)),
    }
}

/// Exact-clock fragment for overview rows, e.g. `(Jul 28, 17:02)`.
pub(crate) fn exact_reset_parenthetical(reset_at: i64) -> String {
    format!("({})", local_timestamp_label(reset_at))
}

/// Compact percent headline from remaining: default `97% left`, or `3% used`.
pub(crate) fn percent_headline(remaining: u8, prefs: UsageFormatPrefs) -> String {
    match prefs.percent_style {
        PercentStyle::Left => format!("{remaining}% left"),
        PercentStyle::Used => {
            let used = 100u8.saturating_sub(remaining);
            format!("{used}% used")
        }
    }
}

pub(super) fn expiry_label(expires_at: i64, now: i64) -> String {
    if expires_at <= now {
        return "now".to_owned();
    }
    format!(
        "in {} ({})",
        compact_duration_label(expires_at.saturating_sub(now).max(0)),
        local_timestamp_label(expires_at)
    )
}

pub(crate) fn local_timestamp_label(epoch: i64) -> String {
    Local.timestamp_opt(epoch, 0).single().map_or_else(
        || "local time unavailable".to_owned(),
        |timestamp| timestamp.format("%b %-d, %H:%M").to_string(),
    )
}

pub(super) fn quota_pace_label(
    remaining_percent: Option<u8>,
    reset_at: Option<i64>,
    window_seconds: Option<i64>,
    now: i64,
) -> Option<String> {
    let remaining_percent_raw = remaining_percent?;
    let remaining_percent = f64::from(remaining_percent_raw);
    let reset_in = reset_at?.saturating_sub(now).max(0);
    let window_seconds = window_seconds?.max(1);
    if reset_in > window_seconds {
        return None;
    }
    let time_left_percent = reset_in as f64 / window_seconds as f64 * 100.0;
    // CodexBar pace model: compare remaining quota against the fraction of the
    // window still left. `delta > 0` means more quota than time remains (ahead
    // of pace = reserve); `delta < 0` means burning faster than the clock
    // (behind = deficit); within 2 points is "On pace". The reset countdown is
    // carried separately in the bucket's reset label, so the pace token stays a
    // bare phrase exactly as the previews show.
    let delta = remaining_percent - time_left_percent;
    let pace = if delta.abs() <= 2.0 {
        "On pace".to_owned()
    } else if delta > 0.0 {
        format!("{}% in reserve", delta.round() as i64)
    } else {
        format!("{}% in deficit", (-delta).round() as i64)
    };
    // Variant A run-out: append `· Runs out in <duration>` only when the linear
    // projection from window start runs out before the reset. Compare exact
    // integer cross-products (float delta is display math and can round a
    // clock-equality case slightly negative).
    let used = 100_i128 - i128::from(remaining_percent_raw);
    let elapsed = window_seconds - reset_in;
    let behind_clock = i128::from(remaining_percent_raw) * i128::from(window_seconds)
        < i128::from(reset_in) * 100_i128;
    if used > 0 && elapsed > 0 && behind_clock {
        let numerator = i128::from(remaining_percent_raw) * i128::from(elapsed);
        let display_seconds = (numerator + used / 2) / used;
        let display_seconds = i64::try_from(display_seconds).ok()?;
        return Some(format!(
            "{pace} · Runs out in {}",
            compact_duration_label(display_seconds)
        ));
    }
    Some(pace)
}

/// Compact time ladder (48-hour threshold):
/// - **&lt; 1 hour** → compact minutes (`45m`)
/// - **&lt; 48 hours** → compact hours (`36h`, optional `36h 30m`) — never days
/// - **≥ 48 hours** → compact days (`2d`, optional `2d 1h`)
///
/// Prefer hours until the 48h line; do not emit a day form for 24–47h windows.
pub(crate) fn compact_duration_label(seconds: i64) -> String {
    let seconds = seconds.max(0);
    if seconds < 60 {
        return "<1m".to_owned();
    }
    let total_hours = seconds / 3_600;
    let minutes = (seconds % 3_600) / 60;
    if total_hours >= 48 {
        let days = total_hours / 24;
        let hours = total_hours % 24;
        if hours > 0 {
            format!("{days}d {hours}h")
        } else {
            format!("{days}d")
        }
    } else if total_hours > 0 {
        if minutes > 0 {
            format!("{total_hours}h {minutes}m")
        } else {
            format!("{total_hours}h")
        }
    } else {
        format!("{minutes}m")
    }
}

#[cfg(test)]
mod tests;

pub(super) fn window_minutes_label(minutes: i64) -> Option<String> {
    if minutes <= 0 {
        return None;
    }
    if minutes % (7 * 24 * 60) == 0 {
        let weeks = minutes / (7 * 24 * 60);
        return Some(format!(
            "{weeks} week{} window",
            if weeks == 1 { "" } else { "s" }
        ));
    }
    if minutes % (24 * 60) == 0 {
        let days = minutes / (24 * 60);
        return Some(format!(
            "{days} day{} window",
            if days == 1 { "" } else { "s" }
        ));
    }
    if minutes % 60 == 0 {
        let hours = minutes / 60;
        return Some(format!(
            "{hours} hour{} window",
            if hours == 1 { "" } else { "s" }
        ));
    }
    Some(format!("{minutes} minute window"))
}

/// Split a machine-style identifier on `_`/`-`/whitespace and join the per-word
/// transform with spaces. Shared by `humanize_plan_label` (plain title-case) and
/// `codex_plan_display_name` (acronym-aware words).
pub(super) fn humanize_words_with(value: &str, word: impl Fn(&str) -> String) -> String {
    value
        .split(|c: char| c == '_' || c == '-' || c.is_whitespace())
        .filter(|part| !part.is_empty())
        .map(word)
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn humanize_plan_label(value: &str) -> String {
    humanize_words_with(value, titlecase_ascii)
}

pub(super) fn codex_limit_label(value: &str) -> String {
    let lower = value.to_ascii_lowercase();
    if lower.contains("spark") {
        "Codex Spark".to_owned()
    } else {
        humanize_plan_label(value)
    }
}

pub(super) fn json_number(value: &serde_json::Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
}

pub(super) fn format_amount_with_unit(value: f64, unit: &str) -> String {
    let amount = if value.fract().abs() < f64::EPSILON {
        format!("{}", value as i64)
    } else {
        format!("{value:.2}")
    };
    format!("{amount} {unit}")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CliOutput {
    pub(crate) success: bool,
    pub(crate) exit_code: Option<i32>,
    pub(super) stdout: String,
    pub(super) stderr: String,
}

pub(super) fn run_cli_with_timeout(
    command: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<String, String> {
    let output = run_cli_with_timeout_full(command, args, timeout)?;
    if !output.success {
        return Err(format!(
            "{command} exited with status {:?}",
            output.exit_code
        ));
    }
    Ok(output.stdout)
}

pub(super) fn run_cli_with_timeout_full(
    command: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<CliOutput, String> {
    let operation = process_telemetry::ChildOperation::begin(command);
    let request = jackin_process::ExecRequest::new(command, args)
        .timeout(timeout)
        .output_limits(PROCESS_OUTPUT_MAX, PROCESS_OUTPUT_MAX);
    let output = match jackin_process::exec_sync(&request) {
        Ok(output) => output,
        Err(error) => {
            if error.downcast_ref::<jackin_process::ExecStage>()
                == Some(&jackin_process::ExecStage::Spawn)
            {
                operation.spawn_failed();
                return Err("usage command failed to start".to_owned());
            }
            operation.io_failed();
            return Err("usage command output failed".to_owned());
        }
    };
    if output.timed_out {
        operation.timed_out();
        return Err("usage command timed out".to_owned());
    }
    let (Ok(stdout), Ok(stderr)) = (
        String::from_utf8(output.stdout),
        String::from_utf8(output.stderr),
    ) else {
        operation.io_failed();
        return Err("process output was not UTF-8".to_owned());
    };
    operation.complete_status(output.code, output.success);
    Ok(CliOutput {
        success: output.success,
        exit_code: output.code,
        stdout,
        stderr,
    })
}

/// Read one bounded RPC frame at a time. Rendezvous delivery keeps unsolicited
/// output in the process pipe instead of retaining an unbounded message queue.
pub(super) fn read_rpc_frames(
    pipe: impl Read,
    tx: SyncSender<Result<String, String>>,
) {
    let mut reader = BufReader::new(pipe);
    loop {
        let mut bytes = Vec::new();
        let result = (&mut reader)
            .take((PROCESS_OUTPUT_MAX + 1) as u64)
            .read_until(b'\n', &mut bytes)
            .map_err(|_| "RPC output read failed".to_owned())
            .and_then(|count| {
                if count == 0 {
                    return Ok(None);
                }
                if count > PROCESS_OUTPUT_MAX {
                    return Err("RPC output exceeded limit".to_owned());
                }
                if bytes.last() == Some(&b'\n') {
                    bytes.pop();
                    if bytes.last() == Some(&b'\r') {
                        bytes.pop();
                    }
                }
                String::from_utf8(bytes)
                    .map(Some)
                    .map_err(|_| "RPC output was not UTF-8".to_owned())
            });
        let frame = match result {
            Ok(Some(frame)) => Ok(frame),
            Ok(None) => return,
            Err(error) => Err(error),
        };
        let failed = frame.is_err();
        if tx.send(frame).is_err() || failed {
            return;
        }
    }
}

pub(super) fn dollar_amounts(text: &str) -> Vec<f64> {
    let mut values = Vec::new();
    let mut rest = text;
    while let Some(index) = rest.find('$') {
        rest = &rest[index + 1..];
        let amount: String = rest
            .chars()
            .take_while(|ch| ch.is_ascii_digit() || matches!(ch, '.' | ','))
            .filter(|ch| *ch != ',')
            .collect();
        if let Ok(value) = amount.parse() {
            values.push(value);
        }
    }
    values
}

pub(super) fn percent_before_used(text: &str) -> Option<f64> {
    let before_used = text.split("% used").next()?;
    let percent = before_used
        .rsplit(|ch: char| !(ch.is_ascii_digit() || ch == '.'))
        .find(|part| !part.is_empty())?;
    percent.parse().ok()
}

pub(super) fn format_currency(value: f64) -> String {
    if value.fract().abs() < f64::EPSILON {
        format!("${value:.0}")
    } else {
        format!("${value:.2}")
    }
}

pub(super) fn format_cents(value: i64) -> String {
    format_currency(value as f64 / 100.0)
}

pub(super) fn codex_account_from_value(value: &serde_json::Value) -> Option<String> {
    value
        .pointer("/tokens/email")
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            value
                .pointer("/tokens/account_id")
                .and_then(serde_json::Value::as_str)
        })
        .or_else(|| value.get("auth_mode").and_then(serde_json::Value::as_str))
        .map(str::to_owned)
}
pub(super) fn first_string_key(value: &serde_json::Value, needle: &str) -> Option<String> {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(found) = map.get(needle).and_then(serde_json::Value::as_str) {
                return Some(found.to_owned());
            }
            map.values().find_map(|v| first_string_key(v, needle))
        }
        serde_json::Value::Array(values) => values.iter().find_map(|v| first_string_key(v, needle)),
        _ => None,
    }
}

pub(super) fn home_path(rel: &str) -> PathBuf {
    let rel = rel.trim_start_matches('/');
    std::env::var("HOME")
        .map_or_else(|_| PathBuf::from("/home/agent"), PathBuf::from)
        .join(rel)
}
pub(super) fn oauth_origin(path: &Path) -> String {
    // `to_string_lossy` borrows (no alloc) for the common UTF-8 path and only
    // allocates for non-UTF-8 container paths; `&Cow<str>` coerces to `&str`.
    format!(
        "OAuth · {}",
        jackin_core::shorten_home(&path.to_string_lossy())
    )
}
pub(super) fn titlecase_ascii(value: &str) -> String {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let mut out = String::new();
    out.extend(first.to_uppercase());
    out.push_str(chars.as_str());
    out
}
pub(super) fn compact_count(value: u64) -> String {
    if value >= 1_000_000_000 {
        format!("{:.1}B", value as f64 / 1_000_000_000.0)
    } else if value >= 1_000_000 {
        format!("{:.1}M", value as f64 / 1_000_000.0)
    } else if value >= 1_000 {
        format!("{:.1}K", value as f64 / 1_000.0)
    } else {
        value.to_string()
    }
}

/// Rust-owned, limits-only presentation of one quota bucket. Shared by the
/// Capsule usage dialog and every native Desktop surface so semantic segment
/// choice and order live in Rust, never in Swift or a per-surface copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageBucketPresentation {
    /// Provider percentage text (segment 0), when the bucket has one.
    pub remaining_label: Option<String>,
    /// Complete semantic segments in display order (a Rust pace composite is
    /// flattened onto the canonical `" · "` separator).
    pub display_segments: Vec<String>,
    /// `display_segments` joined with the canonical `" · "` separator.
    pub display_label: String,
    /// Percentage usable only as presentation geometry (meter fill): remaining
    /// on every slot, including Spend, so all capsule meters agree with the
    /// console windows. The Spend *text* still reads used (`{n}% used`).
    pub meter_percent: Option<u8>,
}

/// Stable human status label for a snapshot status (limits-only; no price).
#[must_use]
pub fn usage_display_status_label(
    status: jackin_protocol::control::UsageSnapshotStatus,
) -> &'static str {
    use jackin_protocol::control::UsageSnapshotStatus as S;
    match status {
        S::Fresh => "fresh",
        S::Stale => "stale",
        S::NeedsLogin => "needs login",
        S::NeedsSecret => "needs secret",
        S::Unsupported => "unsupported",
        S::Unavailable => "unavailable",
        S::Error => "error",
    }
}

/// Build the one provider/account/activity identity block consumed by Capsule
/// and native Desktop surfaces. All visible copy is complete before crossing
/// the FFI boundary.
#[must_use]
pub fn usage_identity_presentation(
    provider_title: &str,
    view: &jackin_protocol::control::FocusedUsageView,
    is_updating: bool,
) -> jackin_protocol::control::UsageIdentityPresentation {
    use jackin_protocol::control::{UsageActivityKind, UsageSnapshotStatus as Status};

    let account_label = if view.account.account_label.trim().is_empty() {
        "No authenticated account".to_owned()
    } else {
        view.account.account_label.clone()
    };
    let (activity_label, activity_kind) = if is_updating || view.is_refreshing_placeholder() {
        ("Updating…".to_owned(), UsageActivityKind::Updating)
    } else {
        match view.status {
            Status::Fresh => (view.updated_label.clone(), UsageActivityKind::Idle),
            Status::Stale => (
                format!("Update delayed · {}", view.updated_label),
                UsageActivityKind::Exceptional,
            ),
            Status::NeedsLogin => (
                "Sign in required".to_owned(),
                UsageActivityKind::Exceptional,
            ),
            Status::NeedsSecret => (
                "Credential required".to_owned(),
                UsageActivityKind::Exceptional,
            ),
            Status::Unsupported => (
                "Usage limits unsupported".to_owned(),
                UsageActivityKind::Exceptional,
            ),
            Status::Unavailable => (
                "Usage unavailable".to_owned(),
                UsageActivityKind::Exceptional,
            ),
            Status::Error => (
                format!("Update failed · {}", view.updated_label),
                UsageActivityKind::Exceptional,
            ),
        }
    };
    jackin_protocol::control::UsageIdentityPresentation {
        provider_title: provider_title.to_owned(),
        account_label: account_label.clone(),
        accessibility_label: format!("{provider_title}, {account_label}, {activity_label}"),
        activity_label,
        activity_kind,
    }
}

fn usage_money_cap_segment(
    used: Option<&str>,
    limit: Option<&str>,
    prefix: &str,
) -> Option<String> {
    match (used, limit) {
        (Some(used), Some(limit)) => Some(format!("{prefix}: {used} / {limit}")),
        (Some(label), None) | (None, Some(label)) => Some(label.to_owned()),
        (None, None) => None,
    }
}

fn count_quota_segments(counts: &jackin_protocol::control::CountQuota) -> [String; 2] {
    // Exact counts are independent observations; never derive remaining by
    // subtraction or recover quantities from formatted display labels.
    let usage = match (counts.used, counts.limit) {
        (Some(used), Some(limit)) => format!("{used} / {limit} requests used"),
        (Some(used), None) => format!("{used} requests used; limit unknown"),
        (None, Some(limit)) => format!("{limit} requests limit; usage unknown"),
        (None, None) => "Request usage and limit unknown".to_owned(),
    };
    let remaining = counts.remaining.map_or_else(
        || "Remaining requests unknown".to_owned(),
        |remaining| format!("{remaining} requests left"),
    );
    [usage, remaining]
}

/// Shared exact count summary for canonical, Console, native and Capsule rows.
#[must_use]
pub fn usage_count_quota_summary(counts: &jackin_protocol::control::CountQuota) -> String {
    count_quota_segments(counts).join(" · ")
}

/// Exact monetary observations; a missing cap is unknown rather than unlimited.
#[must_use]
pub fn usage_money_quota_summary(bucket: &jackin_protocol::control::QuotaBucketView) -> String {
    usage_money_amounts_summary(
        bucket.used_money.as_ref(),
        bucket.limit_money.as_ref(),
        bucket.remaining_money.as_ref(),
    )
}

/// Format the same exact observations from an account DTO or quota bucket.
#[must_use]
pub fn usage_money_amounts_summary(
    used: Option<&jackin_protocol::control::Money>,
    limit: Option<&jackin_protocol::control::Money>,
    remaining: Option<&jackin_protocol::control::Money>,
) -> String {
    let usage = match (used, limit) {
        (Some(used), Some(limit)) => format!("{used} / {limit} spent"),
        (Some(used), None) => format!("{used} spent · Cap unknown"),
        (None, Some(limit)) => format!("Cap {limit} · Spending unknown"),
        (None, None) => "Spending and cap unknown".to_owned(),
    };
    let remaining = remaining.cloned().or_else(|| limit?.checked_sub(used?));
    match remaining {
        Some(remaining) => format!("{usage} · {remaining} remaining"),
        None => format!("{usage} · Remaining allowance unknown"),
    }
}

/// Build the shared limits-only presentation for one quota bucket. The segment
/// choice/order matches the Capsule usage dialog exactly; the Capsule meter is
/// prepended by the caller from [`UsageBucketPresentation::meter_percent`].
#[must_use]
pub fn usage_bucket_presentation(
    bucket: &jackin_protocol::control::QuotaBucketView,
) -> UsageBucketPresentation {
    use jackin_protocol::control::{StatusSlot, UsageSnapshotStatus};

    let mut segments: Vec<String> = Vec::new();
    let mut remaining_label = None;
    let mut meter_percent = None;

    if let Some(counts) = &bucket.count_quota {
        let [usage, remaining] = count_quota_segments(counts);
        remaining_label = Some(remaining.clone());
        segments.extend([usage, remaining]);
        meter_percent = counts.remaining_percent();
        if let Some(pace) = &bucket.pace_label {
            segments.push(pace.clone());
        }
        if let Some(reset) = &bucket.reset_label {
            segments.push(reset.clone());
        }
        if bucket.status != UsageSnapshotStatus::Fresh {
            segments.push(usage_display_status_label(bucket.status).to_owned());
        }
    } else if bucket.used_money.is_some()
        || bucket.limit_money.is_some()
        || bucket.remaining_money.is_some()
    {
        let remaining = bucket.remaining_money.clone().or_else(|| {
            bucket
                .limit_money
                .as_ref()?
                .checked_sub(bucket.used_money.as_ref()?)
        });
        meter_percent = remaining
            .as_ref()
            .zip(bucket.limit_money.as_ref())
            .and_then(|(remaining, cap)| remaining.remaining_percent_of(cap));
        remaining_label = remaining.map(|remaining| format!("{remaining} remaining"));
        segments.push(usage_money_quota_summary(bucket));
        if let Some(reset) = &bucket.reset_label {
            segments.push(reset.clone());
        }
        if bucket.status != UsageSnapshotStatus::Fresh {
            segments.push(usage_display_status_label(bucket.status).to_owned());
        }
    } else if bucket.status_slot == Some(StatusSlot::Spend) {
        if let Some(remaining) = bucket.remaining_percent {
            // A remaining percent saturates at zero, so money over-100%
            // overage is invisible to it; the structured money ratio recovers
            // the raw magnitude through the same checked rule the projection
            // uses, keeping both surfaces on one "{raw}% used" text.
            let money_raw = bucket
                .used_money
                .as_ref()
                .and_then(|used| used.raw_percent_of(bucket.limit_money.as_ref()?));
            let used_raw: i32 = match money_raw {
                Some(raw) if raw > 100 => raw,
                _ => i32::from(100u8.saturating_sub(remaining)),
            };
            let segment = format!("{used_raw}% used");
            remaining_label = Some(segment.clone());
            segments.push(segment);
            meter_percent = Some(u8::try_from((100 - used_raw).clamp(0, 100)).unwrap_or(0));
        }
        if let Some(cap) = usage_money_cap_segment(
            bucket.used_label.as_deref(),
            bucket.limit_label.as_deref(),
            "Cap",
        ) {
            segments.push(cap);
        }
        if segments.is_empty() || bucket.status != UsageSnapshotStatus::Fresh {
            segments.push(usage_display_status_label(bucket.status).to_owned());
        }
    } else {
        if let Some(remaining) = bucket.remaining_percent {
            let segment =
                if bucket.label == "Credits" && remaining == 0 && bucket.limit_label.is_some() {
                    "0 left".to_owned()
                } else {
                    format!("{remaining}% left")
                };
            remaining_label = Some(segment.clone());
            segments.push(segment);
            meter_percent = Some(remaining);
        }
        if let Some(pace) = &bucket.pace_label {
            segments.push(pace.clone());
        }
        if let Some(reset) = &bucket.reset_label {
            segments.push(reset.clone());
        }
        if (bucket.used_money.is_some() || bucket.limit_money.is_some())
            && let Some(budget) = usage_money_cap_segment(
                bucket.used_label.as_deref(),
                bucket.limit_label.as_deref(),
                "Budget",
            )
        {
            segments.push(budget);
        } else if bucket.label == "Credits"
            && bucket.remaining_percent == Some(0)
            && let Some(limit) = &bucket.limit_label
        {
            segments.push(limit.clone());
        }
        // Balance-only quota (no percent, pace, reset, or money) surfaces its
        // limit label as the primary segment — the generic seam Grok's prepaid
        // balance consumes (plan 003). Buckets with any other segment are
        // unaffected, so existing Capsule output stays byte-identical.
        if segments.is_empty()
            && let Some(limit) = &bucket.limit_label
        {
            segments.push(limit.clone());
        }
        if segments.is_empty() || bucket.status != UsageSnapshotStatus::Fresh {
            segments.push(usage_display_status_label(bucket.status).to_owned());
        }
    }

    // Flatten a Rust pace composite (e.g. `"13% in deficit · Runs out in 2d"`)
    // onto the canonical separator so every segment is atomic.
    let display_segments: Vec<String> = segments
        .iter()
        .flat_map(|segment| segment.split(" · ").map(str::to_owned))
        .collect();
    let display_label = display_segments.join(" · ");
    UsageBucketPresentation {
        remaining_label,
        display_segments,
        display_label,
        meter_percent,
    }
}

/// One leading-only metadata line plus its `display_label`.
fn metadata_row(
    row_id: &str,
    label: &str,
    value: String,
) -> jackin_protocol::control::UsageDetailRow {
    jackin_protocol::control::UsageDetailRow {
        row_id: row_id.to_owned(),
        kind: jackin_protocol::control::UsageDetailRowKind::Metadata,
        label: label.to_owned(),
        display_label: value.clone(),
        layout_lines: vec![jackin_protocol::control::UsagePresentationLine {
            leading: Some(value),
            trailing: None,
        }],
        meter_percent: None,
        severity: jackin_protocol::control::UsageSeverity::Normal,
    }
}

/// Build the single Rust-owned provider-detail card shared by the Capsule usage
/// dialog and the native Desktop Usage window. Identity, activity, and ordinary
/// freshness live in the separate identity presentation. This card emits only
/// distinct `username`/`plan`/`auth`, one `bucket:<zero-based index>` per source bucket
/// (so duplicate provider labels stay distinct), then optional `detail`
/// (`last_error`, appended after the last-good bucket rows — errors never
/// replace data). Every visible string is produced here; consumers render the
/// rows mechanically.
#[must_use]
pub fn usage_detail_presentation(
    view: &jackin_protocol::control::FocusedUsageView,
) -> jackin_protocol::control::UsageDetailPresentation {
    use jackin_protocol::control::{UsageDetailRow, UsageDetailRowKind, UsagePresentationLine};

    let mut rows = Vec::new();
    if let Some(username) = &view.account.username
        && username.trim() != view.account.account_label.trim()
    {
        rows.push(metadata_row("username", "Username", username.clone()));
    }
    if let Some(plan) = &view.account.plan_label {
        rows.push(metadata_row("plan", "Plan", plan.clone()));
    }
    if let Some(origin) = &view.account.credential_origin {
        rows.push(metadata_row("auth", "Auth", origin.clone()));
    }

    for (index, bucket) in view.buckets.iter().enumerate() {
        let presentation = usage_bucket_presentation(bucket);
        // Canonical semantic order is already flattened in `display_segments`
        // (remaining, pace/run-out, reset, quota-bound/status). The reset
        // segment moves to the trailing column so the window can right-align it;
        // every other segment is a leading line. Order — and therefore the
        // joined `display_label` — is preserved either way.
        let layout_lines: Vec<UsagePresentationLine> = presentation
            .display_segments
            .iter()
            .map(|segment| {
                if bucket.reset_label.as_deref() == Some(segment.as_str()) {
                    UsagePresentationLine {
                        leading: None,
                        trailing: Some(segment.clone()),
                    }
                } else {
                    UsagePresentationLine {
                        leading: Some(segment.clone()),
                        trailing: None,
                    }
                }
            })
            .collect();
        rows.push(UsageDetailRow {
            row_id: format!("bucket:{index}"),
            kind: UsageDetailRowKind::Bucket,
            label: bucket.label.clone(),
            display_label: presentation.display_label,
            layout_lines,
            meter_percent: presentation.meter_percent,
            severity: bucket.severity,
        });
    }

    if let Some(error) = &view.last_error {
        let mut row = metadata_row("detail", "Detail", error.clone());
        row.kind = UsageDetailRowKind::Detail;
        rows.push(row);
    }

    jackin_protocol::control::UsageDetailPresentation { rows }
}
