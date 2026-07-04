// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Formatting, CLI, and JSON helpers shared by every usage provider.
//!
//! Extracted from `usage.rs` as Phase 2 of codebase-health-enforcement
//! (Workstream C, file-size ratchet). Lives in a sibling module so the
//! provider-specific sections in `usage.rs` only carry their own logic,
//! not the shared display/parsing utilities every provider depends on.
//!
//! Visibility is `pub(super)` so the coordinator can still call every
//! helper directly; tests under `usage/tests.rs` see them through
//! `super::*` and do not need their own re-exports.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use chrono::{DateTime, Local, TimeZone, Utc};

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
    Some(used.round().clamp(0.0, f64::from(u16::MAX)) as u16)
}

pub(super) fn parse_iso_epoch(value: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|date| date.with_timezone(&Utc).timestamp())
}

pub(super) fn reset_label(reset_at: i64, now: i64) -> String {
    if reset_at <= now {
        return "Resets now".to_owned();
    }
    format!(
        "Resets in {} ({})",
        compact_duration_label(reset_at.saturating_sub(now).max(0)),
        local_timestamp_label(reset_at)
    )
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

pub(super) fn local_timestamp_label(epoch: i64) -> String {
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
    let remaining_percent = f64::from(remaining_percent?);
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
    if delta.abs() <= 2.0 {
        Some("On pace".to_owned())
    } else if delta > 0.0 {
        Some(format!("{}% in reserve", delta.round() as i64))
    } else {
        Some(format!("{}% in deficit", (-delta).round() as i64))
    }
}

pub(super) fn compact_duration_label(seconds: i64) -> String {
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;
    if days > 0 {
        if hours > 0 {
            format!("{days}d {hours}h")
        } else {
            format!("{days}d")
        }
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

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

#[allow(clippy::disallowed_methods)]
pub(super) fn run_cli_with_timeout_full(
    command: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<CliOutput, String> {
    let mut child = Command::new(command)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("{command} failed to start: {err}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| format!("{command} stdout unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| format!("{command} stderr unavailable"))?;
    let stdout_reader = thread::spawn(move || read_process_pipe(stdout));
    let stderr_reader = thread::spawn(move || read_process_pipe(stderr));
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return collect_cli_output(command, Some(status), stdout_reader, stderr_reader);
            }
            Ok(None) if started.elapsed() >= timeout => {
                drop(child.kill());
                drop(child.wait());
                return Err(format!("{command} timed out after {}s", timeout.as_secs()));
            }
            Ok(None) => thread::sleep(Duration::from_millis(50)),
            Err(err) if err.raw_os_error() == Some(nix::errno::Errno::ECHILD as i32) => {
                return collect_cli_output(command, None, stdout_reader, stderr_reader);
            }
            Err(err) => {
                drop(child.kill());
                drop(child.wait());
                return Err(format!("{command} status failed: {err}"));
            }
        }
    }
}

pub(super) fn collect_cli_output(
    command: &str,
    status: Option<ExitStatus>,
    stdout_reader: thread::JoinHandle<Result<String, String>>,
    stderr_reader: thread::JoinHandle<Result<String, String>>,
) -> Result<CliOutput, String> {
    let stdout = stdout_reader
        .join()
        .map_err(|_| format!("{command} stdout reader panicked"))?;
    let stderr = stderr_reader
        .join()
        .map_err(|_| format!("{command} stderr reader panicked"))?;
    Ok(CliOutput {
        success: status.is_none_or(|status| status.success()),
        exit_code: status.and_then(|status| status.code()),
        stdout: stdout?,
        stderr: stderr?,
    })
}

pub(super) fn read_process_pipe(mut pipe: impl Read) -> Result<String, String> {
    let mut bytes = Vec::new();
    pipe.read_to_end(&mut bytes)
        .map_err(|err| format!("process output read failed: {err}"))?;
    String::from_utf8(bytes).map_err(|err| format!("process output was not UTF-8: {err}"))
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

pub(super) fn first_number_key(value: &serde_json::Value, needle: &str) -> Option<f64> {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(found) = map.get(needle).and_then(json_number) {
                return Some(found);
            }
            map.values().find_map(|v| first_number_key(v, needle))
        }
        serde_json::Value::Array(values) => values.iter().find_map(|v| first_number_key(v, needle)),
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
