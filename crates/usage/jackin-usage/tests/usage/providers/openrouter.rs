// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_core::Provider;
use jackin_testkit::{Method, Route};
use jackin_usage::{Refresh, UnavailableReason, UsageOutcome, WindowKind};

use super::Case;
use crate::support::{
    Left, TestResult, UsageHarness, account, assert_windows, env_source, guard, spent, unavailable,
    want,
};

const KEY: &str = include_str!("../fixtures/openrouter/key.json");
const KEY_UNLIMITED: &str = include_str!("../fixtures/openrouter/key_unlimited.json");
pub(crate) const TOKEN: &str = "SECRET_TOKEN_OPENROUTER";
const KEY_PATH: &str = "/api/v1/key";
const CREDITS_PATH: &str = "/api/v1/credits";

const KEY_LIMIT: crate::support::Want = want(
    WindowKind::Spend,
    "Key limit",
    Some(24.68),
    Left::Amount("37.66", "USD"),
    None,
);

const KEY_SPEND: crate::support::Want =
    spent(WindowKind::Spend, "Key spend", Left::Amount("12.34", "USD"));

const DAILY_SPEND: crate::support::Want = spent(
    WindowKind::Spend,
    "Daily spend",
    Left::Amount("1.00", "USD"),
);

const WEEKLY_SPEND: crate::support::Want = spent(
    WindowKind::Spend,
    "Weekly spend",
    Left::Amount("5.00", "USD"),
);

const MONTHLY_SPEND: crate::support::Want = spent(
    WindowKind::Spend,
    "Monthly spend",
    Left::Amount("12.34", "USD"),
);

const SPENDS: [crate::support::Want; 4] = [KEY_SPEND, DAILY_SPEND, WEEKLY_SPEND, MONTHLY_SPEND];

pub(crate) fn serve_valid(harness: &UsageHarness, token: &str) {
    serve_key(harness, token, KEY);
}

fn serve_key(harness: &UsageHarness, token: &str, key: &str) {
    let bearer = format!("Bearer {token}");
    harness.server.replace(
        Provider::Openrouter,
        Method::Get,
        KEY_PATH,
        Route::json(200, key).requires_header("authorization", &bearer),
    );
}

pub(crate) async fn started()
-> Result<(UsageHarness, jackin_usage::UsageAccount), Box<dyn std::error::Error>> {
    let mut harness = UsageHarness::new().await?;
    harness.env("OPENROUTER_TEST_KEY", TOKEN);
    serve_key(&harness, TOKEN, KEY);
    let account = account(
        "router",
        Provider::Openrouter,
        env_source("OPENROUTER_TEST_KEY"),
    );
    Ok((harness, account))
}

pub(crate) async fn case() -> Result<Case, Box<dyn std::error::Error>> {
    let (harness, account) = started().await?;
    Ok(Case {
        provider: Provider::Openrouter,
        token: TOKEN,
        harness,
        account,
        first: (Method::Get, KEY_PATH),
        expected: &[
            KEY_LIMIT,
            KEY_SPEND,
            DAILY_SPEND,
            WEEKLY_SPEND,
            MONTHLY_SPEND,
        ],
    })
}

fn spent_texts(outcome: &UsageOutcome) -> Vec<String> {
    crate::support::windows_of(outcome)
        .iter()
        .map(jackin_usage::LimitWindow::spent_text)
        .collect()
}

async fn outcome(key: &str) -> Result<UsageOutcome, Box<dyn std::error::Error>> {
    let (harness, account) = started().await?;
    serve_key(&harness, TOKEN, key);
    Ok(harness.one(&account, Refresh::Cached).await?.outcome)
}

#[tokio::test]
async fn the_key_limit_and_the_spend_rows_give_five_windows_in_dollars() -> TestResult {
    guard(async {
        assert_windows(
            &outcome(KEY).await?,
            &[
                KEY_LIMIT,
                KEY_SPEND,
                DAILY_SPEND,
                WEEKLY_SPEND,
                MONTHLY_SPEND,
            ],
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_credits_endpoint_is_not_requested_any_more() -> TestResult {
    guard(async {
        let (harness, account) = started().await?;
        let usage = harness.one(&account, Refresh::Cached).await?;
        assert_windows(
            &usage.outcome,
            &[
                KEY_LIMIT,
                KEY_SPEND,
                DAILY_SPEND,
                WEEKLY_SPEND,
                MONTHLY_SPEND,
            ],
        );
        let mismatches = harness.server.mismatches();
        assert!(mismatches.is_empty(), "{mismatches:?}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_spend_rows_are_reported_whatever_the_key_limit_is() -> TestResult {
    guard(async {
        assert_windows(&outcome(KEY_UNLIMITED).await?, &SPENDS);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_key_without_a_limit_has_no_limit_window() -> TestResult {
    guard(async {
        let found = outcome(KEY_UNLIMITED).await?;
        let windows = crate::support::windows_of(&found);
        assert_eq!(windows.len(), 4);
        assert!(
            windows.iter().all(|window| window.label != "Key limit"),
            "the key limit window is gone"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_key_without_a_spend_has_no_spend_row() -> TestResult {
    guard(async {
        let key =
            "{\"data\":{\"label\":\"sk-or-v1-abc\",\"limit\":50.0,\"limit_remaining\":37.66}}";
        assert_windows(
            &outcome(key).await?,
            &[want(
                WindowKind::Spend,
                "Key limit",
                Some(24.68),
                Left::Amount("37.66", "USD"),
                None,
            )],
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_rejected_key_gives_an_expired_login() -> TestResult {
    guard(async {
        let (harness, account) = started().await?;
        harness.server.replace(
            Provider::Openrouter,
            Method::Get,
            KEY_PATH,
            Route::status(401),
        );
        let usage = harness.one(&account, Refresh::Cached).await?;
        assert_eq!(
            unavailable(&usage.outcome),
            &UnavailableReason::ExpiredLogin
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_key_response_without_a_data_object_is_a_provider_error() -> TestResult {
    guard(async {
        assert!(matches!(
            unavailable(&outcome("{\"data\":[]}").await?),
            UnavailableReason::Provider(_)
        ));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_key_limit_beyond_the_integers_is_a_provider_error() -> TestResult {
    guard(async {
        let key = "{\"data\":{\"limit\":1e300,\"limit_remaining\":1.0}}";
        assert!(matches!(
            unavailable(&outcome(key).await?),
            UnavailableReason::Provider(_)
        ));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_spend_beyond_the_integers_is_a_provider_error() -> TestResult {
    guard(async {
        let key = "{\"data\":{\"limit\":50.0,\"limit_remaining\":37.66,\"usage\":1e300}}";
        assert!(matches!(
            unavailable(&outcome(key).await?),
            UnavailableReason::Provider(_)
        ));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_credits_path_is_never_answered_by_the_provider() -> TestResult {
    guard(async {
        let (harness, account) = started().await?;
        harness.server.replace(
            Provider::Openrouter,
            Method::Get,
            CREDITS_PATH,
            Route::json(
                200,
                "{\"data\":{\"total_credits\":100.5,\"total_usage\":25.75}}",
            ),
        );
        let usage = harness.one(&account, Refresh::Cached).await?;
        assert_windows(
            &usage.outcome,
            &[
                KEY_LIMIT,
                KEY_SPEND,
                DAILY_SPEND,
                WEEKLY_SPEND,
                MONTHLY_SPEND,
            ],
        );
        let mismatches = harness.server.mismatches();
        assert!(mismatches.is_empty(), "{mismatches:?}");
        Ok(())
    })
    .await
}

async fn bounded(key: &str) -> Result<UsageOutcome, Box<dyn std::error::Error>> {
    tokio::time::timeout(Duration::from_secs(20), outcome(key)).await?
}

fn data(fields: &str) -> String {
    format!("{{\"data\":{{{fields}}}}}")
}

#[tokio::test]
async fn a_spend_below_a_cent_keeps_every_digit() -> TestResult {
    guard(async {
        let key = data("\"usage\":0.004,\"usage_daily\":\"0.0042\",\"usage_weekly\":1e-3,\"usage_monthly\":0.10000000000000001");
        assert_eq!(
            spent_texts(&bounded(&key).await?),
            [
                "0.004 USD spent",
                "0.0042 USD spent",
                "0.001 USD spent",
                "0.10000000000000001 USD spent"
            ]
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn exponent_and_padded_text_reach_the_report_expanded_and_a_zero_costs_nothing() -> TestResult
{
    guard(async {
        let key =
            data("\"usage\":1.5e-7,\"usage_daily\":\" 7.5 \",\"usage_weekly\":0e999999999999");
        assert_eq!(
            spent_texts(&bounded(&key).await?),
            ["0.00000015 USD spent", "7.50 USD spent", "0.00 USD spent"]
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn mixed_scales_subtract_without_rounding() -> TestResult {
    guard(async {
        let key = data("\"limit\":50,\"usage\":12.3456789012345678901");
        assert_windows(
            &bounded(&key).await?,
            &[
                want(
                    WindowKind::Spend,
                    "Key limit",
                    Some(24.691_357_802_469_14),
                    Left::Amount("37.6543210987654321099", "USD"),
                    None,
                ),
                spent(
                    WindowKind::Spend,
                    "Key spend",
                    Left::Amount("12.3456789012345678901", "USD"),
                ),
            ],
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_millionth_survives_between_trillion_dollar_operands() -> TestResult {
    guard(async {
        let key = data("\"limit\":1000000000000.000001,\"usage\":1000000000000");
        assert_windows(
            &bounded(&key).await?,
            &[
                want(
                    WindowKind::Spend,
                    "Key limit",
                    Some(100.0),
                    Left::Amount("0.000001", "USD"),
                    None,
                ),
                spent(
                    WindowKind::Spend,
                    "Key spend",
                    Left::Amount("1000000000000.00", "USD"),
                ),
            ],
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_key_limit_below_a_cent_still_has_a_window() -> TestResult {
    guard(async {
        let key = data("\"limit\":0.001,\"limit_remaining\":0.0004");
        assert_windows(
            &bounded(&key).await?,
            &[want(
                WindowKind::Spend,
                "Key limit",
                Some(60.0),
                Left::Amount("0.0004", "USD"),
                None,
            )],
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_limit_of_zero_or_less_has_no_window_and_a_remainder_above_the_limit_reads_unused()
-> TestResult {
    guard(async {
        for limit in ["0", "-1", "0.000"] {
            let key = data(&format!(
                "\"limit\":{limit},\"limit_remaining\":0,\"usage\":1"
            ));
            assert_eq!(
                spent_texts(&bounded(&key).await?),
                ["1.00 USD spent"],
                "{limit}"
            );
        }
        let key = data("\"limit\":10,\"limit_remaining\":12.5");
        assert_windows(
            &bounded(&key).await?,
            &[want(
                WindowKind::Spend,
                "Key limit",
                Some(0.0),
                Left::Amount("12.50", "USD"),
                None,
            )],
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_negative_remainder_clamps_and_a_negative_spend_is_reported() -> TestResult {
    guard(async {
        let key = data("\"limit\":10,\"limit_remaining\":-2.5,\"usage\":-0.25");
        assert_windows(
            &bounded(&key).await?,
            &[
                want(
                    WindowKind::Spend,
                    "Key limit",
                    Some(100.0),
                    Left::Amount("0.00", "USD"),
                    None,
                ),
                spent(WindowKind::Spend, "Key spend", Left::Amount("-0.25", "USD")),
            ],
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn unreported_and_unreadable_amounts_have_no_window() -> TestResult {
    guard(async {
        let key = data("\"limit\":null,\"usage\":null,\"usage_daily\":\"n/a\",\"usage_weekly\":true,\"usage_monthly\":2.5");
        assert_eq!(spent_texts(&bounded(&key).await?), ["2.50 USD spent"]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_digit_and_scale_bounds_accept_the_edge_and_refuse_beyond() -> TestResult {
    guard(async {
        let widest = "99999999999999999999999999999999999999";
        assert_eq!(
            spent_texts(&bounded(&data(&format!("\"usage\":{widest}"))).await?),
            [format!("{widest}.00 USD spent")]
        );
        assert_eq!(
            spent_texts(&bounded(&data("\"usage\":1e-38")).await?),
            ["0.00000000000000000000000000000000000001 USD spent"]
        );
        assert_eq!(
            spent_texts(&bounded(&data("\"usage\":1.1102230246251565e-16")).await?),
            ["0.00000000000000011102230246251565 USD spent"]
        );
        for beyond in [
            "100000000000000000000000000000000000000",
            "1e-39",
            "1e999999999999",
        ] {
            let found = bounded(&data(&format!("\"usage\":{beyond}"))).await?;
            assert!(
                matches!(unavailable(&found), UnavailableReason::Provider(_)),
                "{beyond}: {found:?}"
            );
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_exact_amounts_come_back_from_the_cache_unchanged() -> TestResult {
    guard(async {
        let (harness, account) = started().await?;
        serve_key(
            &harness,
            TOKEN,
            &data("\"limit\":1000000000000.000001,\"usage\":1000000000000,\"usage_daily\":0.004"),
        );
        let first = harness.one(&account, Refresh::Cached).await?;
        harness.server.replace(
            Provider::Openrouter,
            Method::Get,
            KEY_PATH,
            Route::status(500),
        );
        harness.advance(10);
        let again = harness.one(&account, Refresh::Cached).await?;
        assert_eq!(again.outcome, first.outcome);
        assert_eq!(
            spent_texts(&again.outcome),
            ["-", "1000000000000.00 USD spent", "0.004 USD spent"]
        );
        let cache = harness.cache_text().ok_or("no cache file")?;
        assert!(cache.contains("\"value\":\"0.000001\""), "{cache}");
        assert!(cache.contains("\"value\":\"0.004\""), "{cache}");
        Ok(())
    })
    .await
}
