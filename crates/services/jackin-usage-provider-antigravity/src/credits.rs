// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{
    antigravity_percentages_from_entry, antigravity_remaining_from_entry,
    antigravity_unclassified_utilization_from_entry,
};
use jackin_protocol::control::{
    CanonicalRemainingPercent, CanonicalUsedPercent, Money, PercentObservationOrigin,
    QuotaBucketView, StatusSlot, UsageSnapshotStatus,
};
use jackin_usage_provider_core::{bucket, json_number, with_used_percent};

#[derive(Debug, Clone, Default)]
pub struct AntigravityCredits {
    pub(crate) used_money: Option<Money>,
    pub(crate) limit_money: Option<Money>,
    pub(crate) balance_label: Option<String>,
    pub(crate) limit_label: Option<String>,
    pub(crate) used_percent: Option<(CanonicalUsedPercent, PercentObservationOrigin)>,
    pub(crate) remaining_percent: Option<(CanonicalRemainingPercent, PercentObservationOrigin)>,
    pub(crate) unclassified_utilization: Option<serde_json::Number>,
}

pub fn parse_antigravity_credits_output(text: &str) -> Result<AntigravityCredits, String> {
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
    let (reported_used, reported_remaining) = antigravity_percentages_from_entry(node);
    let used_percent = reported_used.and_then(|points| {
        CanonicalUsedPercent::new(points)
            .ok()
            .map(|percent| (percent, PercentObservationOrigin::ProviderReported))
    });
    let remaining_percent = reported_remaining
        .or_else(|| match (balance, limit) {
            (Some(left), Some(total)) if total > 0.0 => Some(left / total * 100.0),
            _ => antigravity_remaining_from_entry(node),
        })
        .and_then(|points| {
            CanonicalRemainingPercent::new(points).ok().map(|percent| {
                let origin = if reported_remaining.is_some() {
                    PercentObservationOrigin::ProviderReported
                } else {
                    PercentObservationOrigin::Derived
                };
                (percent, origin)
            })
        });
    let money_label = |money: Option<&Money>| money.map(Money::to_string);
    Ok(AntigravityCredits {
        balance_label: money_label(used_money.as_ref())
            .or_else(|| balance.map(|value| format!("{value} {currency}"))),
        limit_label: money_label(limit_money.as_ref())
            .or_else(|| limit.map(|value| format!("{value} {currency}"))),
        used_money,
        limit_money,
        used_percent,
        remaining_percent,
        unclassified_utilization: antigravity_unclassified_utilization_from_entry(node),
    })
}

pub fn antigravity_credits_bucket(credits: &AntigravityCredits) -> Option<QuotaBucketView> {
    if credits.used_money.is_none()
        && credits.limit_money.is_none()
        && credits.balance_label.is_none()
        && credits.limit_label.is_none()
        && credits.used_percent.is_none()
        && credits.remaining_percent.is_none()
        && credits.unclassified_utilization.is_none()
    {
        return None;
    }
    let view = bucket(
        "Credits",
        credits.balance_label.clone(),
        credits.limit_label.clone(),
        credits.remaining_percent,
        None,
        None,
        UsageSnapshotStatus::Fresh,
    );
    let mut view = with_used_percent(view, credits.used_percent);
    if credits.used_money.is_some() {
        view.status_slot = Some(StatusSlot::Spend);
        view.used_money = credits.used_money.clone();
        view.limit_money = credits.limit_money.clone();
    }
    if let Some(utilization) = &credits.unclassified_utilization {
        view.pace_label = Some(format!(
            "utilization {utilization} (unit unknown; not ranked as a percentage)"
        ));
    }
    Some(view)
}
