// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::io::ErrorKind;

use jackin_core::{FileMode, FileSystem, FsError, Provider, Timestamp};
use jackin_testkit::{FsOp, Method, Route};
use jackin_usage::{
    AccountUsage, Freshness, Percent, Refresh, UnavailableReason, UsageAccount, UsageError,
};

use crate::support::{
    CODEX_PATH, T0, TestResult, UsageHarness, codex_body, codex_login, codex_route, guard,
    host_path, unavailable, windows_of,
};

async fn start(used: u32) -> Result<(UsageHarness, UsageAccount), Box<dyn std::error::Error>> {
    UsageHarness::with_codex(&codex_body(used)).await
}

fn serve(harness: &UsageHarness, route: Route) {
    harness
        .server
        .replace(Provider::Codex, Method::Get, CODEX_PATH, route);
}

fn used_of(usage: &AccountUsage) -> f64 {
    windows_of(&usage.outcome)[0]
        .used
        .map_or(-1.0, Percent::get)
}

#[tokio::test]
async fn a_cached_report_inside_the_ttl_shows_the_first_value() -> TestResult {
    guard(async {
        let (harness, account) = start(10).await?;
        let first = harness.one(&account, Refresh::Cached).await?;
        assert_eq!((first.freshness, used_of(&first)), (Freshness::Fresh, 10.0));
        serve(&harness, codex_route(&codex_body(90)));
        harness.advance(119);
        let again = harness.one(&account, Refresh::Cached).await?;
        assert_eq!((again.freshness, used_of(&again)), (Freshness::Fresh, 10.0));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cached_report_after_the_ttl_fetches_again() -> TestResult {
    guard(async {
        let (harness, account) = start(10).await?;
        harness.one(&account, Refresh::Cached).await?;
        serve(&harness, codex_route(&codex_body(90)));
        harness.advance(121);
        let again = harness.one(&account, Refresh::Cached).await?;
        assert_eq!((again.freshness, used_of(&again)), (Freshness::Fresh, 90.0));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_forced_report_shows_the_current_route() -> TestResult {
    guard(async {
        let (harness, account) = start(10).await?;
        harness.one(&account, Refresh::Cached).await?;
        serve(&harness, codex_route(&codex_body(90)));
        harness.advance(1);
        let forced = harness.one(&account, Refresh::Force).await?;
        assert_eq!(used_of(&forced), 90.0);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_failure_is_never_served_from_the_cache() -> TestResult {
    guard(async {
        let (harness, account) = start(10).await?;
        serve(&harness, Route::status(401));
        let failed = harness.one(&account, Refresh::Cached).await?;
        assert_eq!(
            unavailable(&failed.outcome),
            &UnavailableReason::ExpiredLogin
        );
        serve(&harness, codex_route(&codex_body(10)));
        harness.advance(10);
        let after = harness.one(&account, Refresh::Cached).await?;
        assert_eq!(used_of(&after), 10.0);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_new_instance_on_the_same_state_directory_serves_the_first_value() -> TestResult {
    guard(async {
        let (harness, account) = start(10).await?;
        let first = harness.host();
        harness
            .report_on(&first, std::slice::from_ref(&account), Refresh::Cached)
            .await?;
        serve(&harness, codex_route(&codex_body(90)));
        harness.advance(30);
        let second = harness.host();
        let report = harness
            .report_on(&second, std::slice::from_ref(&account), Refresh::Cached)
            .await?;
        assert_eq!(used_of(&report.accounts[0]), 10.0);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_cache_file_is_private() -> TestResult {
    guard(async {
        let (harness, account) = start(10).await?;
        harness.one(&account, Refresh::Cached).await?;
        let mode = harness
            .fs
            .mode(&host_path("/state/usage/usage.json"))
            .await?;
        assert_eq!(mode, Some(FileMode(0o600)));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn another_state_directory_shows_the_current_value() -> TestResult {
    guard(async {
        let (harness, account) = start(10).await?;
        harness.one(&account, Refresh::Cached).await?;
        serve(&harness, codex_route(&codex_body(90)));
        harness.advance(30);
        let elsewhere = harness.host_in(&host_path("/other/usage"));
        let report = harness
            .report_on(&elsewhere, std::slice::from_ref(&account), Refresh::Cached)
            .await?;
        assert_eq!(used_of(&report.accounts[0]), 90.0);
        Ok(())
    })
    .await
}

async fn stale_after(
    failure: Route,
) -> Result<(UsageHarness, UsageAccount, AccountUsage), Box<dyn std::error::Error>> {
    let (harness, account) = start(10).await?;
    harness.one(&account, Refresh::Cached).await?;
    serve(&harness, failure);
    harness.advance(600);
    let usage = harness.one(&account, Refresh::Cached).await?;
    Ok((harness, account, usage))
}

fn assert_stale(usage: &AccountUsage) {
    assert_eq!(
        usage.freshness,
        Freshness::Stale {
            since: Timestamp(T0)
        }
    );
    assert_eq!(used_of(usage), 10.0);
}

#[tokio::test]
async fn an_http_500_after_a_good_report_returns_the_old_windows_as_stale() -> TestResult {
    guard(async {
        let (harness, _, usage) = stale_after(Route::status(500)).await?;
        assert_stale(&usage);
        let files = [(
            "/home/work/.codex/auth.json",
            include_str!("fixtures/codex/auth_chatgpt.json"),
        )];
        crate::support::assert_only_state_written(&harness, &files).await
    })
    .await
}

#[tokio::test]
async fn a_refused_connection_after_a_good_report_returns_the_old_windows_as_stale() -> TestResult {
    guard(async {
        let (harness, account) = start(10).await?;
        harness.one(&account, Refresh::Cached).await?;
        harness.server.shutdown().await;
        harness.advance(600);
        let usage = harness.one(&account, Refresh::Cached).await?;
        assert_stale(&usage);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_malformed_body_after_a_good_report_replaces_the_old_windows() -> TestResult {
    guard(async {
        let (harness, account, usage) = stale_after(Route::json(200, "not json")).await?;
        assert!(matches!(
            unavailable(&usage.outcome),
            UnavailableReason::Provider(_)
        ));
        serve(&harness, codex_route(&codex_body(90)));
        harness.advance(5);
        let again = harness.one(&account, Refresh::Cached).await?;
        assert_eq!((again.freshness, used_of(&again)), (Freshness::Fresh, 90.0));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_forced_401_replaces_the_old_windows_and_a_later_cached_report_fetches() -> TestResult {
    guard(async {
        let (harness, account) = start(10).await?;
        harness.one(&account, Refresh::Cached).await?;
        harness.advance(10);
        serve(&harness, Route::status(401));
        let forced = harness.one(&account, Refresh::Force).await?;
        assert_eq!(
            unavailable(&forced.outcome),
            &UnavailableReason::ExpiredLogin
        );
        serve(&harness, codex_route(&codex_body(90)));
        harness.advance(5);
        let later = harness.one(&account, Refresh::Cached).await?;
        assert_eq!((later.freshness, used_of(&later)), (Freshness::Fresh, 90.0));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_forced_report_without_a_credential_replaces_the_old_windows() -> TestResult {
    guard(async {
        let (harness, account) = start(10).await?;
        harness.one(&account, Refresh::Cached).await?;
        let auth = host_path("/home/work/.codex/auth.json");
        harness.fs.remove_file(&auth).await?;
        harness.advance(10);
        let forced = harness.one(&account, Refresh::Force).await?;
        assert_eq!(
            unavailable(&forced.outcome),
            &UnavailableReason::NoCredentials
        );
        harness
            .put(
                "/home/work/.codex/auth.json",
                include_str!("fixtures/codex/auth_chatgpt.json"),
            )
            .await?;
        serve(&harness, codex_route(&codex_body(90)));
        harness.advance(5);
        let later = harness.one(&account, Refresh::Cached).await?;
        assert_eq!(used_of(&later), 90.0);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_forced_500_keeps_the_old_windows_as_stale() -> TestResult {
    guard(async {
        let (harness, account) = start(10).await?;
        harness.one(&account, Refresh::Cached).await?;
        harness.advance(10);
        serve(&harness, Route::status(500));
        let forced = harness.one(&account, Refresh::Force).await?;
        assert_stale(&forced);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_429_after_a_good_report_returns_the_old_windows_as_stale() -> TestResult {
    guard(async {
        let (_, _, usage) = stale_after(Route::status(429)).await?;
        assert_stale(&usage);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_second_failure_keeps_the_time_of_the_good_report() -> TestResult {
    guard(async {
        let (harness, account, _) = stale_after(Route::status(500)).await?;
        harness.advance(600);
        let again = harness.one(&account, Refresh::Cached).await?;
        assert_stale(&again);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_401_after_a_good_report_shows_no_old_windows() -> TestResult {
    guard(async {
        let (_, _, usage) = stale_after(Route::status(401)).await?;
        assert_eq!(
            unavailable(&usage.outcome),
            &UnavailableReason::ExpiredLogin
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_403_after_a_good_report_shows_no_old_windows() -> TestResult {
    guard(async {
        let (_, _, usage) = stale_after(Route::status(403)).await?;
        assert_eq!(
            unavailable(&usage.outcome),
            &UnavailableReason::ExpiredLogin
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_deleted_credential_file_after_a_good_report_shows_no_old_windows() -> TestResult {
    guard(async {
        let (harness, account) = start(10).await?;
        harness.one(&account, Refresh::Cached).await?;
        harness
            .fs
            .remove_file(&host_path("/home/work/.codex/auth.json"))
            .await?;
        harness.advance(600);
        let usage = harness.one(&account, Refresh::Cached).await?;
        assert_eq!(
            unavailable(&usage.outcome),
            &UnavailableReason::NoCredentials
        );
        Ok(())
    })
    .await
}

async fn first_failure(failure: Route) -> Result<UnavailableReason, Box<dyn std::error::Error>> {
    let (harness, account) = start(10).await?;
    serve(&harness, failure);
    let usage = harness.one(&account, Refresh::Cached).await?;
    assert_eq!(usage.freshness, Freshness::Fresh);
    Ok(unavailable(&usage.outcome).clone())
}

#[tokio::test]
async fn without_an_earlier_report_a_500_is_a_provider_reason() -> TestResult {
    guard(async {
        assert!(matches!(
            first_failure(Route::status(500)).await?,
            UnavailableReason::Provider(_)
        ));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn without_an_earlier_report_a_refused_connection_is_a_network_reason() -> TestResult {
    guard(async {
        let (harness, account) = start(10).await?;
        harness.server.shutdown().await;
        let usage = harness.one(&account, Refresh::Cached).await?;
        assert!(matches!(
            unavailable(&usage.outcome),
            UnavailableReason::Network(_)
        ));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn without_an_earlier_report_a_malformed_body_is_a_provider_reason() -> TestResult {
    guard(async {
        assert!(matches!(
            first_failure(Route::json(200, "{")).await?,
            UnavailableReason::Provider(_)
        ));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn without_an_earlier_report_a_429_is_a_rate_limited_reason() -> TestResult {
    guard(async {
        assert!(matches!(
            first_failure(Route::status(429)).await?,
            UnavailableReason::RateLimited { .. }
        ));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_same_account_name_with_another_login_home_is_fetched_again() -> TestResult {
    guard(async {
        let (harness, account) = start(10).await?;
        harness.one(&account, Refresh::Cached).await?;
        harness
            .put(
                "/home/other/.codex/auth.json",
                include_str!("fixtures/codex/auth_chatgpt.json"),
            )
            .await?;
        serve(&harness, codex_route(&codex_body(90)));
        harness.advance(10);
        let other = codex_login("work", "/home/other/.codex", "acct-1");
        let usage = harness.one(&other, Refresh::Cached).await?;
        assert_eq!(used_of(&usage), 90.0);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_corrupt_cache_file_is_rewritten_and_the_report_succeeds() -> TestResult {
    guard(async {
        let (harness, account) = start(10).await?;
        harness.put("/state/usage/usage.json", "not json").await?;
        let first = harness.one(&account, Refresh::Cached).await?;
        assert_eq!(used_of(&first), 10.0);
        serve(&harness, codex_route(&codex_body(90)));
        harness.advance(10);
        let again = harness.one(&account, Refresh::Cached).await?;
        assert_eq!(used_of(&again), 10.0);
        Ok(())
    })
    .await
}

fn denied() -> FsError {
    FsError::Io {
        path: host_path("/state/usage/usage.json"),
        kind: ErrorKind::PermissionDenied,
        message: "denied".to_owned(),
    }
}

#[tokio::test]
async fn a_cache_write_error_is_a_store_error_naming_the_path() -> TestResult {
    guard(async {
        let (harness, account) = start(10).await?;
        harness.fs.fail_at(FsOp::WriteNew, 1, denied());
        let result = harness.report(&[account], Refresh::Cached).await;
        match result {
            Err(UsageError::Store { path, .. }) => {
                assert_eq!(path, host_path("/state/usage/usage.json"));
            }
            other => panic!("expected a store error, found {other:?}"),
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cache_read_error_is_a_store_error_naming_the_path() -> TestResult {
    guard(async {
        let (harness, account) = crate::providers::openrouter_started().await?;
        harness.fs.fail_at(FsOp::Read, 1, denied());
        let result = harness.report(&[account], Refresh::Cached).await;
        match result {
            Err(UsageError::Store { path, .. }) => {
                assert_eq!(path, host_path("/state/usage/usage.json"));
            }
            other => panic!("expected a store error, found {other:?}"),
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_same_account_name_with_another_chatgpt_account_id_is_fetched_again() -> TestResult {
    guard(async {
        let (harness, account) = start(10).await?;
        harness.one(&account, Refresh::Cached).await?;
        harness.server.serve(
            Provider::Codex,
            Method::Get,
            CODEX_PATH,
            Route::json(200, &codex_body(90))
                .requires_header("authorization", "Bearer SECRET_TOKEN_CODEX")
                .requires_header("chatgpt-account-id", "acct-2"),
        );
        harness.advance(10);
        let other = codex_login("work", "/home/work/.codex", "acct-2");
        let usage = harness.one(&other, Refresh::Cached).await?;
        assert_eq!(used_of(&usage), 90.0);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_same_account_name_under_another_provider_is_fetched_again() -> TestResult {
    guard(async {
        let (harness, router) = crate::providers::openrouter_started().await?;
        let first = harness.one(&router, Refresh::Cached).await?;
        assert_eq!(windows_of(&first.outcome).len(), 2);
        harness.server.serve(
            Provider::Kimi,
            Method::Get,
            "/coding/v1/usages",
            Route::json(200, "{\"usage\":{\"limit\":100,\"used\":30}}"),
        );
        harness.advance(10);
        let kimi = crate::support::account(
            "router",
            Provider::Kimi,
            crate::support::env_source("OPENROUTER_TEST_KEY"),
        );
        let usage = harness.one(&kimi, Refresh::Cached).await?;
        assert_eq!(windows_of(&usage.outcome).len(), 1);
        Ok(())
    })
    .await
}
