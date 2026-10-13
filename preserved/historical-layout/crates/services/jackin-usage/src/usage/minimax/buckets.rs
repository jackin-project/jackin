// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `MiniMax` quota buckets and window labels.

use super::super::{
    QuotaBucketView, StatusSlot, UsageSnapshotStatus, compact_count, timed_bucket, titlecase_ascii,
};

use super::minimax_reset_epoch;

#[derive(Debug, Clone, Copy)]
pub(crate) enum MiniMaxWindow {
    Interval,
    Weekly,
}

/// Window status codes: `0`/`1` normal, `2` exhausted, `3` unlimited.
pub(crate) fn minimax_window_exhausted(status: Option<i64>) -> bool {
    status == Some(2)
}

pub(crate) fn minimax_window_unlimited(status: Option<i64>) -> bool {
    status == Some(3)
}

#[expect(
    clippy::too_many_arguments,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) fn minimax_bucket(
    model_name: &str,
    window: MiniMaxWindow,
    total: Option<i64>,
    usage: Option<i64>,
    remaining_percent: Option<f64>,
    boost_permille: Option<f64>,
    status: Option<i64>,
    end: Option<i64>,
    remains_time: Option<i64>,
    now: i64,
) -> Option<QuotaBucketView> {
    // Only the general model fills the status-bar slots; per-model windows are
    // detail rows the headline ignores.
    let status_slot = match (minimax_is_general_model(Some(model_name)), window) {
        (true, MiniMaxWindow::Interval) => Some(StatusSlot::Session),
        (true, MiniMaxWindow::Weekly) => Some(StatusSlot::Weekly),
        _ => None,
    };
    let reset_epoch = minimax_reset_epoch(end, remains_time, now);
    if minimax_window_unlimited(status) {
        let mut view = timed_bucket(
            &minimax_bucket_label(model_name, window),
            usage.map(|usage| compact_count(u64::try_from(usage.max(0)).unwrap_or(0))),
            None,
            None,
            reset_epoch,
            now,
            Some("Unlimited"),
            UsageSnapshotStatus::Fresh,
        );
        view.status_slot = status_slot;
        return Some(view);
    }
    if matches!(status, Some(value) if !matches!(value, 0..=2)) {
        return None;
    }
    if total.is_none() && usage.is_none() && remaining_percent.is_none() {
        return None;
    }
    let remaining_percent = if minimax_window_exhausted(status) {
        Some(0)
    } else if let Some(remaining_percent) = remaining_percent {
        minimax_effective_remaining(Some(remaining_percent), boost_permille)
    } else {
        let total = total?;
        if total <= 0 {
            None
        } else {
            let usage = usage?;
            minimax_effective_remaining(
                Some(100.0 - (usage.clamp(0, total) as f64 / total as f64) * 100.0),
                boost_permille,
            )
        }
    };
    let used_label = usage.map(|usage| compact_count(u64::try_from(usage.max(0)).unwrap_or(0)));
    let mut pace = minimax_usage_count_line(usage, total, remaining_percent);
    if let Some(note) = minimax_boost_note(boost_permille) {
        pace = Some(match pace {
            Some(line) => format!("{line} · {note}"),
            None => note,
        });
    }
    if minimax_window_exhausted(status) {
        pace = Some(match pace {
            Some(line) => format!("{line} · Exhausted"),
            None => "Exhausted".to_owned(),
        });
    }
    let mut view = timed_bucket(
        &minimax_bucket_label(model_name, window),
        used_label,
        total
            .filter(|value| *value > 0)
            .map(|value| compact_count(u64::try_from(value.max(0)).unwrap_or(0))),
        remaining_percent,
        reset_epoch,
        now,
        pace.as_deref(),
        UsageSnapshotStatus::Fresh,
    );
    view.status_slot = status_slot;
    Some(view)
}

/// Apply the interval/weekly boost: rendered remaining = base ×
/// (`boost_permille` / 1000), which can exceed 100%. The raw over-cap value is
/// kept (bar geometry clamps at render); only the `u8` carrier bounds it.
pub(crate) fn minimax_effective_remaining(
    base_percent: Option<f64>,
    boost_permille: Option<f64>,
) -> Option<u8> {
    let base = base_percent
        .filter(|base| base.is_finite())
        .map(|base| base.max(0.0))?;
    let scaled = match boost_permille.filter(|boost| boost.is_finite() && *boost > 0.0) {
        Some(boost) => base * boost / 1000.0,
        None => base,
    };
    #[expect(
        clippy::cast_sign_loss,
        reason = "base clamped non-negative and boost positive; rounded f64→u8"
    )]
    {
        Some(scaled.round().clamp(0.0, 255.0) as u8)
    }
}

/// Human note for a non-trivial boost, e.g. `+20% boost` for permille 1200.
pub(crate) fn minimax_boost_note(boost_permille: Option<f64>) -> Option<String> {
    let boost = boost_permille.filter(|boost| boost.is_finite() && *boost > 0.0)?;
    if (boost - 1000.0).abs() < f64::EPSILON {
        return None;
    }
    if boost > 1000.0 {
        Some(format!("+{}% boost", (boost / 10.0 - 100.0).round() as i64))
    } else {
        Some(format!("{}% of base", (boost / 10.0).round() as i64))
    }
}

pub(crate) fn minimax_is_general_model(model_name: Option<&str>) -> bool {
    model_name.is_some_and(|value| value.eq_ignore_ascii_case("general"))
}

pub(crate) fn minimax_bucket_label(model_name: &str, window: MiniMaxWindow) -> String {
    let model = titlecase_ascii(model_name);
    match (minimax_is_general_model(Some(model_name)), window) {
        (true, MiniMaxWindow::Interval) => "General · 5h".to_owned(),
        (true, MiniMaxWindow::Weekly) => "General · Weekly".to_owned(),
        (false, MiniMaxWindow::Interval) => model,
        (false, MiniMaxWindow::Weekly) => format!("{model} · Weekly"),
    }
}

pub(crate) fn minimax_usage_count_line(
    usage: Option<i64>,
    total: Option<i64>,
    remaining_percent: Option<u8>,
) -> Option<String> {
    let usage = u64::try_from(usage?.max(0)).unwrap_or(0);
    let total = total.filter(|value| *value > 0).map_or_else(
        || remaining_percent.map(|_| 100),
        |value| Some(u64::try_from(value.max(0)).unwrap_or(0)),
    )?;
    Some(format!(
        "Usage: {} / {}",
        compact_count(usage),
        compact_count(total)
    ))
}
