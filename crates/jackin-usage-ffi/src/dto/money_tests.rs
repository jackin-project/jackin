// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn money_dto_preserves_full_signed_domain_currency_and_exponent() {
    for amount_minor in [i64::MIN, -250, 0, 9_007_199_254_740_993, i64::MAX] {
        for exponent in [0, 2, 3, u8::MAX] {
            let dto = money_dto(Money {
                amount_minor,
                currency: "SGD".to_owned(),
                exponent,
            });
            assert_eq!(dto.amount_minor, amount_minor);
            assert_eq!(dto.currency, "SGD");
            assert_eq!(dto.exponent, exponent);
        }
    }
}

#[test]
fn bucket_dto_preserves_independent_raw_money_including_negative_remaining() {
    let remaining = Money {
        amount_minor: -250,
        currency: "USD".to_owned(),
        exponent: 2,
    };
    let dto = bucket_dto(QuotaBucketView {
        label: "Spend".to_owned(),
        used_label: Some("9000 tokens".to_owned()),
        limit_label: Some("JPY 7".to_owned()),
        remaining_percent: None,
        reset_label: None,
        resets_at: None,
        status_slot: Some(jackin_protocol::control::StatusSlot::Spend),
        pace_label: None,
        status: UsageSnapshotStatus::Fresh,
        used_money: None,
        limit_money: Some(Money {
            amount_minor: i64::MAX,
            currency: "USD".to_owned(),
            exponent: 2,
        }),
        remaining_money: Some(remaining),
        count_quota: None,
        severity: jackin_protocol::control::UsageSeverity::Normal,
    });
    assert!(dto.used_money.is_none());
    let limit = dto.limit_money.expect("limit remains known");
    assert_eq!(limit.amount_minor, i64::MAX);
    let remaining = dto.remaining_money.expect("negative overage must survive");
    assert_eq!(remaining.amount_minor, -250);
    assert_eq!(remaining.currency, "USD");
    assert_eq!(remaining.exponent, 2);
    assert!(dto.count_quota.is_none());
}

#[test]
fn account_dto_copies_all_three_money_fields_with_sign_and_denomination() {
    let money = |amount_minor| Money {
        amount_minor,
        currency: "SGD".to_owned(),
        exponent: u8::MAX,
    };
    let dto = account_dto(HostAccountDescriptor {
        surface_id: "openrouter".to_owned(),
        provider_column_label: String::new(),
        account_key: "money-account".to_owned(),
        account_label: "money-account".to_owned(),
        plan_label: None,
        selected: true,
        lifecycle: "current".to_owned(),
        lifecycle_label: "Current".to_owned(),
        provenance: Vec::new(),
        provenance_label: "Provider reported".to_owned(),
        plan_or_status_label: "Ready".to_owned(),
        remaining_percent: None,
        remaining_label: "Rust-owned remaining label".to_owned(),
        headline: "Rust-owned headline".to_owned(),
        reset_label: None,
        reset_display_label: "—".to_owned(),
        exact_reset: None,
        status_word: "fresh".to_owned(),
        status_label: "Ready".to_owned(),
        severity: "normal".to_owned(),
        updated_label: "Updated now".to_owned(),
        last_error: None,
        dimmed: false,
        accessibility_label: "money-account".to_owned(),
        count_quota: None,
        resets_at: None,
        used_money: Some(money(9_007_199_254_740_993)),
        limit_money: Some(money(0)),
        remaining_money: Some(money(i64::MIN)),
    });
    for (actual, amount) in [
        (dto.used_money, 9_007_199_254_740_993),
        (dto.limit_money, 0),
        (dto.remaining_money, i64::MIN),
    ] {
        let actual = actual.expect("reported money remains present");
        assert_eq!(actual.amount_minor, amount);
        assert_eq!(actual.currency, "SGD");
        assert_eq!(actual.exponent, u8::MAX);
    }
    assert!(dto.count_quota.is_none());
}
