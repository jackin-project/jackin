// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::control::{PercentObservationOrigin, RawPercentPoints};
use jackin_usage_provider_core::{
    epoch_seconds_from_maybe_ms, json_number, parse_iso_epoch, quota_source_remaining_rank_points,
};

use super::{
    AntigravityFamily, AntigravityPool, AntigravityUsage, AntigravityUtilizationObservation,
    AntigravityWindow, antigravity_identity_from_value, antigravity_plan_from_value,
};

pub fn parse_antigravity_usage_output(text: &str) -> Result<AntigravityUsage, String> {
    let value: serde_json::Value = serde_json::from_str(text.trim())
        .map_err(|_| "Antigravity /usage output was not recognized".to_owned())?;
    let identity = antigravity_identity_from_value(&value);
    let plan = antigravity_plan_from_value(&value);
    if let Some(entries) = antigravity_summary_entries(&value) {
        let pools = entries
            .iter()
            .filter_map(antigravity_pool_from_summary_entry)
            .collect();
        return Ok(AntigravityUsage {
            pools,
            identity,
            plan,
            legacy_fallback: false,
        });
    }
    let pools = antigravity_pools_from_legacy_models(&value);
    Ok(AntigravityUsage {
        pools,
        identity,
        plan,
        legacy_fallback: true,
    })
}

fn antigravity_summary_entries(value: &serde_json::Value) -> Option<Vec<serde_json::Value>> {
    for groups in [
        value.get("groups"),
        value.get("response").and_then(|inner| inner.get("groups")),
        value
            .get("command")
            .and_then(|command| command.get("data"))
            .and_then(|data| data.get("groups")),
    ] {
        if let Some(groups) = groups.and_then(serde_json::Value::as_array) {
            let entries = groups
                .iter()
                .filter_map(|group| group.get("buckets"))
                .filter_map(serde_json::Value::as_array)
                .flatten()
                .cloned()
                .collect::<Vec<_>>();
            return Some(entries);
        }
    }
    for key in ["pools", "buckets", "quotaBuckets"] {
        if let Some(entries) = value.get(key).and_then(serde_json::Value::as_array) {
            return Some(entries.clone());
        }
    }
    None
}

fn antigravity_pool_from_summary_entry(entry: &serde_json::Value) -> Option<AntigravityPool> {
    let id = ["bucketId", "bucket_id", "id", "poolId", "pool", "name"]
        .into_iter()
        .filter_map(|key| entry.get(key).and_then(serde_json::Value::as_str))
        .map(str::trim)
        .find(|id| !id.is_empty())?;
    let (used_percent, remaining_percent) = antigravity_percentages_from_entry(entry);
    let reset_at = antigravity_reset_from_entry(entry);
    let (family, window) = match id {
        "gemini-5h" => (AntigravityFamily::Gemini, Some(AntigravityWindow::Session)),
        "gemini-weekly" => (AntigravityFamily::Gemini, Some(AntigravityWindow::Weekly)),
        "3p-5h" => (AntigravityFamily::Other, Some(AntigravityWindow::Session)),
        "3p-weekly" => (AntigravityFamily::Other, Some(AntigravityWindow::Weekly)),
        _ => {
            let lower = id.to_ascii_lowercase();
            let family = if lower.contains("gemini") {
                AntigravityFamily::Gemini
            } else {
                AntigravityFamily::Other
            };
            let window = if lower.contains("week") {
                Some(AntigravityWindow::Weekly)
            } else if lower.contains("5h")
                || lower.contains("5-h")
                || lower.contains("session")
                || lower.contains("hour")
            {
                Some(AntigravityWindow::Session)
            } else {
                None
            };
            (family, window)
        }
    };
    Some(AntigravityPool {
        family,
        window,
        used_percent,
        remaining_percent,
        used_percent_origin: used_percent.map(|_| PercentObservationOrigin::ProviderReported),
        remaining_percent_origin: remaining_percent
            .map(|_| PercentObservationOrigin::ProviderReported),
        reset_at,
        source_label: Some(id.to_owned()),
        unclassified_utilizations: antigravity_unclassified_utilization_from_entry(entry)
            .map(|value| {
                vec![AntigravityUtilizationObservation {
                    source: id.to_owned(),
                    value,
                    reset_at,
                }]
            })
            .unwrap_or_default(),
    })
}

pub fn antigravity_remaining_from_entry(entry: &serde_json::Value) -> Option<f64> {
    let quota = entry.get("quotaInfo").unwrap_or(entry);
    for (key, is_fraction) in [
        ("remainingFraction", true),
        ("remaining_fraction", true),
        ("remainingPercent", false),
        ("remaining_percent", false),
    ] {
        if let Some(raw) = quota.get(key).and_then(json_number)
            && raw.is_finite()
            && raw >= 0.0
        {
            let points = if is_fraction { raw * 100.0 } else { raw };
            return points.is_finite().then_some(points);
        }
    }
    None
}

fn antigravity_used_from_entry(entry: &serde_json::Value) -> Option<f64> {
    let quota = entry.get("quotaInfo").unwrap_or(entry);
    for key in ["usedPercent", "used_percent"] {
        if let Some(raw) = quota.get(key).and_then(json_number)
            && raw.is_finite()
            && raw >= 0.0
        {
            return Some(raw);
        }
    }
    None
}

pub(super) fn antigravity_unclassified_utilization_from_entry(
    entry: &serde_json::Value,
) -> Option<serde_json::Number> {
    let quota = entry.get("quotaInfo").unwrap_or(entry);
    let value = quota.get("utilization")?;
    value.as_f64().filter(|value| value.is_finite())?;
    value.as_number().cloned()
}

pub(super) fn antigravity_percentages_from_entry(
    entry: &serde_json::Value,
) -> (Option<f64>, Option<f64>) {
    (
        antigravity_used_from_entry(entry),
        antigravity_remaining_from_entry(entry),
    )
}

fn antigravity_reset_from_entry(entry: &serde_json::Value) -> Option<i64> {
    let quota = entry.get("quotaInfo").unwrap_or(entry);
    for key in [
        "resetTime",
        "reset_time",
        "resetsAt",
        "resets_at",
        "reset_at",
        "reset",
    ] {
        if let Some(node) = quota.get(key) {
            if let Some(text) = node.as_str()
                && let Some(epoch) = parse_iso_epoch(text.trim())
            {
                return Some(epoch);
            }
            if let Some(number) = json_number(node) {
                return Some(epoch_seconds_from_maybe_ms(number.floor() as i64));
            }
        }
    }
    None
}

fn antigravity_pools_from_legacy_models(value: &serde_json::Value) -> Vec<AntigravityPool> {
    let Some(models) = value.get("models").and_then(serde_json::Value::as_object) else {
        return Vec::new();
    };
    let mut worst: [Option<LegacyPoolChoice>; 2] = [None, None];
    let mut unclassified_utilizations: [Vec<AntigravityUtilizationObservation>; 2] =
        [Vec::new(), Vec::new()];
    let mut entries = models.iter().collect::<Vec<_>>();
    entries.sort_by_key(|(left, _)| *left);
    for (id, model) in entries {
        if model.get("isInternal").and_then(serde_json::Value::as_bool) == Some(true) {
            continue;
        }
        let label = ["displayName", "label", "model"]
            .into_iter()
            .filter_map(|key| model.get(key).and_then(serde_json::Value::as_str))
            .map(str::trim)
            .find(|label| !label.is_empty());
        let name = format!("{id} {}", label.unwrap_or_default()).to_ascii_lowercase();
        let slot = usize::from(!name.contains("gemini"));
        if let Some(utilization) = antigravity_unclassified_utilization_from_entry(model) {
            unclassified_utilizations[slot].push(AntigravityUtilizationObservation {
                source: id.to_owned(),
                value: utilization,
                reset_at: antigravity_reset_from_entry(model),
            });
        }
        if label.is_none() {
            continue;
        }
        let (used, remaining) = antigravity_percentages_from_entry(model);
        let raw_used = used.and_then(|value| RawPercentPoints::new(value).ok());
        let raw_remaining = remaining.and_then(|value| RawPercentPoints::new(value).ok());
        let Some(score) = quota_source_remaining_rank_points(
            None,
            raw_used,
            used.map(|_| PercentObservationOrigin::ProviderReported),
            raw_remaining,
            remaining.map(|_| PercentObservationOrigin::ProviderReported),
            None,
            None,
        ) else {
            continue;
        };
        let reset_at = antigravity_reset_from_entry(model);
        let replace = worst[slot].as_ref().is_none_or(|current| {
            score < current.score
                || (score.partial_cmp(&current.score) == Some(std::cmp::Ordering::Equal)
                    && id.as_str() < current.id.as_str())
        });
        if replace {
            worst[slot] = Some(LegacyPoolChoice {
                id: id.to_owned(),
                score,
                used,
                remaining,
                reset_at,
            });
        }
    }
    [
        (AntigravityFamily::Gemini, worst[0].take()),
        (AntigravityFamily::Other, worst[1].take()),
    ]
    .into_iter()
    .enumerate()
    .filter_map(|(slot, (family, entry))| {
        let choice = entry;
        let unclassified_utilizations = unclassified_utilizations[slot].clone();
        if choice.is_none() && unclassified_utilizations.is_empty() {
            return None;
        }
        let (used, remaining, reset_at) = choice.map_or((None, None, None), |choice| {
            (choice.used, choice.remaining, choice.reset_at)
        });
        Some(AntigravityPool {
            family,
            window: Some(AntigravityWindow::Session),
            used_percent: used,
            remaining_percent: remaining,
            used_percent_origin: used.map(|_| PercentObservationOrigin::ProviderReported),
            remaining_percent_origin: remaining.map(|_| PercentObservationOrigin::ProviderReported),
            reset_at,
            source_label: None,
            unclassified_utilizations,
        })
    })
    .collect()
}

struct LegacyPoolChoice {
    id: String,
    score: f64,
    used: Option<f64>,
    remaining: Option<f64>,
    reset_at: Option<i64>,
}
