// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::Provider;
use jackin_testkit::{Method, Route};
use jackin_usage::{Refresh, UnavailableReason, UsageAccount, WindowKind};

use super::Case;
use crate::support::{
    Left, TestResult, UsageHarness, Want, account, assert_windows, env_source, file_source, guard,
    unavailable, want, windows_of,
};

const TOKEN: &str = "SECRET_TOKEN_CLAUDE";
const PATH: &str = "/api/oauth/usage";
const REASON: &str = "Anthropic API keys have no subscription quota";
const LIVE: &str = include_str!("../fixtures/claude/oauth_usage_live_mirror.json");
const LIMITS: &str = include_str!("../fixtures/claude/oauth_usage_limits.json");
const LEGACY: &str = include_str!("../fixtures/claude/oauth_usage_limits_and_legacy.json");
const SPEND: &str = include_str!("../fixtures/claude/oauth_usage_spend.json");
const FRACTIONAL: &str = include_str!("../fixtures/claude/oauth_usage_spend_fractional.json");
const BOTH: &str = include_str!("../fixtures/claude/oauth_usage_spend_and_extra.json");
const EXTRA: &str = include_str!("../fixtures/claude/oauth_usage_extra.json");
const ROUTINES: &str = include_str!("../fixtures/claude/oauth_usage_routines.json");
const CREDENTIALS: &str = include_str!("../fixtures/claude/credentials.json");
const NO_OAUTH: &str = include_str!("../fixtures/claude/credentials_no_oauth.json");

const FIVE: Want = want(
    WindowKind::FiveHour,
    "5-hour",
    Some(12.0),
    Left::Percent(88.0),
    Some(1_782_414_000),
);
const WEEK: Want = want(
    WindowKind::Weekly,
    "weekly",
    Some(34.0),
    Left::Percent(66.0),
    Some(1_782_482_400),
);
const SONNET: Want = want(
    WindowKind::Weekly,
    "Sonnet weekly",
    Some(5.0),
    Left::Percent(95.0),
    Some(1_782_482_400),
);
const SESSION: Want = want(
    WindowKind::FiveHour,
    "5-hour",
    Some(7.0),
    Left::Percent(93.0),
    Some(1_783_048_799),
);
const ALL: Want = want(
    WindowKind::Weekly,
    "weekly",
    Some(28.0),
    Left::Percent(72.0),
    Some(1_783_062_000),
);
const EXTRA_USAGE: Want = want(
    WindowKind::Spend,
    "Extra usage",
    Some(18.0),
    Left::Amount("246.69", "USD"),
    None,
);

fn serve(harness: &UsageHarness, token: &str, body: &str) {
    harness.server.replace(
        Provider::Claude,
        Method::Get,
        PATH,
        Route::json(200, body)
            .requires_header("authorization", &format!("Bearer {token}"))
            .requires_header("anthropic-beta", "oauth-2025-04-20"),
    );
}

async fn started(body: &str) -> Result<(UsageHarness, UsageAccount), Box<dyn std::error::Error>> {
    let mut harness = UsageHarness::new().await?;
    harness.env("CLAUDE_TEST_KEY", TOKEN);
    serve(&harness, TOKEN, body);
    let account = account("claude", Provider::Claude, env_source("CLAUDE_TEST_KEY"));
    Ok((harness, account))
}

pub(crate) async fn case() -> Result<Case, Box<dyn std::error::Error>> {
    let (harness, account) = started(LIVE).await?;
    Ok(Case {
        provider: Provider::Claude,
        token: TOKEN,
        harness,
        account,
        first: (Method::Get, PATH),
        expected: &[FIVE, WEEK, SONNET],
    })
}

async fn windows_for(body: &str) -> Result<jackin_usage::UsageOutcome, Box<dyn std::error::Error>> {
    let (harness, account) = started(body).await?;
    Ok(harness.one(&account, Refresh::Cached).await?.outcome)
}

#[tokio::test]
async fn the_live_mirror_body_gives_three_windows_and_ignores_the_placeholder_limit_port_fidelity()
-> TestResult {
    guard(async {
        assert_windows(&windows_for(LIVE).await?, &[FIVE, WEEK, SONNET]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn limits_give_the_session_the_weekly_and_a_model_window_port_fidelity() -> TestResult {
    guard(async {
        let fable = want(
            WindowKind::Weekly,
            "Fable weekly",
            Some(35.0),
            Left::Percent(65.0),
            Some(1_783_061_999),
        );
        assert_windows(&windows_for(LIMITS).await?, &[SESSION, ALL, fable]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn legacy_keys_already_covered_by_limits_are_dropped_port_fidelity() -> TestResult {
    guard(async {
        let opus = want(
            WindowKind::Weekly,
            "Opus weekly",
            Some(9.0),
            Left::Percent(91.0),
            Some(1_783_062_000),
        );
        assert_windows(&windows_for(LEGACY).await?, &[SESSION, ALL, opus]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_spend_object_gives_the_extra_usage_window_port_fidelity() -> TestResult {
    guard(async {
        assert_windows(&windows_for(SPEND).await?, &[EXTRA_USAGE]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_fractional_spend_percent_is_kept_port_fidelity() -> TestResult {
    guard(async {
        let fractional = want(
            WindowKind::Spend,
            "Extra usage",
            Some(37.125),
            Left::Amount("87.66", "USD"),
            None,
        );
        assert_windows(&windows_for(FRACTIONAL).await?, &[fractional]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn spend_wins_over_extra_usage_port_fidelity() -> TestResult {
    guard(async {
        assert_windows(&windows_for(BOTH).await?, &[EXTRA_USAGE]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn extra_usage_alone_gives_the_extra_usage_window_in_its_currency_port_fidelity() -> TestResult
{
    guard(async {
        let extra = want(
            WindowKind::Spend,
            "Extra usage",
            Some(30.0),
            Left::Amount("181.51", "SGD"),
            None,
        );
        assert_windows(&windows_for(EXTRA).await?, &[extra]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_routines_window_is_called_routines_weekly_port_fidelity() -> TestResult {
    guard(async {
        let routines = want(
            WindowKind::Weekly,
            "Routines weekly",
            Some(15.0),
            Left::Percent(85.0),
            Some(1_782_482_400),
        );
        assert_windows(&windows_for(ROUTINES).await?, &[routines]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_api_key_is_unsupported_and_the_route_is_not_reached() -> TestResult {
    guard(async {
        let mut harness = UsageHarness::new().await?;
        harness.env("CLAUDE_TEST_KEY", "sk-ant-api03-SECRET_TOKEN_CLAUDE");
        harness
            .server
            .serve(Provider::Claude, Method::Get, PATH, Route::json(200, LIVE));
        let account = account("claude", Provider::Claude, env_source("CLAUDE_TEST_KEY"));
        let usage = harness.one(&account, Refresh::Cached).await?;
        assert_eq!(
            unavailable(&usage.outcome),
            &UnavailableReason::Unsupported {
                detail: Some(REASON.to_owned())
            }
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_credentials_file_gives_its_access_token_port_fidelity() -> TestResult {
    guard(async {
        let (harness, _) = started(LIVE).await?;
        harness
            .put("/home/work/.claude/.credentials.json", CREDENTIALS)
            .await?;
        let login = account(
            "claude",
            Provider::Claude,
            file_source("/home/work/.claude/.credentials.json"),
        );
        let usage = harness.one(&login, Refresh::Cached).await?;
        assert_eq!(windows_of(&usage.outcome).len(), 3);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_credentials_file_without_the_oauth_entry_is_a_provider_error_port_fidelity() -> TestResult
{
    guard(async {
        let (harness, _) = started(LIVE).await?;
        harness
            .put("/home/work/.claude/.credentials.json", NO_OAUTH)
            .await?;
        let login = account(
            "claude",
            Provider::Claude,
            file_source("/home/work/.claude/.credentials.json"),
        );
        let usage = harness.one(&login, Refresh::Cached).await?;
        match unavailable(&usage.outcome) {
            UnavailableReason::Provider(message) => {
                assert!(!message.contains("SECRET_TEXT_CLAUDE"));
            }
            other => panic!("expected a provider reason, found {other:?}"),
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_403_is_an_expired_login() -> TestResult {
    guard(async {
        let (harness, account) = started(LIVE).await?;
        harness
            .server
            .replace(Provider::Claude, Method::Get, PATH, Route::status(403));
        let usage = harness.one(&account, Refresh::Cached).await?;
        assert_eq!(
            unavailable(&usage.outcome),
            &UnavailableReason::ExpiredLogin
        );
        Ok(())
    })
    .await
}

fn used_first(usage: &jackin_usage::AccountUsage) -> f64 {
    windows_of(&usage.outcome)[0]
        .used
        .map_or(-1.0, jackin_usage::Percent::get)
}

#[tokio::test]
async fn the_cache_serves_for_five_minutes() -> TestResult {
    guard(async {
        let (harness, account) = started(LIVE).await?;
        harness.one(&account, Refresh::Cached).await?;
        serve(&harness, TOKEN, LIMITS);
        harness.advance(299);
        let usage = harness.one(&account, Refresh::Cached).await?;
        assert!((used_first(&usage) - 12.0).abs() < 1e-9);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_cache_fetches_again_after_five_minutes() -> TestResult {
    guard(async {
        let (harness, account) = started(LIVE).await?;
        harness.one(&account, Refresh::Cached).await?;
        serve(&harness, TOKEN, LIMITS);
        harness.advance(301);
        let usage = harness.one(&account, Refresh::Cached).await?;
        assert!((used_first(&usage) - 7.0).abs() < 1e-9);
        Ok(())
    })
    .await
}

fn assert_provider(outcome: &jackin_usage::UsageOutcome) {
    assert!(
        matches!(unavailable(outcome), UnavailableReason::Provider(_)),
        "{outcome:?}"
    );
}

#[tokio::test]
async fn a_spend_amount_at_the_edge_of_the_integers_is_a_provider_error_port_fidelity() -> TestResult
{
    guard(async {
        let body = "{\"spend\":{\"used\":{\"amount_minor\":-9223372036854775808,\"currency\":\"USD\",\"exponent\":2},\"limit\":{\"amount_minor\":9223372036854775807,\"currency\":\"USD\",\"exponent\":2},\"percent\":18,\"enabled\":true}}";
        assert_provider(&windows_for(body).await?);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_extra_usage_exponent_that_overflows_a_power_of_ten_is_a_provider_error_port_fidelity()
-> TestResult {
    guard(async {
        let body = "{\"extra_usage\":{\"is_enabled\":true,\"monthly_limit\":26000.0,\"used_credits\":7849.0,\"utilization\":30.0,\"currency\":\"SGD\",\"decimal_places\":40}}";
        assert_provider(&windows_for(body).await?);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_extra_usage_amount_beyond_the_integers_is_a_provider_error_port_fidelity() -> TestResult
{
    guard(async {
        let body = "{\"extra_usage\":{\"is_enabled\":true,\"monthly_limit\":1e300,\"used_credits\":7849.0,\"utilization\":30.0,\"currency\":\"SGD\",\"decimal_places\":2}}";
        assert_provider(&windows_for(body).await?);
        Ok(())
    })
    .await
}
