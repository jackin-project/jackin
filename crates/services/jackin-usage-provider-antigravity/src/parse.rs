// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Antigravity` CLI usage output parsing.

use jackin_usage_provider_core::{epoch_seconds_from_maybe_ms, json_number, parse_iso_epoch};

use super::{
    AntigravityFamily, AntigravityPool, AntigravityUsage, AntigravityWindow,
    antigravity_identity_from_value, antigravity_plan_from_value,
};

/// Parse `agy -p /usage --output-format json` output. `Err` only when the text
/// is not JSON at all; a well-formed response with no quota pools is `Ok` with
/// empty pools (the snapshot renders an honest placeholder, never 100%).
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

/// Summary-bucket containers, in precedence order. `Some` (even when empty)
/// means a summary shape is present and wins over legacy models.
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

/// Map one summary entry to a pool. Exact `bucketId` match first; otherwise a
/// family/window keyword read of the entry id. Entries without quota identity
/// are dropped (never fabricated into an empty-label row).
fn antigravity_pool_from_summary_entry(entry: &serde_json::Value) -> Option<AntigravityPool> {
    let id = ["bucketId", "bucket_id", "id", "poolId", "pool", "name"]
        .into_iter()
        .filter_map(|key| entry.get(key).and_then(serde_json::Value::as_str))
        .map(str::trim)
        .find(|id| !id.is_empty())?;
    // A summary entry with no quota signal is unknown, never depleted: the
    // `/usage` schema is unauthenticated and unverified, so absence ⇒ 0%
    // would invent exhaustion (F05 keeps unknown vs exhausted distinct; only
    // an explicit depleted marker yields 0).
    let remaining_percent = antigravity_remaining_from_entry(entry);
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
        remaining_percent,
        reset_at,
        source_label: Some(id.to_owned()),
    })
}

/// Remaining percent from a quota entry: `remainingFraction` (0-1),
/// `remainingPercent` (0-100), or an inverted used signal. `None` when the
/// entry carries no quota signal at all.
pub fn antigravity_remaining_from_entry(entry: &serde_json::Value) -> Option<u8> {
    let quota = entry.get("quotaInfo").unwrap_or(entry);
    for (key, is_fraction) in [
        ("remainingFraction", true),
        ("remaining_fraction", true),
        ("remainingPercent", false),
        ("remaining_percent", false),
        ("remaining", false),
    ] {
        if let Some(raw) = quota.get(key).and_then(json_number)
            && raw.is_finite()
            && raw >= 0.0
        {
            let percent = if is_fraction { raw * 100.0 } else { raw };
            #[expect(
                clippy::cast_sign_loss,
                reason = "filtered non-negative; clamped 0..=100"
            )]
            {
                return Some(percent.round().clamp(0.0, 100.0) as u8);
            }
        }
    }
    for key in ["usedPercent", "used_percent", "utilization", "used"] {
        if let Some(raw) = quota.get(key).and_then(json_number)
            && raw.is_finite()
            && raw >= 0.0
        {
            let used = if raw <= 1.0 { raw * 100.0 } else { raw };
            #[expect(
                clippy::cast_sign_loss,
                reason = "filtered non-negative; clamped 0..=100"
            )]
            {
                return Some(100u8.saturating_sub(used.round().clamp(0.0, 100.0) as u8));
            }
        }
    }
    None
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

/// Legacy per-model quota collapsed to the worst (lowest) remaining fraction
/// per family. Internal, empty-label, and availability-only rows are dropped:
/// a model that only reports availability contributes no pool.
fn antigravity_pools_from_legacy_models(value: &serde_json::Value) -> Vec<AntigravityPool> {
    let Some(models) = value.get("models").and_then(serde_json::Value::as_object) else {
        return Vec::new();
    };
    let mut worst: [Option<(u8, Option<i64>)>; 2] = [None, None];
    for (id, model) in models {
        if model.get("isInternal").and_then(serde_json::Value::as_bool) == Some(true) {
            continue;
        }
        let label = ["displayName", "label", "model"]
            .into_iter()
            .filter_map(|key| model.get(key).and_then(serde_json::Value::as_str))
            .map(str::trim)
            .find(|label| !label.is_empty());
        // Empty display label (no displayName/label/model) is not a pool,
        // even when the map key is non-empty.
        if label.is_none() {
            continue;
        }
        // Availability-only rows carry no quota signal: skip, never invent.
        let Some(remaining) = antigravity_remaining_from_entry(model) else {
            continue;
        };
        let name = format!("{id} {}", label.unwrap_or_default()).to_ascii_lowercase();
        let slot = usize::from(!name.contains("gemini"));
        let reset_at = antigravity_reset_from_entry(model);
        let replace = worst[slot].is_none_or(|(current, _)| remaining < current);
        if replace {
            worst[slot] = Some((remaining, reset_at));
        }
    }
    [
        (AntigravityFamily::Gemini, worst[0]),
        (AntigravityFamily::Other, worst[1]),
    ]
    .into_iter()
    .filter_map(|(family, entry)| {
        let (remaining, reset_at) = entry?;
        Some(AntigravityPool {
            family,
            window: Some(AntigravityWindow::Session),
            remaining_percent: Some(remaining),
            reset_at,
            source_label: None,
        })
    })
    .collect()
}
