// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::Provider;
use jackin_testkit::guard;
use jackin_usage::{Freshness, Refresh, UnavailableReason, WindowKind};

use crate::support::{
    CODEX_HOME, Left, TestResult, UsageHarness, assert_window, codex_login, unavailable, want,
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
async fn the_account_id_of_the_config_is_sent_and_not_the_one_in_the_file() -> TestResult {
    guard(async {
        let (harness, _) = UsageHarness::with_codex(FULL).await?;
        let other = codex_login("work", CODEX_HOME, "acct-3");
        let usage = harness.one(&other, Refresh::Cached).await?;
        assert_eq!(
            unavailable(&usage.outcome),
            &UnavailableReason::ExpiredLogin
        );
        Ok(())
    })
    .await
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
