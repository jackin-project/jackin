// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_testkit::guard;
use std::time::Duration;

use jackin_core::Provider;
use jackin_testkit::{Method, Route};
use jackin_usage::{Refresh, UnavailableReason, UsageOutcome};

use crate::support::{
    CODEX_PATH, TestResult, UsageHarness, assert_only_state_written, codex_body, codex_login,
};

const AUTH: &str = include_str!("fixtures/codex/auth_chatgpt.json");

async fn three_accounts()
-> Result<(UsageHarness, Vec<jackin_usage::UsageAccount>), Box<dyn std::error::Error>> {
    let harness = UsageHarness::new().await?;
    harness.put("/home/b/.codex/auth.json", AUTH).await?;
    harness.put("/home/a/.codex/auth.json", AUTH).await?;
    harness.server.serve(
        Provider::Codex,
        Method::Get,
        CODEX_PATH,
        Route::json(200, &codex_body(10))
            .requires_header("authorization", "Bearer SECRET_TOKEN_CODEX")
            .requires_header("chatgpt-account-id", "ok-id")
            .delayed(Duration::from_millis(100)),
    );
    harness.server.serve(
        Provider::Codex,
        Method::Get,
        CODEX_PATH,
        Route::status(500).requires_header("chatgpt-account-id", "bad-id"),
    );
    let accounts = vec![
        codex_login("b-codex", "/home/b/.codex", "ok-id"),
        codex_login("a-codex", "/home/a/.codex", "bad-id"),
        codex_login("c-codex", "/home/c/.codex", "ok-id"),
    ];
    Ok((harness, accounts))
}

#[tokio::test]
async fn one_broken_account_does_not_hide_the_others_and_the_order_is_the_request_order()
-> TestResult {
    guard(async {
        let (harness, accounts) = three_accounts().await?;
        let report = harness.report(&accounts, Refresh::Cached).await?;
        let names: Vec<&str> = report.accounts.iter().map(|a| a.account.as_str()).collect();
        assert_eq!(names, ["b-codex", "a-codex", "c-codex"]);
        assert!(matches!(
            report.accounts[0].outcome,
            UsageOutcome::Windows(_)
        ));
        assert!(matches!(
            report.accounts[1].outcome,
            UsageOutcome::Unavailable(UnavailableReason::Provider(_))
        ));
        assert_eq!(
            report.accounts[2].outcome,
            UsageOutcome::Unavailable(UnavailableReason::NoCredentials)
        );
        assert_only_state_written(
            &harness,
            &[
                ("/home/a/.codex/auth.json", AUTH),
                ("/home/b/.codex/auth.json", AUTH),
            ],
        )
        .await
    })
    .await
}

#[tokio::test]
async fn after_the_server_is_gone_network_accounts_fail_and_credential_problems_stay() -> TestResult
{
    guard(async {
        let (harness, accounts) = three_accounts().await?;
        harness.server.shutdown().await;
        let report = harness.report(&accounts, Refresh::Cached).await?;
        assert!(matches!(
            report.accounts[0].outcome,
            UsageOutcome::Unavailable(UnavailableReason::Network(_))
        ));
        assert!(matches!(
            report.accounts[1].outcome,
            UsageOutcome::Unavailable(UnavailableReason::Network(_))
        ));
        assert_eq!(
            report.accounts[2].outcome,
            UsageOutcome::Unavailable(UnavailableReason::NoCredentials)
        );
        Ok(())
    })
    .await
}
