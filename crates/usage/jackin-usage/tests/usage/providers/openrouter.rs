// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::Provider;
use jackin_testkit::{Method, Route};
use jackin_usage::{Refresh, UnavailableReason, WindowKind};

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

async fn outcome(key: &str) -> Result<jackin_usage::UsageOutcome, Box<dyn std::error::Error>> {
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
