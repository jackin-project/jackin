// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Provider, Timestamp};
use jackin_testkit::{Method, Route};
use jackin_usage::{AccountUsage, Freshness, Refresh, UnavailableReason};

use crate::support::{
    CODEX_PATH, T0, TestResult, UsageHarness, codex_body, codex_route, guard, unavailable,
    windows_of,
};

async fn limited(
    retry_after: &str,
) -> Result<(UsageHarness, jackin_usage::UsageAccount, AccountUsage), Box<dyn std::error::Error>> {
    let (harness, account) = UsageHarness::with_codex(&codex_body(10)).await?;
    harness.server.replace(
        Provider::Codex,
        Method::Get,
        CODEX_PATH,
        Route::status(429).reply_header("Retry-After", retry_after),
    );
    let first = harness.one(&account, Refresh::Cached).await?;
    harness.server.replace(
        Provider::Codex,
        Method::Get,
        CODEX_PATH,
        codex_route(&codex_body(10)),
    );
    Ok((harness, account, first))
}

fn limited_until(at: i64) -> UnavailableReason {
    UnavailableReason::RateLimited {
        retry_at: Some(Timestamp(at)),
    }
}

#[tokio::test]
async fn a_429_with_seconds_gives_the_retry_time() -> TestResult {
    guard(async {
        let (_harness, _account, first) = limited("600").await?;
        assert_eq!(unavailable(&first.outcome), &limited_until(T0 + 600));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cached_report_after_the_ttl_still_honors_the_retry_time() -> TestResult {
    guard(async {
        let (harness, account, _) = limited("600").await?;
        harness.advance(300);
        let again = harness.one(&account, Refresh::Cached).await?;
        assert_eq!(unavailable(&again.outcome), &limited_until(T0 + 600));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_forced_report_asks_the_provider_again_during_the_retry_time() -> TestResult {
    guard(async {
        let (harness, account, _) = limited("600").await?;
        harness.advance(300);
        let again = harness.one(&account, Refresh::Force).await?;
        assert_eq!(windows_of(&again.outcome).len(), 1);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_forced_report_that_is_limited_again_holds_the_new_retry_time() -> TestResult {
    guard(async {
        let (harness, account, _) = limited("600").await?;
        harness.advance(300);
        harness.server.replace(
            Provider::Codex,
            Method::Get,
            CODEX_PATH,
            Route::status(429).reply_header("Retry-After", "900"),
        );
        let forced = harness.one(&account, Refresh::Force).await?;
        assert_eq!(unavailable(&forced.outcome), &limited_until(T0 + 300 + 900));
        harness.server.replace(
            Provider::Codex,
            Method::Get,
            CODEX_PATH,
            codex_route(&codex_body(10)),
        );
        harness.advance(600);
        let cached = harness.one(&account, Refresh::Cached).await?;
        assert_eq!(unavailable(&cached.outcome), &limited_until(T0 + 300 + 900));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_forced_report_that_fails_for_another_reason_keeps_the_retry_time() -> TestResult {
    guard(async {
        let (harness, account) = UsageHarness::with_codex(&codex_body(10)).await?;
        harness.one(&account, Refresh::Cached).await?;
        harness.advance(200);
        harness.server.replace(
            Provider::Codex,
            Method::Get,
            CODEX_PATH,
            Route::status(429).reply_header("Retry-After", "600"),
        );
        let limited = harness.one(&account, Refresh::Cached).await?;
        assert_eq!(
            limited.freshness,
            Freshness::Stale {
                since: Timestamp(T0)
            }
        );
        harness.advance(300);
        harness
            .server
            .replace(Provider::Codex, Method::Get, CODEX_PATH, Route::status(503));
        let forced = harness.one(&account, Refresh::Force).await?;
        assert_eq!(
            forced.freshness,
            Freshness::Stale {
                since: Timestamp(T0)
            }
        );
        harness.server.replace(
            Provider::Codex,
            Method::Get,
            CODEX_PATH,
            codex_route(&codex_body(10)),
        );
        harness.advance(100);
        let cached = harness.one(&account, Refresh::Cached).await?;
        assert_eq!(
            cached.freshness,
            Freshness::Stale {
                since: Timestamp(T0)
            }
        );
        harness.advance(201);
        let after = harness.one(&account, Refresh::Cached).await?;
        assert_eq!(after.freshness, Freshness::Fresh);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn after_the_retry_time_the_provider_is_asked_again() -> TestResult {
    guard(async {
        let (harness, account, _) = limited("600").await?;
        harness.advance(601);
        let again = harness.one(&account, Refresh::Cached).await?;
        assert_eq!(windows_of(&again.outcome).len(), 1);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_retry_after_that_is_not_a_number_gives_no_retry_time() -> TestResult {
    guard(async {
        let (_harness, _account, first) = limited("soon").await?;
        assert_eq!(
            unavailable(&first.outcome),
            &UnavailableReason::RateLimited { retry_at: None }
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_retry_after_given_as_an_http_date_gives_no_retry_time() -> TestResult {
    guard(async {
        let (_harness, _account, first) = limited("Thu, 01 Jan 2026 00:10:00 GMT").await?;
        assert_eq!(
            unavailable(&first.outcome),
            &UnavailableReason::RateLimited { retry_at: None }
        );
        Ok(())
    })
    .await
}
