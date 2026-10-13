// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn amp_tier_line_maps_agent_dollars_orb_hours_and_renewal() {
    let now = 1_781_185_560;
    let usage = parse_amp_usage_output(AMP_TIER_FIXTURE).expect("tier usage");
    assert_eq!(usage.account_label.as_deref(), Some("user@example.com"));
    assert_eq!(usage.plan_label().as_deref(), Some("Amp Pro"));
    assert_eq!(
        usage.renewal,
        Some(AmpRenewal {
            value: 12,
            months: false,
        })
    );
    assert_eq!(
        usage.billing_period.as_deref(),
        Some("2026-09-01 to 2026-10-01")
    );

    let buckets = usage.buckets(now);
    let agent = buckets
        .iter()
        .find(|bucket| bucket.label == "Agent usage")
        .expect("agent bucket");
    assert_eq!(agent.used_label.as_deref(), Some("$20.00 used"));
    assert_eq!(agent.limit_label.as_deref(), Some("$100.00"));
    assert_eq!(agent.remaining_percent, Some(80));
    assert_eq!(agent.resets_at, Some(now + 12 * 86_400));
    assert_eq!(agent.status_slot, Some(StatusSlot::Spend));
    assert_eq!(
        agent.used_money.as_ref().map(|m| m.amount_minor),
        Some(2_000)
    );
    assert_eq!(
        agent.limit_money.as_ref().map(|m| m.amount_minor),
        Some(10_000)
    );

    let orb = buckets
        .iter()
        .find(|bucket| bucket.label == "Orb usage")
        .expect("orb bucket");
    assert_eq!(orb.used_label.as_deref(), Some("2h used"));
    assert_eq!(orb.limit_label.as_deref(), Some("10h"));
    assert_eq!(orb.remaining_percent, Some(75));
    assert_eq!(orb.resets_at, Some(now + 12 * 86_400));

    // Daily headline is untouched by subscription pools.
    assert_eq!(
        status_bar_headline_for_surface(UsageSurface::Amp, &buckets).as_deref(),
        Some("Free 61%")
    );
}

#[test]
fn amp_tier_without_orb_keeps_agent_and_skips_orb() {
    let usage = parse_amp_usage_output(
        "Amp Team Tier: agent usage $1,234.50 of $2,000.00 remaining, resets upon renewal in 2 months",
    )
    .expect("orb-less tier");
    assert_eq!(usage.plan_label().as_deref(), Some("Amp Team"));
    let buckets = usage.buckets(1_781_185_560);
    let agent = buckets
        .iter()
        .find(|bucket| bucket.label == "Agent usage")
        .expect("agent bucket");
    assert_eq!(agent.remaining_percent, Some(62));
    assert_eq!(agent.resets_at, Some(1_781_185_560 + 60 * 86_400));
    assert!(
        buckets.iter().all(|bucket| bucket.label != "Orb usage"),
        "no orb segment, no orb bucket"
    );
}

#[test]
fn amp_unrecognized_orb_data_does_not_hide_agent() {
    let usage = parse_amp_usage_output(
        "Amp Pro Tier: agent usage $80.00 of $100.00 remaining, orb usage someday maybe, resets upon renewal in 12 days",
    )
    .expect("agent survives bad orb");
    let buckets = usage.buckets(1_781_185_560);
    assert!(
        buckets.iter().any(|bucket| bucket.label == "Agent usage"),
        "agent bucket present"
    );
    assert!(
        buckets.iter().all(|bucket| bucket.label != "Orb usage"),
        "malformed orb skipped"
    );
}

#[test]
fn amp_legacy_subscription_line_maps_percent_pools() {
    let usage = parse_amp_usage_output(
        "Subscription Business: 30% other usage and 55% orb usage remaining - resets upon renewal in 1 month - https://ampcode.com/settings",
    )
    .expect("legacy subscription");
    assert_eq!(usage.plan_label().as_deref(), Some("Amp Business"));
    let buckets = usage.buckets(1_781_185_560);
    let agent = buckets
        .iter()
        .find(|bucket| bucket.label == "Agent usage")
        .expect("agent bucket");
    assert_eq!(agent.remaining_percent, Some(30));
    assert_eq!(agent.resets_at, Some(1_781_185_560 + 30 * 86_400));
    let orb = buckets
        .iter()
        .find(|bucket| bucket.label == "Orb usage")
        .expect("orb bucket");
    assert_eq!(orb.remaining_percent, Some(55));

    // The `Amp <plan> Subscription:` variant parses identically.
    let variant = parse_amp_usage_output(
        "Amp Business Subscription: 30% other usage and 55% orb usage remaining - resets upon renewal in 5 days",
    )
    .expect("amp-prefixed legacy");
    assert_eq!(
        variant.subscription, usage.subscription,
        "same pools, only renewal differs"
    );
    assert_eq!(
        variant.renewal,
        Some(AmpRenewal {
            value: 5,
            months: false,
        })
    );
}

#[test]
fn amp_tier_wins_over_legacy_and_bold_markers_strip() {
    let usage = parse_amp_usage_output(
        "Subscription Business: 30% other usage and 55% orb usage remaining - resets upon renewal in 5 days\n\
         **Amp Pro Tier:** agent usage $80.00 of $100.00 remaining, resets upon renewal in 12 days",
    )
    .expect("tier over legacy");
    assert_eq!(usage.plan_label().as_deref(), Some("Amp Pro"));
    assert!(
        matches!(
            usage.subscription.as_ref().map(|s| &s.kind),
            Some(AmpSubscriptionKind::Tier { .. })
        ),
        "tier kind kept"
    );
}
