// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::Provider;
use jackin_testkit::{Method, Route};
use jackin_usage::{Refresh, UnavailableReason, WindowKind};

use super::Case;
use crate::support::{
    Left, TestResult, UsageHarness, account, assert_windows, env_source, guard, unavailable, want,
};

const KEY: &str = include_str!("../fixtures/openrouter/key.json");
const KEY_UNLIMITED: &str = include_str!("../fixtures/openrouter/key_unlimited.json");
const CREDITS: &str = include_str!("../fixtures/openrouter/credits.json");
const CREDITS_403: &str = include_str!("../fixtures/openrouter/credits_403.json");
const CREDITS_OVER: &str = include_str!("../fixtures/openrouter/credits_over.json");
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

const ACCOUNT_CREDITS: crate::support::Want = want(
    WindowKind::Credits,
    "Account credits",
    Some(25.621_890_547_263_682),
    Left::Amount("74.75", "USD"),
    None,
);

pub(crate) fn serve_valid(harness: &UsageHarness, token: &str) {
    serve_with(harness, token, KEY, Route::json(200, CREDITS));
}

fn serve(harness: &UsageHarness, key: &str, credits: Route) {
    serve_with(harness, TOKEN, key, credits);
}

fn serve_with(harness: &UsageHarness, token: &str, key: &str, credits: Route) {
    let bearer = format!("Bearer {token}");
    harness.server.replace(
        Provider::Openrouter,
        Method::Get,
        KEY_PATH,
        Route::json(200, key).requires_header("authorization", &bearer),
    );
    harness.server.replace(
        Provider::Openrouter,
        Method::Get,
        CREDITS_PATH,
        credits.requires_header("authorization", &bearer),
    );
}

pub(crate) async fn started()
-> Result<(UsageHarness, jackin_usage::UsageAccount), Box<dyn std::error::Error>> {
    let mut harness = UsageHarness::new().await?;
    harness.env("OPENROUTER_TEST_KEY", TOKEN);
    serve(&harness, KEY, Route::json(200, CREDITS));
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
        harness,
        account,
        first: (Method::Get, KEY_PATH),
        expected: &[KEY_LIMIT, ACCOUNT_CREDITS],
    })
}

#[tokio::test]
async fn a_key_limit_and_the_account_credits_give_two_windows_in_dollars() -> TestResult {
    guard(async {
        let (harness, account) = started().await?;
        let usage = harness.one(&account, Refresh::Cached).await?;
        assert_windows(&usage.outcome, &[KEY_LIMIT, ACCOUNT_CREDITS]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn usage_above_the_credits_shows_a_full_bar_and_zero_left() -> TestResult {
    guard(async {
        let (harness, account) = started().await?;
        serve(&harness, KEY_UNLIMITED, Route::json(200, CREDITS_OVER));
        let usage = harness.one(&account, Refresh::Cached).await?;
        assert_windows(
            &usage.outcome,
            &[want(
                WindowKind::Credits,
                "Account credits",
                Some(100.0),
                Left::Amount("0.00", "USD"),
                None,
            )],
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_unlimited_key_with_credits_shows_only_the_credits() -> TestResult {
    guard(async {
        let (harness, account) = started().await?;
        serve(&harness, KEY_UNLIMITED, Route::json(200, CREDITS));
        let usage = harness.one(&account, Refresh::Cached).await?;
        assert_windows(&usage.outcome, &[ACCOUNT_CREDITS]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_unlimited_key_whose_credits_are_forbidden_has_no_window() -> TestResult {
    guard(async {
        let (harness, account) = started().await?;
        serve(&harness, KEY_UNLIMITED, Route::json(403, CREDITS_403));
        let usage = harness.one(&account, Refresh::Cached).await?;
        assert!(matches!(
            unavailable(&usage.outcome),
            UnavailableReason::Provider(_)
        ));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn forbidden_credits_leave_the_key_limit_window() -> TestResult {
    guard(async {
        let (harness, account) = started().await?;
        serve(&harness, KEY, Route::json(403, CREDITS_403));
        let usage = harness.one(&account, Refresh::Cached).await?;
        assert_windows(&usage.outcome, &[KEY_LIMIT]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_rejected_key_stops_before_the_credits_request() -> TestResult {
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
async fn rejected_credits_are_an_expired_login() -> TestResult {
    guard(async {
        let (harness, account) = started().await?;
        serve(&harness, KEY, Route::status(401));
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
async fn a_credits_server_error_fails_the_account_with_no_partial_window() -> TestResult {
    guard(async {
        let (harness, account) = started().await?;
        serve(&harness, KEY, Route::status(500));
        let usage = harness.one(&account, Refresh::Cached).await?;
        assert!(matches!(
            unavailable(&usage.outcome),
            UnavailableReason::Provider(_)
        ));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_key_response_without_a_data_object_is_a_provider_error() -> TestResult {
    guard(async {
        let (harness, account) = started().await?;
        serve(&harness, "{\"data\":[]}", Route::json(200, CREDITS));
        let usage = harness.one(&account, Refresh::Cached).await?;
        assert!(matches!(
            unavailable(&usage.outcome),
            UnavailableReason::Provider(_)
        ));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn negative_total_credits_are_a_provider_error() -> TestResult {
    guard(async {
        let (harness, account) = started().await?;
        serve(
            &harness,
            KEY,
            Route::json(200, "{\"data\":{\"total_credits\":-1,\"total_usage\":0}}"),
        );
        let usage = harness.one(&account, Refresh::Cached).await?;
        assert!(matches!(
            unavailable(&usage.outcome),
            UnavailableReason::Provider(_)
        ));
        Ok(())
    })
    .await
}
