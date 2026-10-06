// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Format preferences, time labels, and text helpers.

use chrono::{Local, TimeZone};

use super::titlecase_ascii;

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

pub(crate) fn reset_label(reset_at: i64, now: i64) -> String {
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

pub(crate) fn expiry_label(expires_at: i64, now: i64) -> String {
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

pub(crate) fn quota_pace_label(
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

pub(crate) fn window_minutes_label(minutes: i64) -> Option<String> {
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
pub(crate) fn humanize_words_with(value: &str, word: impl Fn(&str) -> String) -> String {
    value
        .split(|c: char| c == '_' || c == '-' || c.is_whitespace())
        .filter(|part| !part.is_empty())
        .map(word)
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn humanize_plan_label(value: &str) -> String {
    humanize_words_with(value, titlecase_ascii)
}

pub(crate) fn codex_limit_label(value: &str) -> String {
    let lower = value.to_ascii_lowercase();
    if lower.contains("spark") {
        "Codex Spark".to_owned()
    } else {
        humanize_plan_label(value)
    }
}
