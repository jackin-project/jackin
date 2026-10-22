// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::legacy_remaining_percent;

#[test]
fn legacy_remaining_percent_accepts_exact_historical_u8_values() {
    for value in [0_u8, 100, 255] {
        assert_eq!(
            legacy_remaining_percent(i64::from(value)).unwrap().get(),
            f64::from(value)
        );
    }
}

#[test]
fn legacy_remaining_percent_rejects_values_outside_historical_u8_domain() {
    for value in [-1, 256, (1_i64 << 53) + 1, i64::MAX] {
        assert!(legacy_remaining_percent(value).is_err(), "accepted {value}");
    }
}
