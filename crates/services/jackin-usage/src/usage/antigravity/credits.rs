// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Antigravity` credit parsing and buckets.

use super::antigravity_remaining_from_entry;
use jackin_protocol::control::{Money, QuotaBucketView, StatusSlot, UsageSnapshotStatus};
use jackin_usage_provider_core::{bucket, json_number};

/// Parsed `/credits` output. Money attaches only when the response states an
/// explicit minor-unit amount + exponent; plain numbers stay labels so an
/// unknown scale can never render 100× off.
#[derive(Debug, Clone, Default)]
pub(crate) struct AntigravityCredits {
    pub(crate) used_money: Option<Money>,
    pub(crate) limit_money: Option<Money>,
    pub(crate) balance_label: Option<String>,
    pub(crate) limit_label: Option<String>,
    pub(crate) remaining_percent: Option<u8>,
}

pub(crate) fn parse_antigravity_credits_output(text: &str) -> Result<AntigravityCredits, String> {
    let value: serde_json::Value = serde_json::from_str(text.trim())
        .map_err(|_| "Antigravity /credits output was not recognized".to_owned())?;
    let node = value.get("credits").unwrap_or(&value);
    let currency = ["currency", "unit"]
        .into_iter()
        .filter_map(|key| node.get(key).and_then(serde_json::Value::as_str))
        .map(str::trim)
        .find(|currency| !currency.is_empty())
        .unwrap_or("credits")
        .to_owned();
    let exponent = node
        .get("exponent")
        .and_then(json_number)
        .map_or(2, |value| {
            #[expect(
                clippy::cast_sign_loss,
                reason = "filtered non-negative; clamped 0..=6"
            )]
            {
                value.round().clamp(0.0, 6.0) as u8
            }
        });
    let minor = |key: &str| {
        node.get(key)
            .and_then(json_number)
            .map(|value| value.round() as i64)
    };
    let (used_money, limit_money) = match (minor("used_minor"), minor("limit_minor")) {
        (Some(used), Some(limit)) => (
            Some(Money::new(used, currency.clone(), exponent)),
            Some(Money::new(limit, currency.clone(), exponent)),
        ),
        (Some(used), None) => (Some(Money::new(used, currency.clone(), exponent)), None),
        _ => (None, None),
    };
    let major = |keys: &[&str]| {
        keys.iter()
            .filter_map(|key| node.get(*key).and_then(json_number))
            .find(|value| value.is_finite() && *value >= 0.0)
    };
    let balance = major(&["balance", "remaining", "available"]);
    let limit = major(&["limit", "total", "allowance"]);
    let remaining_percent = match (balance, limit) {
        (Some(left), Some(total)) if total > 0.0 => {
            #[expect(
                clippy::cast_sign_loss,
                reason = "filtered non-negative; clamped 0..=100"
            )]
            {
                Some((left.clamp(0.0, total) / total * 100.0).round() as u8)
            }
        }
        _ => antigravity_remaining_from_entry(node),
    };
    let money_label = |money: Option<&Money>| money.map(Money::to_string);
    Ok(AntigravityCredits {
        balance_label: money_label(used_money.as_ref())
            .or_else(|| balance.map(|value| format!("{value} {currency}"))),
        limit_label: money_label(limit_money.as_ref())
            .or_else(|| limit.map(|value| format!("{value} {currency}"))),
        used_money,
        limit_money,
        remaining_percent,
    })
}

pub(crate) fn antigravity_credits_bucket(credits: &AntigravityCredits) -> Option<QuotaBucketView> {
    if credits.used_money.is_none()
        && credits.limit_money.is_none()
        && credits.balance_label.is_none()
        && credits.limit_label.is_none()
    {
        return None;
    }
    let mut view = bucket(
        "Credits",
        credits.balance_label.clone(),
        credits.limit_label.clone(),
        credits.remaining_percent,
        None,
        None,
        UsageSnapshotStatus::Fresh,
    );
    // The Spend slot only when structured money backs it; a label-only credit
    // balance is a detail row, never a headline figure.
    if credits.used_money.is_some() {
        view.status_slot = Some(StatusSlot::Spend);
        view.used_money = credits.used_money.clone();
        view.limit_money = credits.limit_money.clone();
    }
    Some(view)
}
