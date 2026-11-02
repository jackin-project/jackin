// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{AccountName, Provider, Timestamp};
use jackin_usage::{
    AccountUsage, Freshness, LimitWindow, Percent, Remaining, UnavailableReason, UsageError,
    UsageOutcome, UsageReport, WindowKind,
};
use serde_json::{Value, json};

use crate::support::{TestResult, guard};

const GOLDEN: &str = include_str!("golden/report.json");

fn percent(value: f64) -> Percent {
    Percent::new(value).expect("a valid percent")
}

fn window(
    kind: WindowKind,
    label: &str,
    used: Option<f64>,
    left: Option<Remaining>,
    at: Option<i64>,
) -> LimitWindow {
    LimitWindow {
        kind,
        label: label.to_owned(),
        used: used.map(percent),
        remaining: left,
        resets_at: at.map(Timestamp),
    }
}

fn amount(value: &str, unit: &str) -> Option<Remaining> {
    Some(Remaining::Amount {
        value: value.to_owned(),
        unit: unit.to_owned(),
    })
}

fn left(value: f64) -> Option<Remaining> {
    Some(Remaining::Percent(percent(value)))
}

fn account(
    name: &str,
    provider: Provider,
    freshness: Freshness,
    outcome: UsageOutcome,
) -> AccountUsage {
    AccountUsage {
        account: AccountName::parse(name).expect("account name"),
        provider,
        freshness,
        outcome,
    }
}

fn reason(name: &str, provider: Provider, reason: UnavailableReason) -> AccountUsage {
    account(
        name,
        provider,
        Freshness::Fresh,
        UsageOutcome::Unavailable(reason),
    )
}

fn fixed_report() -> UsageReport {
    let all_kinds = vec![
        window(
            WindowKind::FiveHour,
            "5-hour",
            Some(42.0),
            left(58.0),
            Some(1_767_232_800),
        ),
        window(
            WindowKind::Weekly,
            "weekly",
            Some(12.0),
            left(88.0),
            Some(1_767_571_200),
        ),
        window(WindowKind::Monthly, "monthly", Some(10.0), left(90.0), None),
        window(
            WindowKind::Credits,
            "credits",
            None,
            amount("12.40", "credits"),
            None,
        ),
        window(
            WindowKind::Spend,
            "Individual limit",
            Some(30.0),
            amount("70.00", "USD"),
            Some(1_767_312_000),
        ),
        window(WindowKind::Other, "2 hours window", None, None, None),
    ];
    let stale = vec![window(
        WindowKind::FiveHour,
        "5-hour",
        Some(7.5),
        left(92.5),
        Some(1_767_232_800),
    )];
    UsageReport {
        schema: 1,
        generated_at: Timestamp(1_767_225_600),
        accounts: vec![
            account(
                "all-kinds",
                Provider::Codex,
                Freshness::Fresh,
                UsageOutcome::Windows(all_kinds),
            ),
            account(
                "stale",
                Provider::Claude,
                Freshness::Stale {
                    since: Timestamp(1_767_222_000),
                },
                UsageOutcome::Windows(stale),
            ),
            reason(
                "no-credentials",
                Provider::Grok,
                UnavailableReason::NoCredentials,
            ),
            reason("expired", Provider::Kimi, UnavailableReason::ExpiredLogin),
            reason(
                "plain-unsupported",
                Provider::Muse,
                UnavailableReason::Unsupported { detail: None },
            ),
            reason(
                "detailed-unsupported",
                Provider::Gemini,
                UnavailableReason::Unsupported {
                    detail: Some("Gemini API keys have no quota method".to_owned()),
                },
            ),
            reason(
                "limited-open",
                Provider::Zai,
                UnavailableReason::RateLimited { retry_at: None },
            ),
            reason(
                "limited-until",
                Provider::Minimax,
                UnavailableReason::RateLimited {
                    retry_at: Some(Timestamp(1_767_226_200)),
                },
            ),
            reason(
                "offline",
                Provider::Cursor,
                UnavailableReason::Network("the connection failed".to_owned()),
            ),
            reason(
                "broken",
                Provider::Openrouter,
                UnavailableReason::Provider("the provider answered HTTP 500".to_owned()),
            ),
        ],
    }
}

fn golden() -> Value {
    serde_json::from_str(GOLDEN).expect("the golden file is JSON")
}

fn keys(value: &Value) -> Vec<&str> {
    let mut keys: Vec<&str> = value
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    keys
}

#[tokio::test]
async fn the_serialized_report_equals_the_golden_file() -> TestResult {
    guard(async {
        assert_eq!(serde_json::to_value(fixed_report())?, golden());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_golden_file_round_trips_through_the_types() -> TestResult {
    guard(async {
        let parsed: UsageReport = serde_json::from_value(golden())?;
        assert_eq!(parsed, fixed_report());
        assert_eq!(serde_json::to_value(&parsed)?, golden());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_key_sets_are_the_documented_ones() -> TestResult {
    guard(async {
        let value = serde_json::to_value(fixed_report())?;
        assert_eq!(keys(&value), ["accounts", "generated_at", "schema"]);
        assert_eq!(value["schema"], 1);
        let first = &value["accounts"][0];
        assert_eq!(keys(first), ["account", "freshness", "outcome", "provider"]);
        let window = &first["outcome"]["windows"][0];
        assert_eq!(
            keys(window),
            ["kind", "label", "remaining", "resets_at", "used_percent"]
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_window_without_a_label_does_not_deserialize() -> TestResult {
    guard(async {
        let without_label =
            json!({"kind": "weekly", "used_percent": 1.0, "remaining": null, "resets_at": null});
        serde_json::from_value::<LimitWindow>(without_label).unwrap_err();
        let without_kind =
            json!({"label": "weekly", "used_percent": 1.0, "remaining": null, "resets_at": null});
        serde_json::from_value::<LimitWindow>(without_kind).unwrap_err();
        Ok(())
    })
    .await
}

#[tokio::test]
async fn every_provider_round_trips_through_a_report() -> TestResult {
    guard(async {
        for provider in Provider::ALL {
            let report = UsageReport {
                schema: 1,
                generated_at: Timestamp(1),
                accounts: vec![reason("round", provider, UnavailableReason::NoCredentials)],
            };
            let back: UsageReport = serde_json::from_value(serde_json::to_value(&report)?)?;
            assert_eq!(back, report, "{provider:?}");
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_percent_accepts_the_range_and_round_trips() -> TestResult {
    guard(async {
        for value in [0.0, 37.5, 100.0] {
            let percent = Percent::new(value)?;
            let back: Percent = serde_json::from_value(serde_json::to_value(percent)?)?;
            assert_eq!(back, percent);
            assert!((back.get() - value).abs() < 1e-9);
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_percent_rejects_values_outside_the_range_and_non_finite_values() -> TestResult {
    guard(async {
        for value in [-0.1, 100.1, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(
                matches!(Percent::new(value), Err(UsageError::InvalidPercent(_))),
                "{value}"
            );
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_percent_above_one_hundred_does_not_deserialize() -> TestResult {
    guard(async {
        serde_json::from_value::<Percent>(json!(101.0)).unwrap_err();
        Ok(())
    })
    .await
}
