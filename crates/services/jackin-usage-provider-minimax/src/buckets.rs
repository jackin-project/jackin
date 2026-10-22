// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::control::{
    CanonicalRemainingPercent, CountQuota, CountQuotaPeriod, CountQuotaProvenance, CountQuotaUnit,
    PercentObservationOrigin, ProviderQuotaState, QuotaBucketView, StatusSlot, UsageSnapshotStatus,
};
use jackin_usage_provider_core::{compact_count, timed_bucket, titlecase_ascii};

use super::minimax_reset_epoch;

#[derive(Debug, Clone, Copy)]
pub enum MiniMaxWindow {
    Interval,
    Weekly,
}

pub(crate) struct MiniMaxRawBucketInput<'a> {
    pub(crate) model_name: &'a str,
    pub(crate) window: MiniMaxWindow,
    pub(crate) total: Option<&'a serde_json::Value>,
    pub(crate) usage: Option<&'a serde_json::Value>,
    pub(crate) remaining_percent: Option<&'a serde_json::Value>,
    pub(crate) boost_permille: Option<f64>,
    pub(crate) status: Option<&'a serde_json::Value>,
    pub(crate) end: Option<i64>,
    pub(crate) remains_time: Option<i64>,
    pub(crate) now: i64,
}

struct MiniMaxBucketInput<'a> {
    model_name: &'a str,
    window: MiniMaxWindow,
    total: Option<u64>,
    usage: Option<u64>,
    remaining_percent: Option<f64>,
    boost_permille: Option<f64>,
    status: Option<i64>,
    end: Option<i64>,
    remains_time: Option<i64>,
    now: i64,
    diagnostic: Option<&'a str>,
}

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
pub fn minimax_bucket(
    model_name: &str,
    window: MiniMaxWindow,
    total: Option<u64>,
    usage: Option<u64>,
    remaining_percent: Option<f64>,
    boost_permille: Option<f64>,
    status: Option<i64>,
    end: Option<i64>,
    remains_time: Option<i64>,
    now: i64,
) -> Option<QuotaBucketView> {
    minimax_bucket_observations(MiniMaxBucketInput {
        model_name,
        window,
        total,
        usage,
        remaining_percent,
        boost_permille,
        status,
        end,
        remains_time,
        now,
        diagnostic: None,
    })
}

pub(crate) fn minimax_bucket_from_raw(input: MiniMaxRawBucketInput<'_>) -> Option<QuotaBucketView> {
    let MiniMaxRawBucketInput {
        model_name,
        window,
        total,
        usage,
        remaining_percent,
        boost_permille,
        status,
        end,
        remains_time,
        now,
    } = input;
    let (total, total_invalid) = minimax_count(total);
    let (usage, usage_invalid) = minimax_count(usage);
    let (remaining_percent, percent_invalid) = minimax_percent(remaining_percent);
    let (status, status_invalid) = minimax_status(status);
    let mut invalid = Vec::new();
    if total_invalid {
        invalid.push("total_count");
    }
    if usage_invalid {
        invalid.push("usage_count");
    }
    if percent_invalid {
        invalid.push("remaining_percent");
    }
    if status_invalid {
        invalid.push("status");
    }
    let diagnostic = (!invalid.is_empty()).then(|| {
        format!(
            "invalid provider field{}: {}",
            if invalid.len() == 1 { "" } else { "s" },
            invalid.join(", ")
        )
    });
    minimax_bucket_observations(MiniMaxBucketInput {
        model_name,
        window,
        total,
        usage,
        remaining_percent,
        boost_permille,
        status,
        end,
        remains_time,
        now,
        diagnostic: diagnostic.as_deref(),
    })
}

fn minimax_bucket_observations(input: MiniMaxBucketInput<'_>) -> Option<QuotaBucketView> {
    let MiniMaxBucketInput {
        model_name,
        window,
        total,
        usage,
        remaining_percent,
        boost_permille,
        status,
        end,
        remains_time,
        now,
        diagnostic,
    } = input;
    let status_slot = match (minimax_is_general_model(Some(model_name)), window) {
        (true, MiniMaxWindow::Interval) => Some(StatusSlot::Session),
        (true, MiniMaxWindow::Weekly) => Some(StatusSlot::Weekly),
        _ => None,
    };
    let reset_epoch = minimax_reset_epoch(end, remains_time, now);
    let reported_remaining = remaining_percent
        .and_then(|points| CanonicalRemainingPercent::new(points).ok())
        .map(|percent| (percent, PercentObservationOrigin::ProviderReported));
    let provider_quota_state = if minimax_window_exhausted(status) {
        Some(ProviderQuotaState::Exhausted)
    } else if minimax_window_unlimited(status) {
        Some(ProviderQuotaState::Unlimited)
    } else {
        None
    };
    let quantitative_observation =
        total.is_some() || usage.is_some() || remaining_percent.is_some();
    if !quantitative_observation && provider_quota_state.is_none() && diagnostic.is_none() {
        return None;
    }
    let used_label = usage.map(compact_count);
    let mut pace = minimax_usage_count_line(usage, total);
    if let Some(note) = minimax_boost_note(boost_permille) {
        pace = Some(match pace {
            Some(line) => format!("{line} · {note}"),
            None => note,
        });
    }
    if let Some(diagnostic) = diagnostic {
        pace = Some(match pace {
            Some(line) => format!("{line} · {diagnostic}"),
            None => diagnostic.to_owned(),
        });
    }
    let mut view = timed_bucket(
        &minimax_bucket_label(model_name, window),
        used_label,
        total.map(compact_count),
        reported_remaining,
        reset_epoch,
        now,
        pace.as_deref(),
        if quantitative_observation || provider_quota_state.is_some() {
            UsageSnapshotStatus::Fresh
        } else {
            UsageSnapshotStatus::Unavailable
        },
    );
    view.status_slot = status_slot;
    view.provider_quota_state = provider_quota_state;
    if total.is_some() || usage.is_some() {
        view.count_quota = Some(CountQuota::new(
            usage,
            total,
            None,
            CountQuotaUnit::Unknown,
            CountQuotaPeriod::Unknown,
            CountQuotaProvenance::ProviderReported,
        ));
    }
    Some(view)
}

fn minimax_count(value: Option<&serde_json::Value>) -> (Option<u64>, bool) {
    let Some(value) = value else {
        return (None, false);
    };
    if value.is_null() {
        return (None, false);
    }
    let parsed = value.as_u64().or_else(|| {
        value
            .as_str()
            .and_then(|text| text.trim().parse::<u64>().ok())
    });
    (parsed, parsed.is_none())
}

fn minimax_percent(value: Option<&serde_json::Value>) -> (Option<f64>, bool) {
    let Some(value) = value else {
        return (None, false);
    };
    if value.is_null() {
        return (None, false);
    }
    let parsed = value
        .as_f64()
        .filter(|value| CanonicalRemainingPercent::new(*value).is_ok());
    (parsed, parsed.is_none())
}

fn minimax_status(value: Option<&serde_json::Value>) -> (Option<i64>, bool) {
    let Some(value) = value else {
        return (None, false);
    };
    if value.is_null() {
        return (None, false);
    }
    let parsed = value.as_i64();
    (parsed, parsed.is_none_or(|status| !matches!(status, 0..=3)))
}

pub(crate) fn minimax_effective_remaining(
    base_percent: Option<f64>,
    boost_permille: Option<f64>,
) -> Option<f64> {
    let base = base_percent
        .filter(|base| base.is_finite())
        .map(|base| base.max(0.0))?;
    let scaled = match boost_permille.filter(|boost| boost.is_finite() && *boost > 0.0) {
        Some(boost) => base * boost / 1000.0,
        None => base,
    };
    scaled.is_finite().then_some(scaled)
}

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

pub fn minimax_is_general_model(model_name: Option<&str>) -> bool {
    model_name.is_some_and(|value| value.eq_ignore_ascii_case("general"))
}

pub fn minimax_bucket_label(model_name: &str, window: MiniMaxWindow) -> String {
    let model = titlecase_ascii(model_name);
    match (minimax_is_general_model(Some(model_name)), window) {
        (true, MiniMaxWindow::Interval) => "General · 5h".to_owned(),
        (true, MiniMaxWindow::Weekly) => "General · Weekly".to_owned(),
        (false, MiniMaxWindow::Interval) => model,
        (false, MiniMaxWindow::Weekly) => format!("{model} · Weekly"),
    }
}

pub fn minimax_usage_count_line(usage: Option<u64>, total: Option<u64>) -> Option<String> {
    let usage = usage?;
    let total = total.filter(|value| *value > 0)?;
    Some(format!(
        "Usage: {} / {}",
        compact_count(usage),
        compact_count(total)
    ))
}
