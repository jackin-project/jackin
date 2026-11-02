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

const TOKEN: &str = "SECRET_TOKEN_GROK";
const PATH: &str = "/v1/billing?format=credits";
const CREDITS: &str = include_str!("../fixtures/grok/billing_credits.json");
const ON_DEMAND_OFF: &str = include_str!("../fixtures/grok/billing_credits_on_demand_off.json");
const DEPRECATED: &str = include_str!("../fixtures/grok/billing_deprecated.json");
const INVALID: &str = include_str!("../fixtures/grok/billing_invalid_percent.json");
const NULL_CONFIG: &str = include_str!("../fixtures/grok/billing_null_config.json");
const AUTH: &str = include_str!("../fixtures/grok/auth.json");
const EXPIRED: &str = include_str!("../fixtures/grok/auth_expired.json");
const TWO_SCOPES: &str = include_str!("../fixtures/grok/auth_two_scopes.json");
const AUTH_PATH: &str = "/home/work/.grok/auth.json";

const WEEK: Want = want(
    WindowKind::Weekly,
    "weekly",
    Some(42.5),
    Left::Percent(57.5),
    Some(1_780_876_800),
);
const EXTRA: Want = want(
    WindowKind::Credits,
    "Extra usage credits",
    None,
    Left::Amount("12.50", "USD"),
    None,
);
const ON_DEMAND: Want = want(
    WindowKind::Spend,
    "On-demand",
    Some(6.0),
    Left::Amount("47.00", "USD"),
    None,
);

fn serve(harness: &UsageHarness, token: &str, body: &str) {
    harness.server.replace(
        Provider::Grok,
        Method::Get,
        PATH,
        Route::json(200, body)
            .requires_header("authorization", &format!("Bearer {token}"))
            .requires_header("x-xai-token-auth", "xai-grok-cli"),
    );
}

async fn started(body: &str) -> Result<(UsageHarness, UsageAccount), Box<dyn std::error::Error>> {
    let mut harness = UsageHarness::new().await?;
    harness.env("GROK_TEST_KEY", TOKEN);
    serve(&harness, TOKEN, body);
    Ok((
        harness,
        account("grok", Provider::Grok, env_source("GROK_TEST_KEY")),
    ))
}

pub(crate) async fn case() -> Result<Case, Box<dyn std::error::Error>> {
    let (harness, account) = started(CREDITS).await?;
    Ok(Case {
        provider: Provider::Grok,
        harness,
        account,
        first: (Method::Get, PATH),
        expected: &[WEEK, EXTRA, ON_DEMAND],
    })
}

async fn outcome(body: &str) -> Result<UsageOutcome, Box<dyn std::error::Error>> {
    let (harness, account) = started(body).await?;
    Ok(harness.one(&account, Refresh::Cached).await?.outcome)
}

fn assert_provider_without(outcome: &UsageOutcome) {
    match unavailable(outcome) {
        UnavailableReason::Provider(message) => {
            assert!(!message.contains("SECRET_TEXT_GROK"), "{message}");
        }
        other => panic!("expected a provider reason, found {other:?}"),
    }
}

async fn login_outcome(file: &str, key: &str) -> Result<UsageOutcome, Box<dyn std::error::Error>> {
    let (harness, _) = started(CREDITS).await?;
    serve(&harness, key, CREDITS);
    harness.put(AUTH_PATH, file).await?;
    let login = account("grok", Provider::Grok, file_source(AUTH_PATH));
    Ok(harness.one(&login, Refresh::Cached).await?.outcome)
}

#[tokio::test]
async fn the_credits_config_gives_a_weekly_window_extra_credits_and_on_demand() -> TestResult {
    guard(async {
        assert_windows(&outcome(CREDITS).await?, &[WEEK, EXTRA, ON_DEMAND]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_deprecated_config_gives_a_monthly_window_and_on_demand() -> TestResult {
    guard(async {
        let monthly = want(
            WindowKind::Monthly,
            "monthly",
            Some(61.7),
            Left::Amount("7.66", "USD"),
            Some(1_746_057_600),
        );
        let on_demand = want(
            WindowKind::Spend,
            "On-demand",
            Some(0.0),
            Left::Amount("5.00", "USD"),
            None,
        );
        assert_windows(&outcome(DEPRECATED).await?, &[monthly, on_demand]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn on_demand_disabled_leaves_no_on_demand_window() -> TestResult {
    guard(async {
        assert_windows(&outcome(ON_DEMAND_OFF).await?, &[WEEK, EXTRA]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_invalid_credit_percent_leaves_no_window_and_a_provider_error() -> TestResult {
    guard(async {
        assert_provider_without(&outcome(INVALID).await?);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_null_config_is_a_provider_error() -> TestResult {
    guard(async {
        assert_provider_without(&outcome(NULL_CONFIG).await?);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_empty_object_is_a_provider_error() -> TestResult {
    guard(async {
        assert_provider_without(&outcome("{}").await?);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_auth_file_gives_its_key_port_fidelity() -> TestResult {
    guard(async {
        assert_windows(
            &login_outcome(AUTH, TOKEN).await?,
            &[WEEK, EXTRA, ON_DEMAND],
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_auth_x_ai_scope_wins_over_a_sign_in_scope_port_fidelity() -> TestResult {
    guard(async {
        assert_windows(
            &login_outcome(TWO_SCOPES, TOKEN).await?,
            &[WEEK, EXTRA, ON_DEMAND],
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_entry_that_expires_now_is_an_expired_login_without_a_request() -> TestResult {
    guard(async {
        let outcome = login_outcome(EXPIRED, TOKEN).await?;
        assert_eq!(unavailable(&outcome), &UnavailableReason::ExpiredLogin);
        Ok(())
    })
    .await
}
