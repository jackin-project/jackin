// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::Provider;
use jackin_testkit::{Method, Route};
use jackin_usage::{Refresh, UnavailableReason};

use crate::support::{
    CODEX_PATH, TestResult, UsageHarness, codex_route, guard, unavailable, windows_of,
};

const FULL: &str = include_str!("fixtures/codex/usage_full.json");

async fn answering(status: u16) -> Result<UnavailableReason, Box<dyn std::error::Error>> {
    let (harness, account) = UsageHarness::with_codex(FULL).await?;
    harness.server.replace(
        Provider::Codex,
        Method::Get,
        CODEX_PATH,
        Route::status(status),
    );
    let usage = harness.one(&account, Refresh::Cached).await?;
    Ok(unavailable(&usage.outcome).clone())
}

#[tokio::test]
async fn http_401_is_an_expired_login() -> TestResult {
    guard(async {
        assert_eq!(answering(401).await?, UnavailableReason::ExpiredLogin);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn http_403_is_an_expired_login() -> TestResult {
    guard(async {
        assert_eq!(answering(403).await?, UnavailableReason::ExpiredLogin);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn http_404_is_a_provider_error() -> TestResult {
    guard(async {
        assert!(matches!(
            answering(404).await?,
            UnavailableReason::Provider(_)
        ));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn http_500_is_a_provider_error() -> TestResult {
    guard(async {
        assert!(matches!(
            answering(500).await?,
            UnavailableReason::Provider(_)
        ));
        Ok(())
    })
    .await
}

const SENTINEL: &str = "SECRET_TEXT_BODY";

async fn broken(body: &str) -> Result<UnavailableReason, Box<dyn std::error::Error>> {
    let (harness, account) = UsageHarness::with_codex(FULL).await?;
    harness
        .server
        .replace(Provider::Codex, Method::Get, CODEX_PATH, codex_route(body));
    let usage = harness.one(&account, Refresh::Cached).await?;
    Ok(unavailable(&usage.outcome).clone())
}

fn assert_provider_without_the_body(reason: &UnavailableReason) {
    match reason {
        UnavailableReason::Provider(message) => {
            assert!(!message.contains(SENTINEL), "{message}");
        }
        other => panic!("expected a provider reason, found {other:?}"),
    }
}

#[tokio::test]
async fn a_body_that_is_not_json_is_a_provider_error() -> TestResult {
    guard(async {
        assert_provider_without_the_body(&broken(&format!("not json {SENTINEL}")).await?);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_empty_object_is_a_provider_error() -> TestResult {
    guard(async {
        assert_provider_without_the_body(&broken(&format!("{{\"note\":\"{SENTINEL}\"}}")).await?);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_window_with_a_wrong_typed_percent_leaves_no_window_and_is_a_provider_error() -> TestResult
{
    guard(async {
        let body = format!(
            "{{\"note\":\"{SENTINEL}\",\"rate_limit\":{{\"primary_window\":{{\"used_percent\":\"high\"}}}}}}"
        );
        assert_provider_without_the_body(&broken(&body).await?);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_empty_rate_limit_is_a_provider_error() -> TestResult {
    guard(async {
        let body = format!("{{\"note\":\"{SENTINEL}\",\"rate_limit\":{{}}}}");
        assert_provider_without_the_body(&broken(&body).await?);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_garbage_secondary_window_is_dropped_and_the_primary_stays() -> TestResult {
    guard(async {
        let (harness, account) =
            UsageHarness::with_codex(include_str!("fixtures/codex/usage_garbage_window.json"))
                .await?;
        let usage = harness.one(&account, Refresh::Cached).await?;
        assert_eq!(windows_of(&usage.outcome).len(), 1);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_reset_after_that_overflows_is_a_provider_error() -> TestResult {
    guard(async {
        let body = "{\"rate_limit\":{\"primary_window\":{\"used_percent\":42,\"limit_window_seconds\":18000,\"reset_after_seconds\":9223372036854775807}}}";
        assert!(matches!(broken(body).await?, UnavailableReason::Provider(_)));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_reset_time_at_the_edge_of_the_integers_is_a_provider_error() -> TestResult {
    guard(async {
        let body = "{\"rate_limit\":{\"primary_window\":{\"used_percent\":42,\"limit_window_seconds\":18000,\"reset_at\":-9223372036854775808}}}";
        assert!(matches!(broken(body).await?, UnavailableReason::Provider(_)));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_individual_limit_beyond_the_integers_is_a_provider_error() -> TestResult {
    guard(async {
        let body = "{\"rate_limit\":{\"primary_window\":{\"used_percent\":42,\"limit_window_seconds\":18000,\"reset_at\":1767232800}},\"spend_control\":{\"individual_limit\":{\"limit\":\"1e300\",\"used\":\"1.00\"}}}";
        assert!(matches!(
            broken(body).await?,
            UnavailableReason::Provider(_)
        ));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_body_over_one_mebibyte_is_a_provider_error_and_one_at_the_limit_decodes() -> TestResult {
    guard(async {
        let limit = 1 << 20;
        let at_limit = format!("{FULL}{}", " ".repeat(limit - FULL.len()));
        let (harness, account) = UsageHarness::with_codex(&at_limit).await?;
        let usage = harness.one(&account, Refresh::Cached).await?;
        assert!(!windows_of(&usage.outcome).is_empty());

        let over = format!("{at_limit} ");
        assert_provider_without_the_body(&broken(&over).await?);
        Ok(())
    })
    .await
}
