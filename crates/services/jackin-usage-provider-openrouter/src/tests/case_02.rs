// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn openrouter_credits_separates_spend_from_remaining_balance() {
    let outcome = parse_openrouter_credits(serde_json::json!({
        "data": {"total_credits": 100.0, "total_usage": 25.0}
    }))
    .expect("credits fixture");
    let OpenRouterCreditsOutcome::Available {
        spent_cents,
        ceiling_cents,
    } = outcome
    else {
        panic!("expected available credits");
    };
    let view = openrouter_credits_bucket(spent_cents, ceiling_cents);
    assert_eq!(view.used_label.as_deref(), Some("$25"));
    assert_eq!(view.limit_label.as_deref(), Some("$100"));
    assert_eq!(
        view.used_money.as_ref().map(|money| money.amount_minor),
        Some(2_500)
    );
    assert_eq!(
        view.limit_money.as_ref().map(|money| money.amount_minor),
        Some(10_000)
    );
    assert_eq!(
        view.remaining_percent
            .map(jackin_protocol::control::CanonicalRemainingPercent::get),
        Some(75.0)
    );
    assert_eq!(
        view.remaining_percent_origin,
        Some(jackin_protocol::control::PercentObservationOrigin::Derived)
    );

    let exhausted = openrouter_credits_bucket(1_000, 1_000);
    assert_eq!(exhausted.used_label.as_deref(), Some("$10"));
    assert_eq!(
        exhausted
            .remaining_percent
            .map(jackin_protocol::control::CanonicalRemainingPercent::get),
        Some(0.0)
    );
    assert_eq!(
        exhausted.remaining_percent_origin,
        Some(jackin_protocol::control::PercentObservationOrigin::Derived)
    );

    let overage = openrouter_credits_bucket(12_000, 10_000);
    assert_eq!(overage.used_label.as_deref(), Some("$120"));
    assert_eq!(overage.remaining_percent, None);
    assert_eq!(
        jackin_usage_provider_core::money_used_percent(&overage)
            .map(jackin_protocol::control::RawPercentPoints::get),
        Some(120.0)
    );
    assert_eq!(
        overage.used_money.as_ref().map(|money| money.amount_minor),
        Some(12_000)
    );

    let bare = openrouter_credits_bucket(0, 0);
    assert_eq!(bare.remaining_percent, None);
}

#[test]
fn openrouter_money_derived_remaining_stays_continuous_and_signed_ratio_stays_raw() {
    use jackin_protocol::control::PercentObservationOrigin::Derived;

    let fractional = openrouter_credits_bucket(1, 3);
    let remaining = fractional
        .remaining_percent
        .map(jackin_protocol::control::CanonicalRemainingPercent::get)
        .expect("derived remaining");
    assert!((remaining - (200.0 / 3.0)).abs() < 1e-12);
    assert_eq!(fractional.remaining_percent_origin, Some(Derived));
    assert_eq!(
        fractional
            .used_money
            .as_ref()
            .map(|money| money.amount_minor),
        Some(1)
    );
    assert_eq!(
        fractional
            .limit_money
            .as_ref()
            .map(|money| money.amount_minor),
        Some(3)
    );
    assert!(
        (jackin_usage_provider_core::money_used_percent(&fractional)
            .expect("money ratio")
            .get()
            - (100.0 / 3.0))
            .abs()
            < 1e-12
    );

    let negative_used = openrouter_credits_bucket(-2_500, 10_000);
    assert_eq!(
        negative_used
            .remaining_percent
            .map(jackin_protocol::control::CanonicalRemainingPercent::get),
        Some(125.0)
    );
    assert_eq!(negative_used.remaining_percent_origin, Some(Derived));
    assert_eq!(
        jackin_usage_provider_core::money_used_percent(&negative_used)
            .map(jackin_protocol::control::RawPercentPoints::get),
        Some(-25.0)
    );
    assert_eq!(
        negative_used
            .used_money
            .as_ref()
            .map(|money| money.amount_minor),
        Some(-2_500)
    );
}
