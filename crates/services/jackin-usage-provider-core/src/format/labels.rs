// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use chrono::{Local, TimeZone};
use jackin_protocol::control::CanonicalRemainingPercent;

use super::{format_percent_points, titlecase_ascii};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PercentStyle {
    #[default]
    Left,
    Used,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResetStyle {
    #[default]
    Countdown,
    ExactClock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct UsageFormatPrefs {
    pub percent_style: PercentStyle,
    pub reset_style: ResetStyle,
}

pub fn reset_label(reset_at: i64, now: i64) -> String {
    reset_label_with_prefs(reset_at, now, UsageFormatPrefs::default())
}

pub fn reset_label_with_prefs(reset_at: i64, now: i64, prefs: UsageFormatPrefs) -> String {
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

pub fn exact_reset_parenthetical(reset_at: i64) -> String {
    format!("({})", local_timestamp_label(reset_at))
}

pub fn percent_headline(remaining: CanonicalRemainingPercent, prefs: UsageFormatPrefs) -> String {
    match prefs.percent_style {
        PercentStyle::Left => remaining.label(),
        PercentStyle::Used => format!("{}% used", format_percent_points(remaining.used_points())),
    }
}

pub fn expiry_label(expires_at: i64, now: i64) -> String {
    if expires_at <= now {
        return "now".to_owned();
    }
    format!(
        "in {} ({})",
        compact_duration_label(expires_at.saturating_sub(now).max(0)),
        local_timestamp_label(expires_at)
    )
}

pub fn local_timestamp_label(epoch: i64) -> String {
    Local.timestamp_opt(epoch, 0).single().map_or_else(
        || "local time unavailable".to_owned(),
        |timestamp| timestamp.format("%b %-d, %H:%M").to_string(),
    )
}

pub fn quota_pace_label(
    remaining_percent: Option<CanonicalRemainingPercent>,
    reset_at: Option<i64>,
    window_seconds: Option<i64>,
    now: i64,
) -> Option<String> {
    let remaining_percent = remaining_percent?.get();
    let reset_in = reset_at?.saturating_sub(now).max(0);
    let window_seconds = window_seconds?.max(1);
    if reset_in > window_seconds {
        return None;
    }
    let time_left_percent = reset_in as f64 / window_seconds as f64 * 100.0;
    let delta = remaining_percent - time_left_percent;
    let pace = if delta.abs() <= 2.0 {
        "On pace".to_owned()
    } else if delta > 0.0 {
        format!("{}% in reserve", format_percent_points(delta))
    } else {
        format!("{}% in deficit", format_percent_points(-delta))
    };
    let used = 100.0 - remaining_percent;
    let elapsed = (window_seconds - reset_in) as f64;
    let behind_clock = (remaining_percent * window_seconds as f64) < reset_in as f64 * 100.0;
    if used > 0.0 && elapsed > 0.0 && behind_clock {
        let display_seconds = remaining_percent * elapsed / used;
        if display_seconds.is_finite() && display_seconds <= i64::MAX as f64 {
            return Some(format!(
                "{pace} · Runs out in {}",
                compact_duration_label(display_seconds.round() as i64)
            ));
        }
    }
    Some(pace)
}

pub fn compact_duration_label(seconds: i64) -> String {
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

pub fn window_minutes_label(minutes: i64) -> Option<String> {
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

pub fn humanize_words_with(value: &str, word: impl Fn(&str) -> String) -> String {
    value
        .split(|c: char| c == '_' || c == '-' || c.is_whitespace())
        .filter(|part| !part.is_empty())
        .map(word)
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn humanize_plan_label(value: &str) -> String {
    humanize_words_with(value, titlecase_ascii)
}

pub fn codex_limit_label(value: &str) -> String {
    let lower = value.to_ascii_lowercase();
    if lower.contains("spark") {
        "Codex Spark".to_owned()
    } else {
        humanize_plan_label(value)
    }
}
