// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::Provider;
use jackin_testkit::{Method, Route};
use jackin_usage::{Refresh, UnavailableReason, UsageAccount, UsageOutcome, WindowKind};

use super::Case;
use crate::support::{
    Left, TestResult, UsageHarness, Want, account, assert_windows, env_source, guard, unavailable,
    want,
};

const TOKEN: &str = "SECRET_TOKEN_MINIMAX";
const PATH: &str = "/v1/token_plan/remains";
const REMAINS: &str = include_str!("../fixtures/minimax/token_plan_remains.json");
const NO_END: &str = include_str!("../fixtures/minimax/token_plan_no_end.json");
const STATES: &str = include_str!("../fixtures/minimax/token_plan_states.json");
const ERROR: &str = include_str!("../fixtures/minimax/token_plan_error.json");
const OVER: &str = include_str!("../fixtures/minimax/token_plan_over.json");

const FIVE: Want = want(
    WindowKind::FiveHour,
    "5-hour",
    Some(42.0),
    Left::Percent(58.0),
    Some(1_767_243_600),
);
const WEEK: Want = want(
    WindowKind::Weekly,
    "weekly",
    Some(12.0),
    Left::Percent(88.0),
    Some(1_767_830_400),
);
const IMAGE: Want = want(
    WindowKind::Other,
    "image-01",
    Some(0.0),
    Left::Percent(100.0),
    Some(1_767_312_000),
);

async fn started(body: &str) -> Result<(UsageHarness, UsageAccount), Box<dyn std::error::Error>> {
    let mut harness = UsageHarness::new().await?;
    harness.env("MINIMAX_TEST_KEY", TOKEN);
    harness.server.replace(
        Provider::Minimax,
        Method::Get,
        PATH,
        Route::json(200, body).requires_header("authorization", &format!("Bearer {TOKEN}")),
    );
    Ok((
        harness,
        account("minimax", Provider::Minimax, env_source("MINIMAX_TEST_KEY")),
    ))
}

pub(crate) async fn case() -> Result<Case, Box<dyn std::error::Error>> {
    let (harness, account) = started(REMAINS).await?;
    Ok(Case {
        provider: Provider::Minimax,
        harness,
        account,
        first: (Method::Get, PATH),
        expected: &[FIVE, WEEK, IMAGE],
    })
}

async fn outcome(body: &str) -> Result<UsageOutcome, Box<dyn std::error::Error>> {
    let (harness, account) = started(body).await?;
    Ok(harness.one(&account, Refresh::Cached).await?.outcome)
}

fn assert_provider_without(outcome: &UsageOutcome) {
    match unavailable(outcome) {
        UnavailableReason::Provider(message) => {
            assert!(!message.contains("SECRET_TEXT_MINIMAX"), "{message}");
        }
        other => panic!("expected a provider reason, found {other:?}"),
    }
}

#[tokio::test]
async fn the_general_model_has_an_interval_and_a_weekly_window_and_others_an_interval_window_port_fidelity()
-> TestResult {
    guard(async {
        assert_windows(&outcome(REMAINS).await?, &[FIVE, WEEK, IMAGE]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn without_an_end_time_the_remaining_time_gives_the_reset_port_fidelity() -> TestResult {
    guard(async {
        assert_windows(
            &outcome(NO_END).await?,
            &[want(
                WindowKind::FiveHour,
                "5-hour",
                Some(42.0),
                Left::Percent(58.0),
                Some(1_767_243_600),
            )],
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_exhausted_interval_status_gives_a_full_window_port_fidelity() -> TestResult {
    guard(async {
        let full = want(
            WindowKind::FiveHour,
            "5-hour",
            Some(100.0),
            Left::Percent(0.0),
            None,
        );
        assert_windows(&outcome(STATES).await?, &[full]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_base_resp_error_in_a_200_body_is_a_provider_error_port_fidelity() -> TestResult {
    guard(async {
        assert_provider_without(&outcome(ERROR).await?);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_empty_model_list_is_a_provider_error_port_fidelity() -> TestResult {
    guard(async {
        assert_provider_without(&outcome("{\"model_remains\":[]}").await?);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_remaining_percent_above_one_hundred_is_clamped_port_fidelity() -> TestResult {
    guard(async {
        let clamped = want(
            WindowKind::FiveHour,
            "5-hour",
            Some(0.0),
            Left::Percent(100.0),
            Some(1_767_243_600),
        );
        assert_windows(&outcome(OVER).await?, &[clamped]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_pay_as_you_go_key_is_unsupported_and_the_route_is_not_reached() -> TestResult {
    guard(async {
        let mut harness = UsageHarness::new().await?;
        harness.env("MINIMAX_TEST_KEY", "sk-api-SECRET_TOKEN_MINIMAX");
        harness.server.serve(
            Provider::Minimax,
            Method::Get,
            PATH,
            Route::json(200, REMAINS),
        );
        let account = account("minimax", Provider::Minimax, env_source("MINIMAX_TEST_KEY"));
        let usage = harness.one(&account, Refresh::Cached).await?;
        assert_eq!(
            unavailable(&usage.outcome),
            &UnavailableReason::Unsupported {
                detail: Some("MiniMax pay-as-you-go keys have no plan quota".to_owned())
            }
        );
        Ok(())
    })
    .await
}
