// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn openrouter_credits_separates_spend_from_remaining_balance() {
    let snapshot = |credits_body: &str| {
        let server =
            CannedSnapshotServer::start_with_credits(key_fixture(), credits_body.to_owned());
        openrouter_snapshot_with_base(
            "opencode",
            Some("fixture-key"),
            server.base_url(),
            1_780_000_000,
        )
    };

    let normal = snapshot(r#"{"data":{"total_credits":100.0,"total_usage":25.0}}"#);
    assert_eq!(
        normal.status,
        jackin_protocol::control::UsageSnapshotStatus::Fresh
    );
    assert_eq!(normal.account.provider_label, "OpenRouter");
    let view = account_credits_bucket(&normal);
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

    let exhausted = snapshot(r#"{"data":{"total_credits":10.0,"total_usage":10.0}}"#);
    assert_eq!(
        exhausted.status,
        jackin_protocol::control::UsageSnapshotStatus::Fresh
    );
    assert_eq!(exhausted.account.provider_label, "OpenRouter");
    let exhausted = account_credits_bucket(&exhausted);
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

    let overage = snapshot(r#"{"data":{"total_credits":100.0,"total_usage":120.0}}"#);
    assert_eq!(
        overage.status,
        jackin_protocol::control::UsageSnapshotStatus::Fresh
    );
    assert_eq!(overage.account.provider_label, "OpenRouter");
    let overage = account_credits_bucket(&overage);
    assert_eq!(overage.used_label.as_deref(), Some("$120"));
    assert_eq!(overage.remaining_percent, None);
    assert_eq!(
        jackin_usage_provider_core::money_used_percent(overage)
            .map(jackin_protocol::control::RawPercentPoints::get),
        Some(120.0)
    );
    assert_eq!(
        overage.used_money.as_ref().map(|money| money.amount_minor),
        Some(12_000)
    );

    let bare = snapshot(r#"{"data":{"total_credits":0.0,"total_usage":0.0}}"#);
    assert_eq!(
        bare.status,
        jackin_protocol::control::UsageSnapshotStatus::Fresh
    );
    assert_eq!(bare.account.provider_label, "OpenRouter");
    let bare = account_credits_bucket(&bare);
    assert_eq!(bare.remaining_percent, None);
}

#[test]
fn openrouter_money_derived_remaining_stays_continuous_and_signed_ratio_stays_raw() {
    use jackin_protocol::control::PercentObservationOrigin::Derived;

    let server = CannedSnapshotServer::start_with_credits(
        key_fixture(),
        r#"{"data":{"total_credits":0.03,"total_usage":0.01}}"#.to_owned(),
    );
    let snapshot = openrouter_snapshot_with_base(
        "opencode",
        Some("fixture-key"),
        server.base_url(),
        1_780_000_000,
    );
    assert_eq!(
        snapshot.status,
        jackin_protocol::control::UsageSnapshotStatus::Fresh
    );
    assert_eq!(snapshot.account.provider_label, "OpenRouter");
    let fractional = account_credits_bucket(&snapshot);
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
        (jackin_usage_provider_core::money_used_percent(fractional)
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
