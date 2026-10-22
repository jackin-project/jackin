// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::control::PercentObservationOrigin;
use jackin_usage_provider_amp::parse_amp_usage_output;

#[test]
fn amp_keeps_reported_fractional_percent_and_marks_money_ratio_derived() {
    let usage = parse_amp_usage_output(
        "Amp Free: 37.125% remaining today\nAmp Pro Tier: agent usage $1.23 of $10.00 remaining",
    )
    .expect("Amp usage parses");
    let buckets = usage.buckets(1_781_185_560);

    let free = buckets
        .iter()
        .find(|bucket| bucket.label == "Amp Free")
        .expect("free bucket");
    assert_eq!(
        free.remaining_percent
            .map(jackin_protocol::control::CanonicalRemainingPercent::get),
        Some(37.125)
    );
    assert_eq!(
        free.remaining_percent_origin,
        Some(PercentObservationOrigin::ProviderReported)
    );

    let agent = buckets
        .iter()
        .find(|bucket| bucket.label == "Agent usage")
        .expect("agent bucket");
    assert_eq!(
        agent.used_money.as_ref().map(|money| money.amount_minor),
        Some(877)
    );
    assert_eq!(
        agent.limit_money.as_ref().map(|money| money.amount_minor),
        Some(1_000)
    );
    assert_eq!(
        agent
            .remaining_percent
            .map(jackin_protocol::control::CanonicalRemainingPercent::get),
        Some(12.3)
    );
    assert_eq!(
        agent.remaining_percent_origin,
        Some(PercentObservationOrigin::Derived)
    );
}

#[test]
fn amp_overage_keeps_signed_money_without_clamping_source_amounts() {
    let usage = parse_amp_usage_output("Amp Pro Tier: agent usage $101.25 of $100.00 remaining")
        .expect("Amp overage parses");
    let agent = usage
        .buckets(1_781_185_560)
        .into_iter()
        .find(|bucket| bucket.label == "Agent usage")
        .expect("agent bucket");

    assert_eq!(agent.used_label.as_deref(), Some("$-1.25 used"));
    assert_eq!(
        agent.used_money.as_ref().map(|money| money.amount_minor),
        Some(-125)
    );
    assert_eq!(
        agent.limit_money.as_ref().map(|money| money.amount_minor),
        Some(10_000)
    );
    assert_eq!(
        agent
            .remaining_percent
            .map(jackin_protocol::control::CanonicalRemainingPercent::get),
        Some(101.25)
    );
    assert_eq!(
        agent.remaining_percent_origin,
        Some(PercentObservationOrigin::Derived)
    );
}

#[test]
fn amp_tier_money_pair_accepts_supported_comma_suffixes() {
    for (line, has_orb) in [
        (
            "Amp Pro Tier: agent usage $1.23 of $10.00 remaining, resets upon renewal in 2 months",
            false,
        ),
        (
            "Amp Pro Tier: agent usage $1.23 of $10.00 remaining, orb usage 2h of 10h orb hours, resets upon renewal in 12 days",
            true,
        ),
    ] {
        let usage = parse_amp_usage_output(line).expect("comma-delimited tier line parses");
        let buckets = usage.buckets(1_781_185_560);
        let agent = buckets
            .iter()
            .find(|bucket| bucket.label == "Agent usage")
            .expect("agent quota is retained");
        assert_eq!(
            agent.used_money.as_ref().map(|money| money.amount_minor),
            Some(877)
        );
        assert_eq!(
            agent.limit_money.as_ref().map(|money| money.amount_minor),
            Some(1_000)
        );
        assert_eq!(
            buckets.iter().any(|bucket| bucket.label == "Orb usage"),
            has_orb
        );
    }
}

#[test]
fn amp_tier_money_pair_uses_anchored_exact_grammar() {
    for malformed in [
        "Amp Pro Tier: agent usage $bad of $10.00 remaining then $1.00 of $2.00 remaining",
        "Amp Pro Tier: agent usage -$1.00 of $10.00 remaining",
        "Amp Pro Tier: agent usage $-1.00 of $10.00 remaining",
        "Amp Pro Tier: agent usage $1,23.00 of $10.00 remaining",
        "Amp Pro Tier: agent usage $1.1234567 of $10.00 remaining",
        "Amp Pro Tier: agent usage $92233720368547759 of $92233720368547758.07 remaining",
        "Amp Pro Tier: agent usage $92233720368547758.08 of $92233720368547758.09 remaining",
        "Amp Pro Tier: agent usage $1.00 of $10.00 remainingLater",
    ] {
        assert!(parse_amp_usage_output(malformed).is_none(), "{malformed}");
    }

    let usage = parse_amp_usage_output("Amp Pro Tier: agent usage $1.2 of $2.345 remaining")
        .expect("unequal decimal scales parse exactly");
    let bucket = usage
        .buckets(1_781_185_560)
        .into_iter()
        .find(|bucket| bucket.label == "Agent usage")
        .expect("agent bucket");
    assert_eq!(
        bucket
            .used_money
            .as_ref()
            .map(|money| (money.amount_minor, money.exponent)),
        Some((1_145, 3))
    );
    assert_eq!(
        bucket
            .limit_money
            .as_ref()
            .map(|money| (money.amount_minor, money.exponent)),
        Some((2_345, 3))
    );
}

#[test]
fn amp_tier_money_parser_preserves_large_exact_minor_units_and_valid_grouping() {
    let usage = parse_amp_usage_output(
        "Amp Pro Tier: agent usage $90,071,992,547,409.93 of $92,233,720,368,547,758.07 remaining",
    )
    .expect("large exact decimal amounts parse");
    let bucket = usage
        .buckets(1_781_185_560)
        .into_iter()
        .find(|bucket| bucket.label == "Agent usage")
        .expect("agent bucket");
    assert_eq!(
        bucket.used_money.as_ref().map(|money| money.amount_minor),
        Some(9_214_364_837_600_034_814)
    );
    assert_eq!(
        bucket.limit_money.as_ref().map(|money| money.amount_minor),
        Some(i64::MAX)
    );
}
