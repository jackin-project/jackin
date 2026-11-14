// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::Provider;
use jackin_usage::{Freshness, Refresh, UnavailableReason, WindowKind};

use crate::support::{
    AUTH_CHATGPT, CODEX_HOME, CODEX_TOKEN, Left, TestResult, UsageHarness, assert_window,
    codex_auth, codex_body, codex_login, codex_route, guard, serve_codex, unavailable, want,
    windows_of,
};

const FULL: &str = include_str!("fixtures/codex/usage_full.json");
const RESET_AFTER: &str = include_str!("fixtures/codex/usage_reset_after.json");

#[tokio::test]
async fn a_chatgpt_login_gives_the_five_hour_and_the_weekly_window() -> TestResult {
    guard(async {
        let (harness, account) = UsageHarness::with_codex(FULL).await?;
        let usage = harness.one(&account, Refresh::Cached).await?;
        assert_eq!(usage.freshness, Freshness::Fresh);
        assert_eq!(usage.provider, Provider::Codex);
        let windows = windows_of(&usage.outcome);
        let five = want(
            WindowKind::FiveHour,
            "5-hour",
            Some(42.0),
            Left::Percent(58.0),
            Some(1_767_232_800),
        );
        let week = want(
            WindowKind::Weekly,
            "weekly",
            Some(12.0),
            Left::Percent(88.0),
            Some(1_767_571_200),
        );
        assert_window(&windows[0], &five);
        assert_window(&windows[1], &week);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_login_of_another_account_is_refused_before_the_request() -> TestResult {
    guard(async {
        let harness = UsageHarness::new().await?;
        harness
            .put(&format!("{CODEX_HOME}/auth.json"), AUTH_CHATGPT)
            .await?;
        serve_codex(&harness, CODEX_TOKEN, codex_route(&codex_body(42)));
        let other = codex_login("work", CODEX_HOME, "acct-2");
        let usage = harness.one(&other, Refresh::Cached).await?;
        assert_eq!(
            unavailable(&usage.outcome),
            &UnavailableReason::Provider(
                "the Codex login account is not the configured account".to_owned()
            )
        );
        assert!(
            harness.server.mismatches().is_empty(),
            "the request of the other account was sent"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_login_of_the_configured_account_is_sent_with_its_own_account_id() -> TestResult {
    guard(async {
        let harness = UsageHarness::new().await?;
        harness
            .put(&format!("{CODEX_HOME}/auth.json"), AUTH_CHATGPT)
            .await?;
        serve_codex(&harness, CODEX_TOKEN, codex_route(&codex_body(42)));
        let same = codex_login("work", CODEX_HOME, "acct-1");
        let usage = harness.one(&same, Refresh::Cached).await?;
        assert_eq!(windows_of(&usage.outcome).len(), 1);
        assert!(
            harness.server.mismatches().is_empty(),
            "{:?}",
            harness.server.mismatches()
        );
        Ok(())
    })
    .await
}

#[test]
fn the_shared_login_fixture_belongs_to_the_shared_account() {
    assert_eq!(codex_auth("acct-1"), AUTH_CHATGPT.trim_end());
}

#[tokio::test]
async fn a_window_without_reset_at_resets_after_the_given_seconds() -> TestResult {
    guard(async {
        let (harness, account) = UsageHarness::with_codex(RESET_AFTER).await?;
        let usage = harness.one(&account, Refresh::Cached).await?;
        let first = &windows_of(&usage.outcome)[0];
        assert_eq!(first.resets_at.map(|at| at.0), Some(1_767_232_800));
        Ok(())
    })
    .await
}
