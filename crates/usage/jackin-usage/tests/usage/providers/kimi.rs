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
const PARTIAL: &str = include_str!("../fixtures/kimi/usages_partial.json");
const NO_RATIO: &str = include_str!("../fixtures/kimi/usages_no_ratio.json");
const OVER: &str = include_str!("../fixtures/kimi/usages_over.json");
const EMPTY: &str = include_str!("../fixtures/kimi/usages_empty.json");
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
const MONTH: Want = want(
    WindowKind::Monthly,
    "monthly",
    Some(50.0),
    Left::Percent(50.0),
    Some(1_769_817_600),
);
const MONTH_CODE: Want = want(
    WindowKind::Monthly,
    "monthly code",
    Some(10.0),
    Left::Percent(90.0),
    Some(1_769_817_600),
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
        token: TOKEN,
        harness,
        account,
        first: (Method::Get, PATH),
        expected: &[FIVE, WEEK, MONTH, MONTH_CODE],
    })
}

pub(crate) async fn login_case() -> Result<Case, Box<dyn std::error::Error>> {
    let case = case().await?;
    case.harness.put(LOGIN_PATH, CODE).await?;
    let login = account("kimi", Provider::Kimi, file_source(LOGIN_PATH));
    Ok(Case {
        account: login,
        ..case
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
async fn the_four_usages_give_the_five_hour_weekly_and_monthly_windows_port_fidelity() -> TestResult
{
    guard(async {
        assert_windows(&outcome(USAGES).await?, &[FIVE, WEEK, MONTH, MONTH_CODE]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_usage_that_is_not_there_leaves_no_window_port_fidelity() -> TestResult {
    guard(async {
        assert_windows(&outcome(PARTIAL).await?, &[FIVE, WEEK]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_usage_above_the_limit_is_clamped_to_a_full_window_port_fidelity() -> TestResult {
    guard(async {
        let full = want(
            WindowKind::FiveHour,
            "5-hour",
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
async fn a_usage_without_a_ratio_is_a_provider_error_port_fidelity() -> TestResult {
    guard(async {
        assert_provider(&outcome(NO_RATIO).await?);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_usages_map_that_is_not_an_object_is_a_provider_error_port_fidelity() -> TestResult {
    guard(async {
        assert_provider(&outcome(EMPTY).await?);
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
        assert_eq!(windows_of(&usage.outcome).len(), 4);
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
