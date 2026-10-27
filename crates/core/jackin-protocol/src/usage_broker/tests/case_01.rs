// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn metric_group_validate_rejects_each_mixed_currency_spend_pair() {
    for value in [
        UsageMetricValueV1::SpendCap {
            cap: Some(Money::new(100, "USD", 2)),
            spent: Some(Money::new(50, "SGD", 0)),
            remaining: None,
        },
        UsageMetricValueV1::SpendCap {
            cap: Some(Money::new(100, "USD", 2)),
            spent: None,
            remaining: Some(Money::new(50, "SGD", 0)),
        },
        UsageMetricValueV1::SpendCap {
            cap: None,
            spent: Some(Money::new(100, "USD", 2)),
            remaining: Some(Money::new(50, "SGD", 0)),
        },
    ] {
        let mut group = window_group("g-currency-mismatch", 0);
        group.kind = UsageMetricGroupKindV1::SpendCap;
        group.value = value;
        group.validate(0).unwrap_err();
    }
}
#[test]
fn metric_group_validate_accepts_independent_scales_and_rejects_mismatches() {
    let mut group = window_group("g", 0);
    group.kind = UsageMetricGroupKindV1::Balance;
    group.validate(0).unwrap_err();
    group.kind = UsageMetricGroupKindV1::Window;

    group.value = UsageMetricValueV1::SpendCap {
        cap: Some(Money::new(1_00, "USD", 2)),
        spent: Some(Money::new(50, "SGD", 2)),
        remaining: None,
    };
    group.kind = UsageMetricGroupKindV1::SpendCap;
    group.validate(0).unwrap_err();

    group.value = UsageMetricValueV1::SpendCap {
        cap: Some(Money::new(1_00, "USD", 2)),
        spent: Some(Money::new(50, "USD", 3)),
        remaining: None,
    };
    group
        .validate(0)
        .expect("same-currency Money values may use independent scales");

    group.group_id = "g-mixed-scale".to_owned();
    group.value = UsageMetricValueV1::SpendCap {
        cap: Some(Money::new(100_000_000_000_000_000, "USD", 0)),
        spent: Some(Money::new(999_999_999_999_999_999, "USD", 2)),
        remaining: Some(Money::new(9_000_000_000_000_000_001, "USD", 2)),
    };
    group
        .validate(0)
        .expect("independent Money scales validate");

    let bytes = serde_json::to_vec(&group).expect("group serializes");
    assert_eq!(
        serde_json::from_slice::<UsageMetricGroupV1>(&bytes).expect("group deserializes"),
        group
    );
}
