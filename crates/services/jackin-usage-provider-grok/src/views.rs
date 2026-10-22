// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::control::{
    CanonicalRemainingPercent, CanonicalUsedPercent, Money, PercentObservationOrigin,
    QuotaBucketView, StatusSlot, UsageSnapshotStatus, UsageSource,
};
use jackin_usage_provider_core::{
    bucket, format_cents, parse_iso_epoch, quota_pace_label, timed_bucket, with_used_percent,
};

use super::{
    GrokBillingConfig, GrokBillingResponse, GrokBillingSnapshot, GrokWebBillingSnapshot,
    grok_period_label, positive_cent_value,
};

impl GrokBillingSnapshot {
    pub fn buckets(&self, now: i64) -> Vec<QuotaBucketView> {
        match self {
            Self::Rpc(response) | Self::Rest(response) => response.buckets(now),
            Self::Web(snapshot) => snapshot.buckets(now),
        }
    }

    pub(crate) fn source(&self) -> UsageSource {
        match self {
            Self::Rpc(_) => UsageSource::Cli,
            Self::Rest(_) => UsageSource::ProviderApi,
            Self::Web(_) => UsageSource::ProviderApi,
        }
    }

    pub fn plan_label(&self) -> Option<String> {
        match self {
            Self::Rpc(response) | Self::Rest(response) => response.plan_label(),
            Self::Web(_) => None,
        }
    }
}

impl GrokWebBillingSnapshot {
    pub fn buckets(&self, now: i64) -> Vec<QuotaBucketView> {
        let label = self.reset_at_epoch.map_or("Credits", |reset_at| {
            grok_cycle_label_from_reset(reset_at, now)
        });
        let view = timed_bucket(
            label,
            Some(self.used_percent.label()),
            Some("100%".to_owned()),
            None,
            self.reset_at_epoch,
            now,
            None,
            UsageSnapshotStatus::Fresh,
        );
        let mut view = with_used_percent(
            view,
            Some((
                self.used_percent,
                PercentObservationOrigin::ProviderReported,
            )),
        );
        view.status_slot = Some(StatusSlot::Weekly);
        vec![view]
    }
}

impl GrokBillingResponse {
    pub fn plan_label(&self) -> Option<String> {
        self.subscription_tier
            .as_deref()
            .map(str::trim)
            .filter(|tier| !tier.is_empty())
            .map(str::to_owned)
    }

    pub fn buckets(&self, now: i64) -> Vec<QuotaBucketView> {
        let mut buckets = Vec::new();
        let Some(config) = self.config.as_ref() else {
            return buckets;
        };
        if let Some(headline) = config.headline_bucket(now) {
            buckets.push(headline);
        }
        if let Some(balance) = config
            .prepaid_balance
            .as_ref()
            .and_then(|cents| positive_cent_value(cents.val))
        {
            let mut view = bucket(
                "Extra usage credits",
                None,
                Some(format_cents(balance)),
                None,
                None,
                None,
                UsageSnapshotStatus::Fresh,
            );
            view.limit_money = Some(Money::new(balance, "USD", 2));
            buckets.push(view);
        }
        if self.on_demand_enabled != Some(false)
            && let Some(cap) = config
                .on_demand_cap
                .as_ref()
                .and_then(|cents| positive_cent_value(cents.val))
        {
            let used = config
                .on_demand_used
                .as_ref()
                .map(|cents| cents.val)
                .filter(|used| *used >= 0);
            let mut view = bucket(
                "On-demand usage",
                used.map(format_cents),
                Some(format_cents(cap)),
                None,
                None,
                None,
                UsageSnapshotStatus::Fresh,
            );
            view.used_money = used.map(|used| Money::new(used, "USD", 2));
            view.limit_money = Some(Money::new(cap, "USD", 2));
            buckets.push(view);
        }
        buckets
    }
}

impl GrokBillingConfig {
    pub(crate) fn headline_bucket(&self, now: i64) -> Option<QuotaBucketView> {
        let explicit_used_percent = self.credit_usage_percent_value();
        if explicit_used_percent.is_err() {
            let period = self.current_period.as_ref();
            let reset_at = period
                .and_then(|period| period.end.as_deref())
                .and_then(parse_iso_epoch)
                .or_else(|| {
                    self.billing_period_end
                        .as_deref()
                        .or(self.period_end.as_deref())
                        .and_then(parse_iso_epoch)
                });
            let label = period.map_or("Usage", |period| {
                let seconds = period
                    .start
                    .as_deref()
                    .and_then(parse_iso_epoch)
                    .zip(reset_at)
                    .map(|(start, end)| end.saturating_sub(start));
                seconds.map_or("Usage", |seconds| {
                    grok_period_label(period.period_type.as_deref(), seconds)
                })
            });
            return Some(invalid_billing_headline(label, reset_at, now));
        }
        if let Some(period) = self.current_period.as_ref()
            && !period
                .period_type
                .as_deref()
                .is_some_and(|kind| kind.contains("MONTHLY"))
            && let Some(start) = period.start.as_deref().and_then(parse_iso_epoch)
            && let Some(end) = period.end.as_deref().and_then(parse_iso_epoch)
            && end > start
        {
            let window_seconds = end - start;
            let label = grok_period_label(period.period_type.as_deref(), window_seconds);
            let Some(used_percent) = explicit_used_percent.ok().flatten() else {
                return Some(unknown_billing_headline(label, end, now));
            };
            return Some(used_percent_billing_headline(
                label,
                used_percent,
                PercentObservationOrigin::ProviderReported,
                end,
                window_seconds,
                now,
            ));
        }
        if let Some(limit) = self
            .monthly_limit
            .as_ref()
            .and_then(|cents| positive_cent_value(cents.val))
            && let Some(start) = self
                .billing_period_start
                .as_deref()
                .or(self.period_start.as_deref())
                .and_then(parse_iso_epoch)
            && let Some(end) = self
                .billing_period_end
                .as_deref()
                .or(self.period_end.as_deref())
                .and_then(parse_iso_epoch)
            && end > start
        {
            let window_seconds = end - start;
            let label = grok_cycle_label_from_minutes(window_seconds / 60);
            let Some(used_cents) = self
                .used
                .as_ref()
                .map(|cents| cents.val)
                .filter(|used| *used >= 0)
            else {
                return Some(unknown_billing_headline(label, end, now));
            };
            let mut view =
                money_billing_headline(label, used_cents, limit, end, window_seconds, now);
            view.used_money = Some(Money::new(used_cents, "USD", 2));
            view.limit_money = Some(Money::new(limit, "USD", 2));
            return Some(view);
        }
        None
    }
}

pub(crate) fn invalid_billing_headline(
    label: &str,
    reset_at: Option<i64>,
    now: i64,
) -> QuotaBucketView {
    let mut view = timed_bucket(
        label,
        None,
        Some("100%".to_owned()),
        None,
        reset_at,
        now,
        Some("Grok used percent is invalid"),
        UsageSnapshotStatus::Unavailable,
    );
    if label == "Weekly" {
        view.status_slot = Some(StatusSlot::Weekly);
    }
    view
}

pub(crate) fn unknown_billing_headline(label: &str, reset_at: i64, now: i64) -> QuotaBucketView {
    timed_bucket(
        label,
        None,
        None,
        None,
        Some(reset_at),
        now,
        Some("No data"),
        UsageSnapshotStatus::Unavailable,
    )
}

pub(crate) fn used_percent_billing_headline(
    label: &str,
    used_percent: CanonicalUsedPercent,
    origin: PercentObservationOrigin,
    reset_at: i64,
    window_seconds: i64,
    now: i64,
) -> QuotaBucketView {
    let remaining = CanonicalRemainingPercent::new(used_percent.remaining_points()).ok();
    let pace = quota_pace_label(remaining, Some(reset_at), Some(window_seconds), now);
    let view = timed_bucket(
        label,
        Some(used_percent.label()),
        Some("100%".to_owned()),
        None,
        Some(reset_at),
        now,
        pace.as_deref(),
        UsageSnapshotStatus::Fresh,
    );
    let mut view = with_used_percent(view, Some((used_percent, origin)));
    view.status_slot = Some(StatusSlot::Weekly);
    view
}

fn money_billing_headline(
    label: &str,
    used_cents: i64,
    limit_cents: i64,
    reset_at: i64,
    window_seconds: i64,
    now: i64,
) -> QuotaBucketView {
    #[expect(clippy::cast_precision_loss, reason = "cents magnitudes fit f64")]
    let raw_used = (used_cents as f64 / limit_cents as f64) * 100.0;
    let remaining = CanonicalRemainingPercent::new((100.0 - raw_used).max(0.0)).ok();
    let pace = quota_pace_label(remaining, Some(reset_at), Some(window_seconds), now);
    let mut view = timed_bucket(
        label,
        Some(format_cents(used_cents)),
        Some(format_cents(limit_cents)),
        None,
        Some(reset_at),
        now,
        pace.as_deref(),
        UsageSnapshotStatus::Fresh,
    );
    view.status_slot = Some(StatusSlot::Weekly);
    view
}

pub fn grok_cycle_label_from_minutes(minutes: i64) -> &'static str {
    let days = minutes / (24 * 60);
    if (6..=8).contains(&days) {
        "Weekly"
    } else if (28..=31).contains(&days) {
        "Monthly"
    } else {
        "Credits"
    }
}

pub fn grok_cycle_label_from_reset(reset_at: i64, now: i64) -> &'static str {
    let days = reset_at.saturating_sub(now) / 86_400;
    if days <= 8 {
        "Weekly"
    } else if days <= 35 {
        "Monthly"
    } else {
        "Credits"
    }
}
