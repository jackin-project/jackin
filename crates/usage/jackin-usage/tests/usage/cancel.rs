// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_core::Provider;
use jackin_testkit::{Method, Route};
use jackin_usage::{Refresh, UnavailableReason, UsageError, UsageRequest};
use tokio_util::sync::CancellationToken;

use crate::support::{
    CODEX_PATH, TestResult, UsageHarness, codex_body, codex_login, codex_route, guard, serve_codex,
    unavailable, windows_of,
};

#[tokio::test]
async fn cancelling_a_request_in_flight_returns_cancelled_and_writes_no_cache() -> TestResult {
    guard(async {
        let (harness, account) = UsageHarness::with_codex(&codex_body(10)).await?;
        let held = harness
            .server
            .hold(Provider::Codex, Method::Get, CODEX_PATH);
        let request = UsageRequest {
            accounts: vec![account],
            refresh: Refresh::Force,
        };
        let cancel = CancellationToken::new();
        let host = harness.host();
        let canceller = async {
            held.arrived().await;
            cancel.cancel();
        };
        let (result, ()) = tokio::join!(host.report(&request, &cancel), canceller);
        assert!(matches!(result, Err(UsageError::Cancelled)), "{result:?}");
        assert_eq!(harness.cache_text(), None);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cancelled_forced_report_leaves_the_cache_bytes_unchanged() -> TestResult {
    guard(async {
        let (harness, account) = UsageHarness::with_codex(&codex_body(10)).await?;
        harness.one(&account, Refresh::Force).await?;
        let before = harness.cache_text();
        assert!(before.is_some());
        harness.advance(600);
        harness.server.replace(
            Provider::Codex,
            Method::Get,
            CODEX_PATH,
            codex_route(&codex_body(90)),
        );
        let held = harness
            .server
            .hold(Provider::Codex, Method::Get, CODEX_PATH);
        let request = UsageRequest {
            accounts: vec![account],
            refresh: Refresh::Force,
        };
        let cancel = CancellationToken::new();
        let host = harness.host();
        let canceller = async {
            held.arrived().await;
            cancel.cancel();
        };
        let (result, ()) = tokio::join!(host.report(&request, &cancel), canceller);
        assert!(matches!(result, Err(UsageError::Cancelled)), "{result:?}");
        assert_eq!(harness.cache_text(), before);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_provider_that_never_answers_is_a_network_error_after_the_timeout() -> TestResult {
    guard(async {
        let (harness, slow) = UsageHarness::with_codex(&codex_body(10)).await?;
        harness
            .put(
                "/home/fast/.codex/auth.json",
                include_str!("fixtures/codex/auth_chatgpt.json"),
            )
            .await?;
        serve_codex(
            &harness,
            "SECRET_TOKEN_CODEX",
            Route::json(200, &codex_body(10))
                .requires_header("chatgpt-account-id", "slow-id")
                .delayed(Duration::from_secs(8)),
        );
        let slow = codex_login(slow.name.as_str(), "/home/work/.codex", "slow-id");
        let fast = codex_login("fast", "/home/fast/.codex", "acct-1");
        let host = harness.host_with(
            harness
                .server
                .endpoints()
                .with_timeout(Duration::from_millis(300)),
        );
        let report = harness
            .report_on(&host, &[slow, fast], Refresh::Cached)
            .await?;
        assert!(matches!(
            unavailable(&report.accounts[0].outcome),
            UnavailableReason::Network(_)
        ));
        assert_eq!(windows_of(&report.accounts[1].outcome).len(), 1);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_answer_inside_the_timeout_is_a_normal_report() -> TestResult {
    guard(async {
        let (harness, account) = UsageHarness::with_codex(&codex_body(10)).await?;
        harness.server.replace(
            Provider::Codex,
            Method::Get,
            CODEX_PATH,
            codex_route(&codex_body(10)).delayed(Duration::from_millis(25)),
        );
        let host = harness.host_with(
            harness
                .server
                .endpoints()
                .with_timeout(Duration::from_millis(300)),
        );
        let report = harness
            .report_on(&host, &[account], Refresh::Cached)
            .await?;
        assert_eq!(windows_of(&report.accounts[0].outcome).len(), 1);
        Ok(())
    })
    .await
}
