// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use chrono::{DateTime, Duration, Utc};
use jackin_core::{Provider, Timestamp};
use jackin_testkit::{Method, Route};
use jackin_usage::{AccountUsage, Freshness, Refresh, UnavailableReason};
use jackin_usage_provider_core::retry_after_text;

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
async fn a_retry_after_given_as_a_past_http_date_gives_no_retry_time() -> TestResult {
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

#[tokio::test]
async fn a_retry_after_given_as_a_future_http_date_gives_the_retry_time() -> TestResult {
    guard(async {
        let date = http_date(Utc::now() + Duration::seconds(600));
        let (_harness, _account, first) = limited(&date).await?;
        match unavailable(&first.outcome) {
            UnavailableReason::RateLimited { retry_at: Some(at) } => {
                assert!(
                    (at.0 - (T0 + 600)).abs() <= 2,
                    "the retry time {at:?} is not ten minutes away"
                );
            }
            other => panic!("expected a rate limited reason, found {other:?}"),
        }
        Ok(())
    })
    .await
}

fn http_date(at: DateTime<Utc>) -> String {
    at.format("%a, %d %b %Y %H:%M:%S GMT").to_string()
}

fn instant(text: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc2822(text)
        .expect("an HTTP date")
        .with_timezone(&Utc)
}

#[test]
fn an_http_date_inside_the_current_second_is_already_past() {
    let now = instant("Thu, 01 Jan 2026 00:00:00 GMT") + Duration::milliseconds(500);
    assert_eq!(retry_after_text("Thu, 01 Jan 2026 00:00:00 GMT", now), None);
}

#[test]
fn an_http_date_at_the_current_instant_gives_no_wait() {
    let now = instant("Thu, 01 Jan 2026 00:00:00 GMT");
    assert_eq!(
        retry_after_text("Thu, 01 Jan 2026 00:00:00 GMT", now),
        Some(0)
    );
}

#[test]
fn an_http_date_with_an_offset_is_the_same_instant() {
    let now = instant("Thu, 01 Jan 2026 00:00:00 +0000") + Duration::milliseconds(500);
    assert_eq!(retry_after_text("Thu, 01 Jan 2026 00:00:00 GMT", now), None);
    assert_eq!(
        retry_after_text("Thu, 01 Jan 2026 00:00:00 +0000", now),
        None
    );
    let exact = instant("Thu, 01 Jan 2026 00:00:00 +0000");
    assert_eq!(
        retry_after_text("Thu, 01 Jan 2026 00:10:00 +0000", exact),
        Some(600)
    );
}

#[test]
fn a_numeric_retry_after_of_zero_gives_no_wait() {
    let now = instant("Thu, 01 Jan 2026 00:00:00 GMT") + Duration::milliseconds(500);
    assert_eq!(retry_after_text("0", now), Some(0));
}
