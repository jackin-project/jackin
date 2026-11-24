// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::io::ErrorKind;
use std::time::Duration;

use jackin_core::{FileMode, FileSystem, FsError, Provider, Timestamp};
use jackin_testkit::{FsOp, Held, Method, Route, UsageServer};
use jackin_usage::{
    AccountUsage, Freshness, Percent, Refresh, UnavailableReason, UsageAccount, UsageError,
    UsageOutcome,
};

use crate::support::{
    AUTH_CHATGPT, CODEX_PATH, CODEX_TOKEN, RESET_CREDITS_PATH, T0, TestResult, UsageHarness,
    account, codex_body, codex_login, codex_reset_credits_route, codex_route, env_source,
    file_source, guard, host_path, put_codex_auth, serve_codex, unavailable, windows_of,
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
        assert_eq!(
            unavailable(&usage.outcome),
            &UnavailableReason::Provider("the response is not valid JSON".to_owned())
        );
        serve(&harness, codex_route(&codex_body(90)));
        harness.advance(5);
        let again = harness.one(&account, Refresh::Cached).await?;
        assert_eq!((again.freshness, used_of(&again)), (Freshness::Fresh, 90.0));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_404_after_a_good_report_replaces_the_old_windows() -> TestResult {
    guard(async {
        let (harness, account, usage) = stale_after(Route::status(404)).await?;
        assert_eq!(
            unavailable(&usage.outcome),
            &UnavailableReason::Provider("the provider answered HTTP 404".to_owned())
        );
        serve(&harness, codex_route(&codex_body(90)));
        harness.advance(5);
        let again = harness.one(&account, Refresh::Cached).await?;
        assert_eq!((again.freshness, used_of(&again)), (Freshness::Fresh, 90.0));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_503_after_a_good_report_returns_the_old_windows_as_stale() -> TestResult {
    guard(async {
        let (_, _, usage) = stale_after(Route::status(503)).await?;
        assert_stale(&usage);
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
            &UnavailableReason::Provider("the login is not allowed to read this".to_owned())
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

#[tokio::test]
async fn a_cache_larger_than_the_limit_is_ignored_and_replaced() -> TestResult {
    guard(async {
        let (harness, account) = start(10).await?;
        harness.one(&account, Refresh::Cached).await?;
        let mut cache = harness.cache_text().ok_or("no cache was written")?;
        cache.push_str(&" ".repeat((1 << 20) + 1));
        let path = host_path("/state/usage/usage.json");
        harness.fs.remove_file(&path).await?;
        harness.put("/state/usage/usage.json", &cache).await?;
        serve(&harness, codex_route(&codex_body(90)));
        let again = harness.one(&account, Refresh::Cached).await?;
        assert_eq!(used_of(&again), 90.0);
        assert!(
            harness
                .cache_text()
                .is_some_and(|text| text.len() < 1 << 20)
        );
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
        harness
            .fs
            .remove_file(&host_path("/home/work/.codex/auth.json"))
            .await?;
        put_codex_auth(&harness, "/home/work/.codex", "acct-2").await?;
        serve_codex(
            &harness,
            "SECRET_TOKEN_CODEX",
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
        assert_eq!(windows_of(&first.outcome).len(), 5);
        harness.server.serve(
            Provider::Kimi,
            Method::Get,
            "/coding/v1/usages",
            Route::json(200, "{\"usages\":{\"limit_5h\":{\"used_ratio\":0.3}}}"),
        );
        harness.advance(10);
        let kimi = account("router", Provider::Kimi, env_source("OPENROUTER_TEST_KEY"));
        let usage = harness.one(&kimi, Refresh::Cached).await?;
        assert_eq!(windows_of(&usage.outcome).len(), 1);
        Ok(())
    })
    .await
}

const OPENROUTER_KEY_PATH: &str = "/api/v1/key";
const ROUTER_KEY: &str = "ROUTER_TEST_KEY";

async fn start_two()
-> Result<(UsageHarness, UsageAccount, UsageAccount), Box<dyn std::error::Error>> {
    let (mut harness, codex) = start(10).await?;
    harness.env("OPENROUTER_TEST_KEY", ROUTER_KEY);
    crate::providers::openrouter_serve(&harness, ROUTER_KEY);
    let router = account(
        "router",
        Provider::Openrouter,
        env_source("OPENROUTER_TEST_KEY"),
    );
    Ok((harness, codex, router))
}

async fn two_runs(
    harness: &UsageHarness,
    parked: (&UsageAccount, Refresh),
    other: (&UsageAccount, Refresh),
    held: Held,
) -> TestResult {
    let (first, second) = (harness.host(), harness.host());
    let parked_run = harness.report_on(&first, std::slice::from_ref(parked.0), parked.1);
    let other_run = async {
        held.arrived().await;
        let done = harness
            .report_on(&second, std::slice::from_ref(other.0), other.1)
            .await;
        held.release();
        done
    };
    let (parked_done, other_done) = tokio::join!(parked_run, other_run);
    parked_done?;
    other_done?;
    Ok(())
}

async fn a_second_run_keeps_the_entry_of_the_first() -> TestResult {
    let (harness, codex, router) = start_two().await?;
    let held = harness
        .server
        .hold(Provider::Codex, Method::Get, CODEX_PATH);
    two_runs(
        &harness,
        (&codex, Refresh::Cached),
        (&router, Refresh::Cached),
        held,
    )
    .await?;
    harness.server.shutdown().await;
    for account in [&codex, &router] {
        let usage = harness.one(account, Refresh::Cached).await?;
        if !matches!(usage.outcome, UsageOutcome::Windows(_)) {
            return Err(format!(
                "{} was not served from the cache: {:?}",
                account.name.as_str(),
                usage.outcome
            )
            .into());
        }
    }
    Ok(())
}

async fn a_stale_save_does_not_bring_back_a_removed_entry() -> TestResult {
    let (harness, codex, router) = start_two().await?;
    harness.one(&codex, Refresh::Cached).await?;
    let held = harness
        .server
        .hold(Provider::Openrouter, Method::Get, OPENROUTER_KEY_PATH);
    serve(&harness, Route::status(401));
    two_runs(
        &harness,
        (&router, Refresh::Cached),
        (&codex, Refresh::Force),
        held,
    )
    .await?;
    serve(&harness, Route::status(500));
    let usage = harness.one(&codex, Refresh::Cached).await?;
    if !matches!(usage.outcome, UsageOutcome::Unavailable(_)) {
        return Err(format!("the removed entry came back: {:?}", usage.outcome).into());
    }
    Ok(())
}

#[tokio::test]
async fn concurrent_reports_neither_drop_nor_bring_back_cache_entries() -> TestResult {
    guard(async {
        let kept = a_second_run_keeps_the_entry_of_the_first().await;
        let removed = a_stale_save_does_not_bring_back_a_removed_entry().await;
        assert!(
            kept.is_ok() && removed.is_ok(),
            "kept: {kept:?}; removed: {removed:?}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_save_sweeps_the_crash_leftovers_of_the_cache_file_and_keeps_every_other_file()
-> TestResult {
    guard(async {
        let (harness, account) = start(10).await?;
        let leftovers = [
            "/state/usage/.usage.json.4242-0.tmp",
            "/state/usage/.usage.json.7-31.tmp",
        ];
        let others = [
            "/state/usage/.usage.json.tmp",
            "/state/usage/.usage.json.4242-0.tmp.bak",
            "/state/usage/.usage.json.x-0.tmp",
            "/state/usage/.usage.json.4242-.tmp",
            "/state/usage/.usage.json.4242-0.5-6.tmp",
            "/state/usage/usage.json.4242-0.tmp",
            "/state/usage/.other.json.4242-0.tmp",
            "/state/usage/.usage.json.lock.4242-0.tmp",
        ];
        for path in leftovers.iter().chain(&others) {
            harness.put(path, "leftover").await?;
        }
        harness.one(&account, Refresh::Cached).await?;
        let mut expected: Vec<String> = others.iter().map(ToString::to_string).collect();
        expected.extend(
            [
                "/home/work/.codex/auth.json",
                "/state/usage/usage.json",
                "/state/usage/usage.json.lock",
            ]
            .map(str::to_owned),
        );
        expected.sort();
        let found: Vec<String> = harness
            .all_files()
            .await?
            .into_iter()
            .map(|(path, _)| path)
            .collect();
        assert_eq!(found, expected);
        Ok(())
    })
    .await
}

const OP_REFERENCE: &str = "op://v/i/f";

fn router_through(harness: &UsageHarness, account: &str, token: &str) -> UsageAccount {
    harness.runner.expect(
        jackin_testkit::ScriptedProgram::Op,
        &["read", "--account", account, OP_REFERENCE],
        jackin_testkit::ProcessReply::exited(0, format!("{token}\n").as_bytes(), b""),
    );
    crate::support::account(
        "router",
        Provider::Openrouter,
        crate::support::op_source_in(OP_REFERENCE, account),
    )
}

#[tokio::test]
async fn two_one_password_accounts_of_one_name_do_not_share_a_cached_entry() -> TestResult {
    guard(async {
        let harness = UsageHarness::new().await?;
        crate::providers::openrouter_serve(&harness, ROUTER_KEY);
        let first = router_through(&harness, "acct-1", ROUTER_KEY);
        let second = router_through(&harness, "acct-2", ROUTER_KEY);
        let fetched = harness.one(&first, Refresh::Cached).await?;
        assert!(matches!(fetched.outcome, UsageOutcome::Windows(_)));
        harness.server.replace(
            Provider::Openrouter,
            Method::Get,
            OPENROUTER_KEY_PATH,
            Route::status(500),
        );
        harness.advance(1);
        let same = harness.one(&first, Refresh::Cached).await?;
        assert!(matches!(same.outcome, UsageOutcome::Windows(_)));
        let other = harness.one(&second, Refresh::Cached).await?;
        assert!(
            matches!(unavailable(&other.outcome), UnavailableReason::Provider(_)),
            "the second account inherited the cached windows of the first"
        );
        Ok(())
    })
    .await
}

const ROTATION_PATH: &str = "/keys/router";
const TOKEN_A: &str = "SECRET_TOKEN_ROTATION_A";
const TOKEN_B: &str = "SECRET_TOKEN_ROTATION_B";
const KEY_A: &str = "{\"data\":{\"limit\":50.0,\"limit_remaining\":37.66,\"usage\":12.34,\"usage_daily\":1.0,\"usage_weekly\":5.0,\"usage_monthly\":12.34}}";
const KEY_B: &str = "{\"data\":{\"limit\":50.0,\"limit_remaining\":12.34,\"usage\":37.66,\"usage_daily\":1.0,\"usage_weekly\":5.0,\"usage_monthly\":12.34}}";

fn serve_keys(harness: &UsageHarness) {
    harness.server.replace(
        Provider::Openrouter,
        Method::Get,
        OPENROUTER_KEY_PATH,
        Route::json(200, KEY_A).requires_header("authorization", &format!("Bearer {TOKEN_A}")),
    );
    harness.server.serve(
        Provider::Openrouter,
        Method::Get,
        OPENROUTER_KEY_PATH,
        Route::json(200, KEY_B).requires_header("authorization", &format!("Bearer {TOKEN_B}")),
    );
}

async fn rotation_harness() -> Result<(UsageHarness, UsageAccount), Box<dyn std::error::Error>> {
    let harness = UsageHarness::new().await?;
    harness.put(ROTATION_PATH, TOKEN_A).await?;
    serve_keys(&harness);
    let account = account("router", Provider::Openrouter, file_source(ROTATION_PATH));
    Ok((harness, account))
}

async fn rotate(harness: &UsageHarness) -> TestResult {
    harness.fs.remove_file(&host_path(ROTATION_PATH)).await?;
    harness.put(ROTATION_PATH, TOKEN_B).await?;
    Ok(())
}

fn first_used(usage: &AccountUsage) -> f64 {
    windows_of(&usage.outcome)[0]
        .used
        .map_or(-1.0, Percent::get)
}

#[tokio::test]
async fn a_token_rotated_at_the_same_path_is_not_served_from_the_cache() -> TestResult {
    guard(async {
        let (harness, account) = rotation_harness().await?;
        let first = harness.one(&account, Refresh::Cached).await?;
        assert!(
            (first_used(&first) - 24.68).abs() < 1e-9,
            "{}",
            first_used(&first)
        );
        rotate(&harness).await?;
        let again = harness.one(&account, Refresh::Cached).await?;
        assert!(
            (first_used(&again) - 75.32).abs() < 1e-9,
            "the cached windows of the old token were served"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_token_rotated_at_the_same_path_does_not_inherit_the_retry_time() -> TestResult {
    guard(async {
        let (harness, account) = rotation_harness().await?;
        harness.server.replace(
            Provider::Openrouter,
            Method::Get,
            OPENROUTER_KEY_PATH,
            Route::status(429).reply_header("Retry-After", "600"),
        );
        let first = harness.one(&account, Refresh::Cached).await?;
        assert_eq!(
            unavailable(&first.outcome),
            &UnavailableReason::RateLimited {
                retry_at: Some(Timestamp(T0 + 600))
            }
        );
        rotate(&harness).await?;
        serve_keys(&harness);
        let again = harness.one(&account, Refresh::Cached).await?;
        assert!(
            matches!(again.outcome, UsageOutcome::Windows(_)),
            "the retry time of the old token was served: {:?}",
            again.outcome
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_token_rotated_at_the_same_path_does_not_inherit_the_stale_windows() -> TestResult {
    guard(async {
        let (harness, account) = rotation_harness().await?;
        let first = harness.one(&account, Refresh::Cached).await?;
        assert!(matches!(first.outcome, UsageOutcome::Windows(_)));
        rotate(&harness).await?;
        harness.server.replace(
            Provider::Openrouter,
            Method::Get,
            OPENROUTER_KEY_PATH,
            Route::status(500),
        );
        let again = harness.one(&account, Refresh::Cached).await?;
        assert!(
            matches!(unavailable(&again.outcome), UnavailableReason::Provider(_)),
            "the stale windows of the old token were served: {:?}",
            again.outcome
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_environment_credential_that_changed_is_fetched_again_inside_the_ttl() -> TestResult {
    guard(async {
        let mut harness = UsageHarness::new().await?;
        harness.set_env("ROTATION_ENV", TOKEN_A);
        harness.server.replace(
            Provider::Openrouter,
            Method::Get,
            OPENROUTER_KEY_PATH,
            Route::json(200, KEY_A).requires_header("authorization", &format!("Bearer {TOKEN_A}")),
        );
        let account = account("router", Provider::Openrouter, env_source("ROTATION_ENV"));
        let first = harness.one(&account, Refresh::Cached).await?;
        assert!((first_used(&first) - 24.68).abs() < 1e-9);
        harness.set_env("ROTATION_ENV", TOKEN_B);
        harness.server.replace(
            Provider::Openrouter,
            Method::Get,
            OPENROUTER_KEY_PATH,
            Route::json(200, KEY_B).requires_header("authorization", &format!("Bearer {TOKEN_B}")),
        );
        harness.advance(1);
        let again = harness.one(&account, Refresh::Cached).await?;
        assert!(
            (first_used(&again) - 75.32).abs() < 1e-9,
            "the cached windows of the old environment value were served"
        );
        Ok(())
    })
    .await
}

const NEWER_TOKEN: &str = "SECRET_TOKEN_CODEX_NEWER";
const CODEX_AUTH_PATH: &str = "/home/work/.codex/auth.json";

#[derive(Clone, Copy)]
enum First {
    Early,
    Late,
}

struct Side {
    token: &'static str,
    usage: Route,
    refresh: Refresh,
}

async fn side_server(side: &Side) -> Result<(UsageServer, Held), Box<dyn std::error::Error>> {
    let server = UsageServer::start().await?;
    server.serve(
        Provider::Codex,
        Method::Get,
        CODEX_PATH,
        side.usage
            .clone()
            .requires_header("authorization", &format!("Bearer {}", side.token))
            .requires_header("chatgpt-account-id", "acct-1"),
    );
    server.serve(
        Provider::Codex,
        Method::Get,
        RESET_CREDITS_PATH,
        codex_reset_credits_route(side.token),
    );
    let held = server.hold(Provider::Codex, Method::Get, CODEX_PATH);
    Ok((server, held))
}

async fn wait_for_a_save(harness: &UsageHarness, before: Option<String>) -> TestResult {
    for _ in 0..500 {
        if harness.cache_text() != before {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err("the instance never saved".into())
}

async fn overlapped(
    harness: &UsageHarness,
    account: &UsageAccount,
    early: Side,
    late: Side,
    gap: u64,
    late_login: Option<String>,
    first: First,
) -> TestResult {
    let (early_server, early_gate) = side_server(&early).await?;
    let (late_server, late_gate) = side_server(&late).await?;
    let early_host = harness.host_with(early_server.endpoints());
    let late_host = harness.host_with(late_server.endpoints());
    let early_run = harness.report_on(&early_host, std::slice::from_ref(account), early.refresh);
    let late_run = async {
        early_gate.arrived().await;
        harness.advance(gap);
        if let Some(login) = &late_login {
            harness.fs.remove_file(&host_path(CODEX_AUTH_PATH)).await?;
            harness.put(CODEX_AUTH_PATH, login).await?;
        }
        harness
            .report_on(&late_host, std::slice::from_ref(account), late.refresh)
            .await?;
        Ok::<(), Box<dyn std::error::Error>>(())
    };
    let order = async {
        late_gate.arrived().await;
        let (now, then) = match first {
            First::Early => (&early_gate, &late_gate),
            First::Late => (&late_gate, &early_gate),
        };
        let before = harness.cache_text();
        now.release();
        wait_for_a_save(harness, before).await?;
        then.release();
        Ok::<(), Box<dyn std::error::Error>>(())
    };
    let (early_done, late_done, ordered) = tokio::join!(early_run, late_run, order);
    early_done?;
    late_done?;
    ordered
}

fn side(token: &'static str, usage: Route, refresh: Refresh) -> Side {
    Side {
        token,
        usage,
        refresh,
    }
}

fn windows_route(used: u32) -> Route {
    Route::json(200, &codex_body(used))
}

async fn account_only() -> Result<(UsageHarness, UsageAccount), Box<dyn std::error::Error>> {
    let harness = UsageHarness::new().await?;
    harness.put(CODEX_AUTH_PATH, AUTH_CHATGPT).await?;
    Ok((harness, codex_login("work", "/home/work/.codex", "acct-1")))
}

async fn settled_read(
    harness: &UsageHarness,
    account: &UsageAccount,
) -> Result<AccountUsage, Box<dyn std::error::Error>> {
    harness.advance(1);
    Ok(harness.one(account, Refresh::Cached).await?)
}

#[tokio::test]
async fn an_earlier_request_that_finishes_last_does_not_replace_the_later_report() -> TestResult {
    guard(async {
        let (harness, account) = account_only().await?;
        overlapped(
            &harness,
            &account,
            side(CODEX_TOKEN, windows_route(10), Refresh::Cached),
            side(CODEX_TOKEN, windows_route(70), Refresh::Cached),
            100,
            None,
            First::Late,
        )
        .await?;
        let usage = settled_read(&harness, &account).await?;
        assert_eq!((usage.freshness, used_of(&usage)), (Freshness::Fresh, 70.0));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_later_request_that_finishes_last_replaces_the_earlier_report() -> TestResult {
    guard(async {
        let (harness, account) = account_only().await?;
        overlapped(
            &harness,
            &account,
            side(CODEX_TOKEN, windows_route(10), Refresh::Cached),
            side(CODEX_TOKEN, windows_route(70), Refresh::Cached),
            100,
            None,
            First::Early,
        )
        .await?;
        let usage = settled_read(&harness, &account).await?;
        assert_eq!(used_of(&usage), 70.0);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn requests_that_start_in_the_same_second_keep_the_report_that_saves_last() -> TestResult {
    guard(async {
        for (first, expected) in [(First::Early, 70.0), (First::Late, 10.0)] {
            let (harness, account) = account_only().await?;
            overlapped(
                &harness,
                &account,
                side(CODEX_TOKEN, windows_route(10), Refresh::Cached),
                side(CODEX_TOKEN, windows_route(70), Refresh::Cached),
                0,
                None,
                first,
            )
            .await?;
            assert_eq!(used_of(&settled_read(&harness, &account).await?), expected);
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_later_rate_limit_keeps_its_cooldown_and_an_earlier_report_that_saves_last() -> TestResult
{
    guard(async {
        for first in [First::Late, First::Early] {
            let (harness, account) = account_only().await?;
            overlapped(
                &harness,
                &account,
                side(CODEX_TOKEN, windows_route(10), Refresh::Cached),
                side(
                    CODEX_TOKEN,
                    Route::status(429).reply_header("Retry-After", "600"),
                    Refresh::Cached,
                ),
                100,
                None,
                first,
            )
            .await?;
            serve_codex(&harness, CODEX_TOKEN, codex_route(&codex_body(90)));
            harness.advance(150);
            let usage = harness.one(&account, Refresh::Cached).await?;
            assert_eq!(
                usage.freshness,
                Freshness::Stale {
                    since: Timestamp(T0)
                }
            );
            assert_eq!(used_of(&usage), 10.0);
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_success_after_a_cooldown_that_started_earlier_clears_it() -> TestResult {
    guard(async {
        let (harness, account) = account_only().await?;
        overlapped(
            &harness,
            &account,
            side(
                CODEX_TOKEN,
                Route::status(429).reply_header("Retry-After", "600"),
                Refresh::Cached,
            ),
            side(CODEX_TOKEN, windows_route(70), Refresh::Cached),
            100,
            None,
            First::Early,
        )
        .await?;
        serve_codex(&harness, CODEX_TOKEN, codex_route(&codex_body(90)));
        harness.advance(150);
        let usage = harness.one(&account, Refresh::Cached).await?;
        assert_eq!((usage.freshness, used_of(&usage)), (Freshness::Fresh, 90.0));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_rotated_credential_is_not_replaced_by_the_older_credential_that_saves_last() -> TestResult
{
    guard(async {
        for (first, expected) in [(First::Late, 70.0), (First::Early, 70.0)] {
            let (harness, account) = account_only().await?;
            let login = AUTH_CHATGPT.replace(CODEX_TOKEN, NEWER_TOKEN);
            overlapped(
                &harness,
                &account,
                side(CODEX_TOKEN, windows_route(10), Refresh::Cached),
                side(NEWER_TOKEN, windows_route(70), Refresh::Cached),
                100,
                Some(login),
                first,
            )
            .await?;
            let usage = settled_read(&harness, &account).await?;
            assert_eq!(used_of(&usage), expected);
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_older_removal_does_not_delete_a_newer_report_and_a_newer_one_does() -> TestResult {
    guard(async {
        let (harness, account) = account_only().await?;
        overlapped(
            &harness,
            &account,
            side(CODEX_TOKEN, Route::status(401), Refresh::Cached),
            side(CODEX_TOKEN, windows_route(70), Refresh::Cached),
            100,
            None,
            First::Late,
        )
        .await?;
        assert_eq!(used_of(&settled_read(&harness, &account).await?), 70.0);

        let (harness, account) = account_only().await?;
        overlapped(
            &harness,
            &account,
            side(CODEX_TOKEN, windows_route(10), Refresh::Cached),
            side(CODEX_TOKEN, Route::status(401), Refresh::Cached),
            100,
            None,
            First::Early,
        )
        .await?;
        serve_codex(&harness, CODEX_TOKEN, codex_route(&codex_body(55)));
        assert_eq!(used_of(&settled_read(&harness, &account).await?), 55.0);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_newer_removal_is_not_undone_by_an_older_report_that_saves_after_it() -> TestResult {
    guard(async {
        let (harness, account) = account_only().await?;
        overlapped(
            &harness,
            &account,
            side(CODEX_TOKEN, windows_route(10), Refresh::Cached),
            side(CODEX_TOKEN, Route::status(401), Refresh::Cached),
            100,
            None,
            First::Late,
        )
        .await?;
        serve_codex(&harness, CODEX_TOKEN, codex_route(&codex_body(55)));
        assert_eq!(used_of(&settled_read(&harness, &account).await?), 55.0);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_removal_wins_over_a_report_observed_in_the_same_second_in_either_order() -> TestResult {
    guard(async {
        for first in [First::Early, First::Late] {
            let (harness, account) = account_only().await?;
            overlapped(
                &harness,
                &account,
                side(CODEX_TOKEN, windows_route(10), Refresh::Cached),
                side(CODEX_TOKEN, Route::status(401), Refresh::Cached),
                0,
                None,
                first,
            )
            .await?;
            serve_codex(&harness, CODEX_TOKEN, codex_route(&codex_body(55)));
            assert_eq!(used_of(&settled_read(&harness, &account).await?), 55.0);
        }
        Ok(())
    })
    .await
}

async fn seeded() -> Result<(UsageHarness, UsageAccount), Box<dyn std::error::Error>> {
    let (harness, account) = account_only().await?;
    serve_codex(&harness, CODEX_TOKEN, codex_route(&codex_body(5)));
    assert_eq!(used_of(&harness.one(&account, Refresh::Cached).await?), 5.0);
    harness.advance(10);
    Ok((harness, account))
}

#[tokio::test]
async fn a_failed_refresh_that_saves_last_does_not_bring_back_the_older_report() -> TestResult {
    guard(async {
        let (harness, account) = seeded().await?;
        overlapped(
            &harness,
            &account,
            side(CODEX_TOKEN, Route::status(500), Refresh::Force),
            side(CODEX_TOKEN, windows_route(70), Refresh::Force),
            20,
            None,
            First::Late,
        )
        .await?;
        let usage = settled_read(&harness, &account).await?;
        assert_eq!((usage.freshness, used_of(&usage)), (Freshness::Fresh, 70.0));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_later_failed_refresh_keeps_the_report_the_earlier_request_saved() -> TestResult {
    guard(async {
        let (harness, account) = seeded().await?;
        overlapped(
            &harness,
            &account,
            side(CODEX_TOKEN, windows_route(10), Refresh::Force),
            side(CODEX_TOKEN, Route::status(500), Refresh::Force),
            20,
            None,
            First::Early,
        )
        .await?;
        let usage = settled_read(&harness, &account).await?;
        assert_eq!((usage.freshness, used_of(&usage)), (Freshness::Fresh, 10.0));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn credentials_that_rotate_in_the_same_second_keep_the_credential_that_saves_last()
-> TestResult {
    guard(async {
        for (first, expected) in [(First::Early, 70.0), (First::Late, 33.0)] {
            let (harness, account) = account_only().await?;
            let login = AUTH_CHATGPT.replace(CODEX_TOKEN, NEWER_TOKEN);
            overlapped(
                &harness,
                &account,
                side(CODEX_TOKEN, windows_route(10), Refresh::Cached),
                side(NEWER_TOKEN, windows_route(70), Refresh::Cached),
                0,
                Some(login),
                first,
            )
            .await?;
            serve_codex(
                &harness,
                NEWER_TOKEN,
                windows_route(33)
                    .requires_header("authorization", &format!("Bearer {NEWER_TOKEN}"))
                    .requires_header("chatgpt-account-id", "acct-1"),
            );
            let usage = settled_read(&harness, &account).await?;
            assert_eq!(used_of(&usage), expected);
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cooldown_that_has_passed_does_not_hold_a_later_request() -> TestResult {
    guard(async {
        let (harness, account) = seeded().await?;
        harness.server.replace(
            Provider::Codex,
            Method::Get,
            CODEX_PATH,
            Route::status(429).reply_header("Retry-After", "60"),
        );
        let limited = harness.one(&account, Refresh::Force).await?;
        assert!(matches!(limited.freshness, Freshness::Stale { .. }));
        harness.advance(30);
        let held = harness.one(&account, Refresh::Cached).await?;
        assert_eq!(used_of(&held), 5.0);
        harness.server.replace(
            Provider::Codex,
            Method::Get,
            CODEX_PATH,
            codex_route(&codex_body(90)),
        );
        harness.advance(120);
        let usage = harness.one(&account, Refresh::Cached).await?;
        assert_eq!((usage.freshness, used_of(&usage)), (Freshness::Fresh, 90.0));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cache_file_from_before_observation_times_is_replaced_not_trusted() -> TestResult {
    guard(async {
        let (harness, account) = start(10).await?;
        harness
            .put(
                "/state/usage/usage.json",
                r#"{"entries":{"work":{"fingerprint":"f","good":null,"retry_at":99999999999}}}"#,
            )
            .await?;
        let first = harness.one(&account, Refresh::Cached).await?;
        assert_eq!(used_of(&first), 10.0);
        let text = harness.cache_text().unwrap_or_default();
        assert!(
            text.contains("\"hold\"") && !text.contains("retry_at"),
            "{text}"
        );
        Ok(())
    })
    .await
}
