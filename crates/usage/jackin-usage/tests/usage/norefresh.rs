// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::Provider;
use jackin_testkit::guard;
use jackin_testkit::{Method, Route};
use jackin_usage::{Refresh, UnavailableReason};

use crate::support::{
    CODEX_PATH, TestResult, UsageHarness, assert_only_state_written, codex_body, codex_login,
    unavailable, windows_of,
};

const AUTH_PATH: &str = "/home/work/.codex/auth.json";
const AUTH: &str = include_str!("fixtures/codex/auth_chatgpt.json");

async fn setup(accepted_token: &str) -> Result<UsageHarness, Box<dyn std::error::Error>> {
    let harness = UsageHarness::new().await?;
    harness.put(AUTH_PATH, AUTH).await?;
    harness.server.serve(
        Provider::Codex,
        Method::Get,
        CODEX_PATH,
        Route::json(200, &codex_body(42))
            .requires_header("authorization", &format!("Bearer {accepted_token}")),
    );
    harness.server.serve(
        Provider::Codex,
        Method::Post,
        "/oauth/token",
        Route::json(200, "{\"access_token\":\"SECRET_TOKEN_CODEX_NEW\"}"),
    );
    Ok(harness)
}

#[tokio::test]
async fn a_rejected_login_is_reported_and_never_refreshed() -> TestResult {
    guard(async {
        let harness = setup("SECRET_TOKEN_CODEX_NEW").await?;
        let usage = harness
            .one(
                &codex_login("work", "/home/work/.codex", "acct-1"),
                Refresh::Force,
            )
            .await?;
        assert_eq!(
            unavailable(&usage.outcome),
            &UnavailableReason::ExpiredLogin
        );
        assert_only_state_written(&harness, &[(AUTH_PATH, AUTH)]).await
    })
    .await
}

#[tokio::test]
async fn the_same_setup_with_a_valid_access_token_has_windows() -> TestResult {
    guard(async {
        let harness = setup("SECRET_TOKEN_CODEX").await?;
        let usage = harness
            .one(
                &codex_login("work", "/home/work/.codex", "acct-1"),
                Refresh::Force,
            )
            .await?;
        assert_eq!(windows_of(&usage.outcome).len(), 1);
        Ok(())
    })
    .await
}
