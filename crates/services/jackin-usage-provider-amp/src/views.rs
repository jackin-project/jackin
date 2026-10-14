// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{AmpSubscriptionKind, AmpUsage, parse_amp_usage_output};
use jackin_protocol::control::{
    FocusedUsageView, Money, QuotaBucketView, StatusSlot, UsageConfidence, UsageSnapshotStatus,
    UsageSource,
};
use jackin_usage_provider_core::{
    UsageSurface, UsageViewInput, bucket, format_currency, timed_bucket, usage_view,
    with_status_slot,
};

impl AmpUsage {
    pub fn from_api_value(value: serde_json::Value) -> Option<Self> {
        let root = value.get("result").unwrap_or(&value);
        let display_text = root
            .get("displayText")
            .and_then(serde_json::Value::as_str)?;
        parse_amp_usage_output(display_text)
    }

    pub fn plan_label(&self) -> Option<String> {
        if let Some(subscription) = &self.subscription {
            return Some(format!("Amp {}", subscription.plan));
        }
        self.daily_remaining_percent.map(|_| "Amp Free".to_owned())
    }

    fn subscription_pace(&self) -> Option<String> {
        let mut parts = Vec::new();
        if let Some(renewal) = &self.renewal {
            parts.push(renewal.label());
        }
        if let Some(period) = &self.billing_period {
            parts.push(format!("period {period}"));
        }
        (!parts.is_empty()).then(|| parts.join(" · "))
    }

    pub fn buckets(&self, now: i64) -> Vec<QuotaBucketView> {
        let mut buckets = Vec::new();
        if let Some(remaining) = self.daily_remaining_percent {
            buckets.push(with_status_slot(
                bucket(
                    "Amp Free",
                    None,
                    None,
                    Some(remaining),
                    Some("Resets daily".to_owned()),
                    None,
                    UsageSnapshotStatus::Fresh,
                ),
                Some(StatusSlot::Daily),
            ));
        }
        if let Some(subscription) = &self.subscription {
            let reset_at = self.renewal.as_ref().map(|renewal| renewal.resets_at(now));
            let pace = self.subscription_pace();
            match &subscription.kind {
                AmpSubscriptionKind::Tier {
                    agent_remaining,
                    agent_limit,
                    orb_remaining_hours,
                    orb_limit_hours,
                } => {
                    push_amp_agent_dollar_bucket(
                        &mut buckets,
                        *agent_remaining,
                        *agent_limit,
                        reset_at,
                        now,
                        pace.as_deref(),
                    );
                    if let (Some(remaining), Some(limit)) = (orb_remaining_hours, orb_limit_hours) {
                        push_amp_orb_bucket(
                            &mut buckets,
                            *remaining,
                            *limit,
                            reset_at,
                            now,
                            pace.as_deref(),
                        );
                    }
                }
                AmpSubscriptionKind::Legacy {
                    agent_remaining_percent,
                    orb_remaining_percent,
                } => {
                    for (label, remaining) in [
                        ("Agent usage", *agent_remaining_percent),
                        ("Orb usage", *orb_remaining_percent),
                    ] {
                        buckets.push(timed_bucket(
                            label,
                            Some(format!("{}% used", 100u8.saturating_sub(remaining))),
                            Some("100%".to_owned()),
                            Some(remaining),
                            reset_at,
                            now,
                            pace.as_deref(),
                            UsageSnapshotStatus::Fresh,
                        ));
                    }
                }
            }
        }
        if let Some(credits) = self.individual_credits {
            buckets.push(bucket(
                "Individual credits",
                None,
                Some(format_currency(credits)),
                None,
                None,
                Some(&format!("Individual credits: {}", format_currency(credits))),
                UsageSnapshotStatus::Fresh,
            ));
        }
        for balance in &self.workspace_balances {
            let label = format!("Workspace {}", balance.name);
            let detail = format!("{label}: {}", format_currency(balance.remaining));
            buckets.push(bucket(
                &label,
                None,
                Some(format_currency(balance.remaining)),
                None,
                None,
                Some(&detail),
                UsageSnapshotStatus::Fresh,
            ));
        }
        buckets
    }
}

pub(crate) fn push_amp_agent_dollar_bucket(
    buckets: &mut Vec<QuotaBucketView>,
    remaining: f64,
    limit: f64,
    reset_at: Option<i64>,
    now: i64,
    pace: Option<&str>,
) {
    let used = (limit - remaining).max(0.0);
    let used_money = Money::new((used * 100.0).round() as i64, "USD", 2);
    let limit_money = Money::new((limit * 100.0).round() as i64, "USD", 2);
    #[expect(
        clippy::cast_sign_loss,
        reason = "fraction clamped to 0.0..=1.0; percent is rounded f64→u8"
    )]
    let remaining_percent = Some(((remaining / limit).clamp(0.0, 1.0) * 100.0).round() as u8);
    let mut view = timed_bucket(
        "Agent usage",
        Some(format!("{used_money} used")),
        Some(limit_money.to_string()),
        remaining_percent,
        reset_at,
        now,
        pace,
        UsageSnapshotStatus::Fresh,
    );
    view.status_slot = Some(StatusSlot::Spend);
    view.used_money = Some(used_money);
    view.limit_money = Some(limit_money);
    buckets.push(view);
}

pub(crate) fn push_amp_orb_bucket(
    buckets: &mut Vec<QuotaBucketView>,
    remaining: f64,
    limit: f64,
    reset_at: Option<i64>,
    now: i64,
    pace: Option<&str>,
) {
    let used = (limit - remaining).max(0.0);
    #[expect(
        clippy::cast_sign_loss,
        reason = "fraction clamped to 0.0..=1.0; percent is rounded f64→u8"
    )]
    let remaining_percent = Some(((remaining / limit).clamp(0.0, 1.0) * 100.0).round() as u8);
    buckets.push(timed_bucket(
        "Orb usage",
        Some(format!("{} used", format_orb_hours(used))),
        Some(format_orb_hours(limit)),
        remaining_percent,
        reset_at,
        now,
        pace,
        UsageSnapshotStatus::Fresh,
    ));
}

fn format_orb_hours(hours: f64) -> String {
    if hours < 1.0 {
        if hours > 0.0 {
            "< 1h".to_owned()
        } else {
            "0h".to_owned()
        }
    } else {
        format!("{}h", hours.floor())
    }
}

#[derive(Debug)]
pub struct AmpSuccessContext<'a> {
    pub agent: &'a str,
    pub credential_origin: Option<String>,
    pub source: UsageSource,
}

pub fn amp_view_from_usage(
    context: AmpSuccessContext<'_>,
    usage: AmpUsage,
    now: i64,
) -> FocusedUsageView {
    let account_label = usage
        .account_label
        .clone()
        .unwrap_or_else(|| "local Amp auth".to_owned());
    let plan_label = usage.plan_label();
    let buckets = usage.buckets(now);
    usage_view(UsageViewInput {
        agent: context.agent,
        provider: None,
        surface: UsageSurface::Amp,
        account_label,
        username: None,
        plan_label,
        credential_origin: context.credential_origin,
        buckets,
        status: UsageSnapshotStatus::Fresh,
        source: context.source,
        confidence: UsageConfidence::Authoritative,
        now,
        last_error: None,
    })
}
