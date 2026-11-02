// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::Provider;
use jackin_testkit::{Method, Route};
use jackin_usage::{Refresh, UnavailableReason, UsageAccount, UsageOutcome, WindowKind};
use serde_json::json;

use super::Case;
use crate::support::{
    Left, TestResult, UsageHarness, Want, account, assert_windows, env_source, file_source, guard,
    unavailable, want,
};

const TOKEN: &str = "SECRET_TOKEN_CURSOR";
const PATH: &str = "/aiserver.v1.DashboardService/GetCurrentPeriodUsage";
const PERIOD: &str = include_str!("../fixtures/cursor/current_period_usage.json");
const NO_PLAN: &str = include_str!("../fixtures/cursor/current_period_no_plan.json");
const AUTH: &str = include_str!("../fixtures/cursor/auth.json");
const NO_TOKEN: &str = include_str!("../fixtures/cursor/auth_no_token.json");
const AUTH_PATH: &str = "/home/work/.config/cursor/auth.json";

const TOTAL: Want = want(
    WindowKind::Monthly,
    "monthly",
    Some(11.597),
    Left::Percent(88.403),
    Some(1_792_134_030),
);
const AUTO: Want = want(
    WindowKind::Monthly,
    "Auto monthly",
    Some(10.289),
    Left::Percent(89.711),
    Some(1_792_134_030),
);
const API: Want = want(
    WindowKind::Monthly,
    "API monthly",
    Some(50.82),
    Left::Percent(49.18),
    Some(1_792_134_030),
);

fn serve(harness: &UsageHarness, token: &str, body: &str) {
    harness.server.replace(
        Provider::Cursor,
        Method::Post,
        PATH,
        Route::json(200, body)
            .requires_header("authorization", &format!("Bearer {token}"))
            .requires_header("connect-protocol-version", "1")
            .requires_header("content-type", "application/json")
            .requires_json_body(json!({})),
    );
}

async fn started(body: &str) -> Result<(UsageHarness, UsageAccount), Box<dyn std::error::Error>> {
    let mut harness = UsageHarness::new().await?;
    harness.env("CURSOR_TEST_KEY", TOKEN);
    serve(&harness, TOKEN, body);
    Ok((
        harness,
        account("cursor", Provider::Cursor, env_source("CURSOR_TEST_KEY")),
    ))
}

pub(crate) async fn case() -> Result<Case, Box<dyn std::error::Error>> {
    let (harness, account) = started(PERIOD).await?;
    Ok(Case {
        provider: Provider::Cursor,
        harness,
        account,
        first: (Method::Post, PATH),
        expected: &[TOTAL, AUTO, API],
    })
}

async fn outcome(body: &str) -> Result<UsageOutcome, Box<dyn std::error::Error>> {
    let (harness, account) = started(body).await?;
    Ok(harness.one(&account, Refresh::Cached).await?.outcome)
}

#[tokio::test]
async fn the_billing_cycle_gives_three_monthly_windows_port_fidelity() -> TestResult {
    guard(async {
        assert_windows(&outcome(PERIOD).await?, &[TOTAL, AUTO, API]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_response_without_plan_usage_is_a_provider_error() -> TestResult {
    guard(async {
        let outcome = outcome(NO_PLAN).await?;
        assert!(
            matches!(unavailable(&outcome), UnavailableReason::Provider(_)),
            "{outcome:?}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_connect_unauthenticated_401_is_an_expired_login() -> TestResult {
    guard(async {
        let (harness, account) = started(PERIOD).await?;
        harness.server.replace(
            Provider::Cursor,
            Method::Post,
            PATH,
            Route::json(401, "{\"code\":\"unauthenticated\"}"),
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
async fn an_auth_file_gives_its_access_token_port_fidelity() -> TestResult {
    guard(async {
        let (harness, _) = started(PERIOD).await?;
        harness.put(AUTH_PATH, AUTH).await?;
        let login = account("cursor", Provider::Cursor, file_source(AUTH_PATH));
        let usage = harness.one(&login, Refresh::Cached).await?;
        assert_windows(&usage.outcome, &[TOTAL, AUTO, API]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_auth_file_without_an_access_token_is_a_provider_error_port_fidelity() -> TestResult {
    guard(async {
        let (harness, _) = started(PERIOD).await?;
        harness.put(AUTH_PATH, NO_TOKEN).await?;
        let login = account("cursor", Provider::Cursor, file_source(AUTH_PATH));
        let usage = harness.one(&login, Refresh::Cached).await?;
        match unavailable(&usage.outcome) {
            UnavailableReason::Provider(message) => {
                assert!(!message.contains("SECRET_TEXT_CURSOR"));
            }
            other => panic!("expected a provider reason, found {other:?}"),
        }
        Ok(())
    })
    .await
}
