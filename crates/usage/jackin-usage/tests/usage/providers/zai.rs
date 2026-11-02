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

const TOKEN: &str = "SECRET_TOKEN_ZAI";
const PATH: &str = "/api/monitor/usage/quota/limit";
const QUOTA: &str = include_str!("../fixtures/zai/quota_limit.json");
const CREDIT: &str = include_str!("../fixtures/zai/quota_credit_limit.json");
const REJECTED: &str = include_str!("../fixtures/zai/quota_rejected.json");
const UNKNOWN_UNIT: &str = include_str!("../fixtures/zai/quota_unknown_unit.json");
const NEGATIVE: &str = include_str!("../fixtures/zai/quota_negative.json");
const SUCCESS_FALSE: &str = include_str!("../fixtures/zai/quota_success_false.json");
const CODE_500: &str = include_str!("../fixtures/zai/quota_code_500.json");
const NO_LIMITS: &str = include_str!("../fixtures/zai/quota_no_limits.json");

const FIVE: Want = want(
    WindowKind::FiveHour,
    "5-hour",
    Some(42.0),
    Left::Percent(58.0),
    Some(1_767_232_800),
);
const WEEK: Want = want(
    WindowKind::Weekly,
    "weekly",
    Some(12.0),
    Left::Percent(88.0),
    Some(1_767_571_200),
);

async fn started(body: &str) -> Result<(UsageHarness, UsageAccount), Box<dyn std::error::Error>> {
    let mut harness = UsageHarness::new().await?;
    harness.env("ZAI_TEST_KEY", TOKEN);
    harness.server.replace(
        Provider::Zai,
        Method::Get,
        PATH,
        Route::json(200, body).requires_header("authorization", &format!("Bearer {TOKEN}")),
    );
    Ok((
        harness,
        account("zai", Provider::Zai, env_source("ZAI_TEST_KEY")),
    ))
}

pub(crate) async fn case() -> Result<Case, Box<dyn std::error::Error>> {
    let (harness, account) = started(QUOTA).await?;
    Ok(Case {
        provider: Provider::Zai,
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

fn assert_provider_without(outcome: &UsageOutcome) {
    match unavailable(outcome) {
        UnavailableReason::Provider(message) => {
            assert!(!message.contains("SECRET_TEXT_ZAI"), "{message}");
        }
        other => panic!("expected a provider reason, found {other:?}"),
    }
}

#[tokio::test]
async fn a_token_limit_in_hours_and_one_in_weeks_give_the_five_hour_and_weekly_windows_port_fidelity()
-> TestResult {
    guard(async {
        assert_windows(&outcome(QUOTA).await?, &[FIVE, WEEK]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_credit_limit_in_weeks_gives_a_weekly_window_port_fidelity() -> TestResult {
    guard(async {
        let credit = want(
            WindowKind::Weekly,
            "weekly",
            Some(25.0),
            Left::Percent(75.0),
            Some(1_781_798_400),
        );
        assert_windows(&outcome(CREDIT).await?, &[credit]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_rejected_plan_in_a_200_body_is_a_provider_error_port_fidelity() -> TestResult {
    guard(async {
        assert_provider_without(&outcome(REJECTED).await?);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn success_false_with_code_200_is_a_provider_error_port_fidelity() -> TestResult {
    guard(async {
        assert_provider_without(&outcome(SUCCESS_FALSE).await?);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_code_500_in_a_200_body_is_a_provider_error_port_fidelity() -> TestResult {
    guard(async {
        assert_provider_without(&outcome(CODE_500).await?);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn no_limits_is_a_provider_error_port_fidelity() -> TestResult {
    guard(async {
        assert_provider_without(&outcome(NO_LIMITS).await?);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_negative_percentage_is_clamped_to_zero_port_fidelity() -> TestResult {
    guard(async {
        let clamped = want(
            WindowKind::FiveHour,
            "5-hour",
            Some(0.0),
            Left::Percent(100.0),
            Some(1_767_232_800),
        );
        assert_windows(&outcome(NEGATIVE).await?, &[clamped]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_limit_with_an_unknown_unit_is_dropped_port_fidelity() -> TestResult {
    guard(async {
        let kept = want(
            WindowKind::FiveHour,
            "5-hour",
            Some(40.0),
            Left::Percent(60.0),
            Some(1_767_232_800),
        );
        assert_windows(&outcome(UNKNOWN_UNIT).await?, &[kept]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_login_json_file_is_a_provider_error_port_fidelity() -> TestResult {
    guard(async {
        let (harness, _) = started(QUOTA).await?;
        harness
            .put("/keys/zai", "{\"api_key\":\"SECRET_TEXT_ZAI\"}")
            .await?;
        let login = account("zai", Provider::Zai, file_source("/keys/zai"));
        let usage = harness.one(&login, Refresh::Cached).await?;
        assert_provider_without(&usage.outcome);
        Ok(())
    })
    .await
}
