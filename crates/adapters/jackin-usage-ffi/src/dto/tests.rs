// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn selected_account_route_dto_preserves_each_typed_state() {
    let cases = [
        (
            HostSelectedAccountRoute::Unselected,
            "unselected",
            None,
            None,
        ),
        (
            HostSelectedAccountRoute::Resolving {
                account_key: "persisted-key".to_owned(),
            },
            "resolving",
            Some("persisted-key"),
            None,
        ),
        (
            HostSelectedAccountRoute::Available {
                account_key: "persisted-key".to_owned(),
            },
            "available",
            Some("persisted-key"),
            None,
        ),
        (
            HostSelectedAccountRoute::Unavailable {
                account_key: "persisted-key".to_owned(),
                notice: jackin_usage::host::SELECTED_ACCOUNT_UNAVAILABLE_NOTICE,
            },
            "unavailable",
            Some("persisted-key"),
            Some(jackin_usage::host::SELECTED_ACCOUNT_UNAVAILABLE_NOTICE),
        ),
    ];

    for (route, expected_status, expected_key, expected_notice) in cases {
        let dto = selected_account_route_dto(route);
        assert_eq!(dto.status, expected_status);
        assert_eq!(dto.account_key.as_deref(), expected_key);
        assert_eq!(dto.notice.as_deref(), expected_notice);
    }
}

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
