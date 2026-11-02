// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::Provider;
use jackin_testkit::{Method, Route};
use jackin_usage::{Refresh, UnavailableReason, UsageAccount, UsageOutcome, WindowKind};

use super::Case;
use crate::support::{
    Left, TestResult, UsageHarness, Want, account, assert_windows, env_source, file_source, guard,
    unavailable, want, windows_of,
};

const TOKEN: &str = "SECRET_TOKEN_KIMI";
const PATH: &str = "/coding/v1/usages";
const USAGES: &str = include_str!("../fixtures/kimi/usages.json");
const HOURS: &str = include_str!("../fixtures/kimi/usages_hours.json");
const NO_WINDOW: &str = include_str!("../fixtures/kimi/usages_no_window.json");
const OVER: &str = include_str!("../fixtures/kimi/usages_over.json");
const CODE: &str = include_str!("../fixtures/kimi/kimi_code.json");
const EXPIRED: &str = include_str!("../fixtures/kimi/kimi_code_expired.json");
const NO_TOKEN: &str = include_str!("../fixtures/kimi/kimi_code_no_token.json");
const LOGIN_PATH: &str = "/home/work/.kimi/kimi-code.json";

const FIVE: Want = want(
    WindowKind::FiveHour,
    "5-hour",
    Some(25.0),
    Left::Percent(75.0),
    Some(1_767_243_600),
);
const WEEK: Want = want(
    WindowKind::Weekly,
    "weekly",
    Some(30.0),
    Left::Percent(70.0),
    Some(1_767_830_400),
);

fn serve(harness: &UsageHarness, body: &str) {
    harness.server.replace(
        Provider::Kimi,
        Method::Get,
        PATH,
        Route::json(200, body).requires_header("authorization", &format!("Bearer {TOKEN}")),
    );
}

async fn started(body: &str) -> Result<(UsageHarness, UsageAccount), Box<dyn std::error::Error>> {
    let mut harness = UsageHarness::new().await?;
    harness.env("KIMI_TEST_KEY", TOKEN);
    serve(&harness, body);
    Ok((
        harness,
        account("kimi", Provider::Kimi, env_source("KIMI_TEST_KEY")),
    ))
}

pub(crate) async fn case() -> Result<Case, Box<dyn std::error::Error>> {
    let (harness, account) = started(USAGES).await?;
    Ok(Case {
        provider: Provider::Kimi,
        harness,
        account,
        first: (Method::Get, PATH),
        expected: &[FIVE, WEEK],
    })
}

async fn outcome(body: &str) -> Result<UsageOutcome, Box<dyn std::error::Error>> {
    let (harness, account) = started(body).await?;
    Ok(harness.one(&account, Refresh::Cached).await?.outcome)
}

fn assert_provider(outcome: &UsageOutcome) {
    assert!(
        matches!(unavailable(outcome), UnavailableReason::Provider(_)),
        "{outcome:?}"
    );
}

#[tokio::test]
async fn the_limits_come_first_and_the_summary_is_the_weekly_window() -> TestResult {
    guard(async {
        assert_windows(&outcome(USAGES).await?, &[FIVE, WEEK]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_window_in_hours_with_numeric_counts_is_the_five_hour_window() -> TestResult {
    guard(async {
        assert_windows(&outcome(HOURS).await?, &[FIVE, WEEK]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_limit_without_a_window_is_labelled_by_its_name() -> TestResult {
    guard(async {
        let burst = want(
            WindowKind::Other,
            "Burst",
            Some(10.0),
            Left::Percent(90.0),
            Some(1_767_243_600),
        );
        assert_windows(&outcome(NO_WINDOW).await?, &[burst]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn usage_above_the_limit_is_clamped_to_a_full_window() -> TestResult {
    guard(async {
        let full = want(
            WindowKind::Weekly,
            "weekly",
            Some(100.0),
            Left::Percent(0.0),
            None,
        );
        assert_windows(&outcome(OVER).await?, &[full]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_empty_object_is_a_provider_error() -> TestResult {
    guard(async {
        assert_provider(&outcome("{}").await?);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_empty_summary_and_no_limits_is_a_provider_error() -> TestResult {
    guard(async {
        assert_provider(&outcome("{\"usage\":{},\"limits\":[]}").await?);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_zero_limit_drops_the_row_and_leaves_a_provider_error() -> TestResult {
    guard(async {
        assert_provider(&outcome("{\"usage\":{\"limit\":0,\"used\":0}}").await?);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_404_is_a_provider_error() -> TestResult {
    guard(async {
        let (harness, account) = started(USAGES).await?;
        harness
            .server
            .replace(Provider::Kimi, Method::Get, PATH, Route::status(404));
        let usage = harness.one(&account, Refresh::Cached).await?;
        assert_provider(&usage.outcome);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_login_file_gives_its_access_token_port_fidelity() -> TestResult {
    guard(async {
        let (harness, _) = started(USAGES).await?;
        harness.put(LOGIN_PATH, CODE).await?;
        let login = account("kimi", Provider::Kimi, file_source(LOGIN_PATH));
        let usage = harness.one(&login, Refresh::Cached).await?;
        assert_eq!(windows_of(&usage.outcome).len(), 2);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_login_file_without_an_access_token_is_a_provider_error_port_fidelity() -> TestResult {
    guard(async {
        let (harness, _) = started(USAGES).await?;
        harness.put(LOGIN_PATH, NO_TOKEN).await?;
        let login = account("kimi", Provider::Kimi, file_source(LOGIN_PATH));
        let usage = harness.one(&login, Refresh::Cached).await?;
        match unavailable(&usage.outcome) {
            UnavailableReason::Provider(message) => assert!(!message.contains("SECRET_TEXT_KIMI")),
            other => panic!("expected a provider reason, found {other:?}"),
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_login_that_expired_before_now_is_an_expired_login_without_a_request() -> TestResult {
    guard(async {
        let (harness, _) = started(USAGES).await?;
        harness.put(LOGIN_PATH, EXPIRED).await?;
        let login = account("kimi", Provider::Kimi, file_source(LOGIN_PATH));
        let usage = harness.one(&login, Refresh::Cached).await?;
        assert_eq!(
            unavailable(&usage.outcome),
            &UnavailableReason::ExpiredLogin
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_window_length_that_overflows_is_a_provider_error() -> TestResult {
    guard(async {
        let body = "{\"limits\":[{\"window\":{\"duration\":9223372036854775807,\"timeUnit\":\"TIME_UNIT_HOUR\"},\"detail\":{\"limit\":10,\"used\":1}}]}";
        assert_provider(&outcome(body).await?);
        Ok(())
    })
    .await
}
