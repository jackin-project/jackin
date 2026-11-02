// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::Provider;
use jackin_testkit::{Method, Route};
use jackin_usage::{Refresh, UnavailableReason, UsageAccount, UsageOutcome, WindowKind};

use super::Case;
use crate::support::{
    Left, TestResult, UsageHarness, Want, account, assert_windows, env_source, file_source, guard,
    unavailable, want,
};

const TOKEN: &str = "SECRET_TOKEN_OPENCODE";
const PATH: &str = "/zen/go/v1/usage";
const USAGE: &str = include_str!("../fixtures/opencode/usage.json");
const NO_MONTHLY: &str = include_str!("../fixtures/opencode/usage_missing_monthly.json");
const LIMITED: &str = include_str!("../fixtures/opencode/usage_rate_limited.json");
const BAD_PERCENT: &str = include_str!("../fixtures/opencode/usage_bad_percent.json");
const AUTH: &str = include_str!("../fixtures/opencode/auth.json");
const AUTH_OAUTH: &str = include_str!("../fixtures/opencode/auth_go_oauth.json");
const AUTH_NO_GO: &str = include_str!("../fixtures/opencode/auth_no_go.json");
const AUTH_PATH: &str = "/home/work/.local/share/opencode/auth.json";

const FIVE: Want = want(
    WindowKind::FiveHour,
    "5-hour",
    Some(20.0),
    Left::Percent(80.0),
    Some(1_767_243_600),
);
const WEEK: Want = want(
    WindowKind::Weekly,
    "weekly",
    Some(35.0),
    Left::Percent(65.0),
    Some(1_767_830_400),
);
const MONTH: Want = want(
    WindowKind::Monthly,
    "monthly",
    Some(10.0),
    Left::Percent(90.0),
    Some(1_769_817_600),
);

async fn started(body: &str) -> Result<(UsageHarness, UsageAccount), Box<dyn std::error::Error>> {
    let mut harness = UsageHarness::new().await?;
    harness.env("OPENCODE_TEST_KEY", TOKEN);
    harness.server.replace(
        Provider::Opencode,
        Method::Get,
        PATH,
        Route::json(200, body).requires_header("authorization", &format!("Bearer {TOKEN}")),
    );
    Ok((
        harness,
        account(
            "opencode",
            Provider::Opencode,
            env_source("OPENCODE_TEST_KEY"),
        ),
    ))
}

pub(crate) async fn case() -> Result<Case, Box<dyn std::error::Error>> {
    let (harness, account) = started(USAGE).await?;
    Ok(Case {
        provider: Provider::Opencode,
        token: TOKEN,
        harness,
        account,
        first: (Method::Get, PATH),
        expected: &[FIVE, WEEK, MONTH],
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
async fn the_rolling_weekly_and_monthly_windows_are_five_hour_weekly_and_monthly() -> TestResult {
    guard(async {
        assert_windows(&outcome(USAGE).await?, &[FIVE, WEEK, MONTH]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_missing_monthly_window_leaves_two_windows() -> TestResult {
    guard(async {
        assert_windows(&outcome(NO_MONTHLY).await?, &[FIVE, WEEK]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_rate_limited_window_is_full_port_fidelity() -> TestResult {
    guard(async {
        let full = want(
            WindowKind::Weekly,
            "weekly",
            Some(100.0),
            Left::Percent(0.0),
            Some(1_767_830_400),
        );
        assert_windows(&outcome(LIMITED).await?, &[FIVE, full, MONTH]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_window_with_a_wrong_typed_percent_is_dropped_and_the_others_stay() -> TestResult {
    guard(async {
        assert_windows(&outcome(BAD_PERCENT).await?, &[WEEK, MONTH]);
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
async fn a_usage_array_is_a_provider_error() -> TestResult {
    guard(async {
        assert_provider(&outcome("{\"usage\":[]}").await?);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_403_is_an_expired_login() -> TestResult {
    guard(async {
        let (harness, account) = started(USAGE).await?;
        harness
            .server
            .replace(Provider::Opencode, Method::Get, PATH, Route::status(403));
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
async fn an_auth_file_with_several_providers_gives_the_opencode_go_key_port_fidelity() -> TestResult
{
    guard(async {
        let (harness, _) = started(USAGE).await?;
        harness.put(AUTH_PATH, AUTH).await?;
        let login = account("opencode", Provider::Opencode, file_source(AUTH_PATH));
        let usage = harness.one(&login, Refresh::Cached).await?;
        assert_windows(&usage.outcome, &[FIVE, WEEK, MONTH]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_auth_file_without_the_opencode_go_entry_is_a_provider_error_port_fidelity() -> TestResult
{
    guard(async {
        let (harness, _) = started(USAGE).await?;
        harness.put(AUTH_PATH, AUTH_NO_GO).await?;
        let login = account("opencode", Provider::Opencode, file_source(AUTH_PATH));
        let usage = harness.one(&login, Refresh::Cached).await?;
        match unavailable(&usage.outcome) {
            UnavailableReason::Provider(message) => {
                assert!(!message.contains("SECRET_TOKEN_OTHER"));
            }
            other => panic!("expected a provider reason, found {other:?}"),
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_opencode_go_entry_that_is_not_an_api_key_is_a_provider_error_port_fidelity()
-> TestResult {
    guard(async {
        let (harness, _) = started(USAGE).await?;
        harness.put(AUTH_PATH, AUTH_OAUTH).await?;
        let login = account("opencode", Provider::Opencode, file_source(AUTH_PATH));
        let usage = harness.one(&login, Refresh::Cached).await?;
        match unavailable(&usage.outcome) {
            UnavailableReason::Provider(message) => {
                assert!(!message.contains("SECRET_TEXT_OPENCODE"));
            }
            other => panic!("expected a provider reason, found {other:?}"),
        }
        Ok(())
    })
    .await
}
