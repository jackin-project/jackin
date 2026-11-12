// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::Timestamp;
use jackin_usage::{LimitWindow, Percent, Remaining, UnavailableReason, WindowKind};

const NOW: i64 = 1_767_225_600;

fn percent(value: f64) -> Percent {
    Percent::new(value).expect("a valid percent")
}

fn window(used: Option<f64>, remaining: Option<Remaining>, resets: Option<i64>) -> LimitWindow {
    LimitWindow {
        kind: WindowKind::Weekly,
        label: "weekly".to_owned(),
        used: used.map(percent),
        remaining,
        spent: None,
        resets_at: resets.map(Timestamp),
    }
}

fn spent_window(spent: Remaining) -> LimitWindow {
    LimitWindow {
        kind: WindowKind::Spend,
        label: "Key spend".to_owned(),
        used: None,
        remaining: None,
        spent: Some(spent),
        resets_at: None,
    }
}

#[test]
fn every_unavailable_reason_has_its_one_sentence() {
    let cases = [
        (UnavailableReason::NoCredentials, "no credentials"),
        (UnavailableReason::ExpiredLogin, "login expired or rejected"),
        (
            UnavailableReason::Unsupported { detail: None },
            "not supported for this provider",
        ),
        (
            UnavailableReason::Unsupported {
                detail: Some("no usage endpoint".to_owned()),
            },
            "not supported: no usage endpoint",
        ),
        (
            UnavailableReason::RateLimited { retry_at: None },
            "rate limited",
        ),
        (
            UnavailableReason::RateLimited {
                retry_at: Some(Timestamp(NOW)),
            },
            "rate limited, retry at 2026-01-01T00:00:00Z",
        ),
        (
            UnavailableReason::Network("timed out".to_owned()),
            "network error: timed out",
        ),
        (
            UnavailableReason::Provider("HTTP 500".to_owned()),
            "provider error: HTTP 500",
        ),
    ];
    for (reason, text) in cases {
        assert_eq!(reason.to_string(), text);
    }
}

#[test]
fn a_percent_reads_with_at_most_two_places_and_no_trailing_zeros() {
    let cases = [
        (0.0, "0%"),
        (0.004, "0%"),
        (12.5, "12.5%"),
        (33.333, "33.33%"),
        (99.999, "100%"),
        (100.0, "100%"),
    ];
    for (value, text) in cases {
        assert_eq!(percent(value).to_string(), text, "{value}");
    }
}

#[test]
fn a_window_reads_used_left_and_reset_and_a_dash_where_the_provider_said_nothing() {
    let full = window(
        Some(12.5),
        Some(Remaining::Percent(percent(87.5))),
        Some(NOW + 2 * 3_600 + 3 * 60 + 59),
    );
    assert_eq!(full.used_text(), "12.5% used");
    assert_eq!(full.remaining_text(), "87.5% left");
    assert_eq!(
        full.reset_text(Timestamp(NOW)),
        "resets 2026-01-01T02:03:59Z (in 2h3m)"
    );

    let spend = window(
        None,
        Some(Remaining::Amount {
            value: "4.20".to_owned(),
            unit: "USD".to_owned(),
        }),
        Some(NOW + 3 * 86_400 + 4 * 3_600),
    );
    assert_eq!(spend.used_text(), "-");
    assert_eq!(spend.remaining_text(), "4.20 USD left");
    assert_eq!(
        spend.reset_text(Timestamp(NOW)),
        "resets 2026-01-04T04:00:00Z (in 3d4h)"
    );

    let spent = spent_window(Remaining::Amount {
        value: "4.20".to_owned(),
        unit: "USD".to_owned(),
    });
    assert_eq!(spent.spent_text(), "4.20 USD spent");
    assert_eq!(spent.used_text(), "-");
    assert_eq!(spent.remaining_text(), "-");

    let silent = window(None, None, None);
    assert_eq!(
        (
            silent.used_text(),
            silent.remaining_text(),
            silent.reset_text(Timestamp(NOW))
        ),
        ("-".to_owned(), "-".to_owned(), "-".to_owned())
    );
    let past = window(None, None, Some(NOW - 1));
    assert_eq!(
        past.reset_text(Timestamp(NOW)),
        "resets 2025-12-31T23:59:59Z (passed)"
    );
}

#[test]
fn a_reset_reads_as_passed_at_the_instant_and_as_days_from_a_full_day() {
    let reset = |offset: i64| window(None, None, Some(NOW + offset)).reset_text(Timestamp(NOW));
    assert_eq!(reset(0), "resets 2026-01-01T00:00:00Z (passed)");
    assert!(!reset(1).contains("passed"), "{}", reset(1));
    assert_eq!(reset(86_400 - 1), "resets 2026-01-01T23:59:59Z (in 23h59m)");
    assert_eq!(reset(86_400), "resets 2026-01-02T00:00:00Z (in 1d0h)");
}
