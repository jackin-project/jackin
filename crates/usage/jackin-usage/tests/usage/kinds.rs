// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_testkit::guard;
use jackin_usage::{AccountUsage, Refresh, UnavailableReason, WindowKind};

use crate::support::{
    Left, TestResult, UsageHarness, assert_window, assert_windows, unavailable, want, windows_of,
};

async fn usage_for(body: &str) -> Result<AccountUsage, Box<dyn std::error::Error>> {
    let (harness, account) = UsageHarness::with_codex(body).await?;
    Ok(harness.one(&account, Refresh::Cached).await?)
}

const FIVE: crate::support::Want = want(
    WindowKind::FiveHour,
    "5-hour",
    Some(42.0),
    Left::Percent(58.0),
    Some(1_767_232_800),
);

const CREDITS: crate::support::Want = want(
    WindowKind::Credits,
    "credits",
    None,
    Left::Amount("12.40", "credits"),
    None,
);

#[tokio::test]
async fn the_window_length_beats_the_slot() -> TestResult {
    guard(async {
        let usage = usage_for(include_str!("fixtures/codex/usage_length_beats_slot.json")).await?;
        assert_windows(
            &usage.outcome,
            &[
                want(
                    WindowKind::Weekly,
                    "weekly",
                    Some(10.0),
                    Left::Percent(90.0),
                    Some(1_767_571_200),
                ),
                want(
                    WindowKind::FiveHour,
                    "5-hour",
                    Some(20.0),
                    Left::Percent(80.0),
                    Some(1_767_232_800),
                ),
            ],
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_position_decides_when_the_length_is_absent() -> TestResult {
    guard(async {
        let usage = usage_for(include_str!("fixtures/codex/usage_no_length.json")).await?;
        assert_windows(
            &usage.outcome,
            &[
                FIVE,
                want(
                    WindowKind::Weekly,
                    "weekly",
                    Some(12.0),
                    Left::Percent(88.0),
                    Some(1_767_571_200),
                ),
            ],
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_additional_limit_carries_its_name_in_the_label() -> TestResult {
    guard(async {
        let usage = usage_for(include_str!("fixtures/codex/usage_full.json")).await?;
        let windows = windows_of(&usage.outcome);
        let five = want(
            WindowKind::FiveHour,
            "Spark 5-hour",
            Some(5.0),
            Left::Percent(95.0),
            Some(1_767_243_600),
        );
        let week = want(
            WindowKind::Weekly,
            "Spark weekly",
            Some(2.0),
            Left::Percent(98.0),
            Some(1_767_830_400),
        );
        assert_window(&windows[2], &five);
        assert_window(&windows[3], &week);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_credit_balance_given_as_a_number_keeps_two_decimals() -> TestResult {
    guard(async {
        let usage = usage_for(include_str!("fixtures/codex/usage_credits_number.json")).await?;
        assert_windows(&usage.outcome, &[FIVE, CREDITS]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_credit_balance_given_as_a_string_keeps_two_decimals() -> TestResult {
    guard(async {
        let usage = usage_for(include_str!("fixtures/codex/usage_credits_string.json")).await?;
        assert_windows(&usage.outcome, &[FIVE, CREDITS]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn no_credits_means_no_credit_window() -> TestResult {
    guard(async {
        let usage = usage_for(include_str!("fixtures/codex/usage_credits_none.json")).await?;
        assert_windows(&usage.outcome, &[FIVE]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn unlimited_credits_with_a_balance_show_the_balance() -> TestResult {
    guard(async {
        let usage = usage_for(include_str!("fixtures/codex/usage_credits_unlimited.json")).await?;
        assert_windows(&usage.outcome, &[FIVE, CREDITS]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_individual_limit_shows_money_left_and_the_percent_used() -> TestResult {
    guard(async {
        let usage = usage_for(include_str!("fixtures/codex/usage_full.json")).await?;
        let windows = windows_of(&usage.outcome);
        let spend = want(
            WindowKind::Spend,
            "Individual limit",
            Some(30.0),
            Left::Amount("70.00", "USD"),
            Some(1_767_312_000),
        );
        assert_window(&windows[5], &spend);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_overspent_individual_limit_shows_zero_left_from_either_source() -> TestResult {
    guard(async {
        for body in [
            include_str!("fixtures/codex/usage_individual_overspent_remaining.json"),
            include_str!("fixtures/codex/usage_individual_overspent_used.json"),
        ] {
            let usage = usage_for(body).await?;
            assert_windows(
                &usage.outcome,
                &[want(
                    WindowKind::Spend,
                    "Individual limit",
                    Some(100.0),
                    Left::Amount("0.00", "USD"),
                    Some(1_767_312_000),
                )],
            );
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_individual_limit_with_only_a_remaining_percent_shows_that_percent() -> TestResult {
    guard(async {
        let usage = usage_for(include_str!(
            "fixtures/codex/usage_individual_percent_only.json"
        ))
        .await?;
        assert_windows(
            &usage.outcome,
            &[want(
                WindowKind::Spend,
                "Individual limit",
                Some(30.0),
                Left::Percent(70.0),
                Some(1_767_312_000),
            )],
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_remaining_percent_gives_the_exact_complement_as_used() -> TestResult {
    guard(async {
        let usage = usage_for(include_str!(
            "fixtures/codex/usage_individual_percent_half.json"
        ))
        .await?;
        assert_windows(
            &usage.outcome,
            &[want(
                WindowKind::Spend,
                "Individual limit",
                Some(99.5),
                Left::Percent(0.5),
                Some(1_767_312_000),
            )],
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn month_and_odd_window_lengths_get_a_kind_and_a_label() -> TestResult {
    guard(async {
        let usage = usage_for(include_str!("fixtures/codex/usage_lengths.json")).await?;
        assert_windows(
            &usage.outcome,
            &[
                want(
                    WindowKind::Monthly,
                    "monthly",
                    Some(30.0),
                    Left::Percent(70.0),
                    Some(1_769_817_600),
                ),
                want(
                    WindowKind::Other,
                    "2 hours window",
                    Some(40.0),
                    Left::Percent(60.0),
                    Some(1_767_232_800),
                ),
            ],
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn percentages_outside_the_range_are_clamped() -> TestResult {
    guard(async {
        let usage = usage_for(include_str!("fixtures/codex/usage_clamp.json")).await?;
        assert_windows(
            &usage.outcome,
            &[
                want(
                    WindowKind::FiveHour,
                    "5-hour",
                    Some(100.0),
                    Left::Percent(0.0),
                    Some(1_767_232_800),
                ),
                want(
                    WindowKind::Weekly,
                    "weekly",
                    Some(0.0),
                    Left::Percent(100.0),
                    Some(1_767_571_200),
                ),
            ],
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_number_that_is_not_finite_makes_the_account_a_provider_error() -> TestResult {
    guard(async {
        let usage = usage_for(include_str!("fixtures/codex/usage_inf.json")).await?;
        assert!(matches!(
            unavailable(&usage.outcome),
            UnavailableReason::Provider(_)
        ));
        Ok(())
    })
    .await
}
