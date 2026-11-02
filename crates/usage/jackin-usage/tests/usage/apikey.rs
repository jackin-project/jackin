// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{CredentialSource, Provider};
use jackin_testkit::{Method, Route};
use jackin_usage::{Refresh, UnavailableReason, UsageAccount};

use crate::support::{
    CODEX_HOME, CODEX_PATH, TestResult, UsageHarness, account, codex_login, env_source,
    file_source, guard, op_source, unavailable, windows_of,
};

const FULL: &str = include_str!("fixtures/codex/usage_full.json");
const REASON: &str = "OpenAI API keys have no subscription quota";
const AUTH_PATH: &str = "/home/work/.codex/auth.json";

fn unsupported() -> UnavailableReason {
    UnavailableReason::Unsupported {
        detail: Some(REASON.to_owned()),
    }
}

async fn open_server() -> Result<UsageHarness, Box<dyn std::error::Error>> {
    let harness = UsageHarness::new().await?;
    harness.server.serve(
        Provider::Codex,
        Method::Get,
        CODEX_PATH,
        Route::json(200, FULL),
    );
    Ok(harness)
}

fn keyed(source: CredentialSource) -> UsageAccount {
    account("key", Provider::Codex, source)
}

fn login() -> UsageAccount {
    codex_login("work", CODEX_HOME, "acct-1")
}

#[tokio::test]
async fn an_env_api_key_is_unsupported_while_a_login_on_the_same_server_has_windows() -> TestResult
{
    guard(async {
        let mut harness = open_server().await?;
        harness.env("OPENAI_API_KEY", "SECRET_TOKEN_APIKEY");
        let key = harness
            .one(&keyed(env_source("OPENAI_API_KEY")), Refresh::Cached)
            .await?;
        assert_eq!(unavailable(&key.outcome), &unsupported());
        harness
            .put(AUTH_PATH, include_str!("fixtures/codex/auth_chatgpt.json"))
            .await?;
        let chatgpt = harness.one(&login(), Refresh::Cached).await?;
        assert_eq!(windows_of(&chatgpt.outcome).len(), 6);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_env_account_decides_before_the_variable_is_read() -> TestResult {
    guard(async {
        let harness = open_server().await?;
        let usage = harness
            .one(&keyed(env_source("OPENAI_API_KEY")), Refresh::Cached)
            .await?;
        assert_eq!(unavailable(&usage.outcome), &unsupported());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_one_password_account_is_unsupported_and_runs_no_program() -> TestResult {
    guard(async {
        let harness = open_server().await?;
        let usage = harness
            .one(&keyed(op_source("op://vault/item/field")), Refresh::Cached)
            .await?;
        assert_eq!(unavailable(&usage.outcome), &unsupported());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_file_account_is_unsupported() -> TestResult {
    guard(async {
        let harness = open_server().await?;
        harness.put("/keys/openai", "SECRET_TOKEN_APIKEY\n").await?;
        let usage = harness
            .one(&keyed(file_source("/keys/openai")), Refresh::Cached)
            .await?;
        assert_eq!(unavailable(&usage.outcome), &unsupported());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_login_file_with_only_an_api_key_is_unsupported() -> TestResult {
    guard(async {
        let harness = open_server().await?;
        harness
            .put(
                AUTH_PATH,
                include_str!("fixtures/codex/auth_apikey_only.json"),
            )
            .await?;
        let usage = harness.one(&login(), Refresh::Cached).await?;
        assert_eq!(unavailable(&usage.outcome), &unsupported());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_login_file_with_an_api_key_and_tokens_uses_the_tokens() -> TestResult {
    guard(async {
        let harness = open_server().await?;
        harness
            .put(AUTH_PATH, include_str!("fixtures/codex/auth_both.json"))
            .await?;
        let usage = harness.one(&login(), Refresh::Cached).await?;
        assert_eq!(windows_of(&usage.outcome).len(), 6);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_missing_login_file_is_no_credentials() -> TestResult {
    guard(async {
        let harness = open_server().await?;
        let usage = harness.one(&login(), Refresh::Cached).await?;
        assert_eq!(
            unavailable(&usage.outcome),
            &UnavailableReason::NoCredentials
        );
        Ok(())
    })
    .await
}

fn provider_message(reason: &UnavailableReason) -> &str {
    match reason {
        UnavailableReason::Provider(message) => message,
        other => panic!("expected a provider reason, found {other:?}"),
    }
}

#[tokio::test]
async fn a_login_file_that_is_not_json_is_a_provider_error_without_its_text() -> TestResult {
    guard(async {
        let harness = open_server().await?;
        harness.put(AUTH_PATH, "not json SECRET_TEXT_FILE").await?;
        let usage = harness.one(&login(), Refresh::Cached).await?;
        let message = provider_message(unavailable(&usage.outcome));
        assert!(!message.contains("SECRET_TEXT_FILE"), "{message}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_login_file_with_tokens_but_no_access_token_is_a_provider_error_without_its_text()
-> TestResult {
    guard(async {
        let harness = open_server().await?;
        harness
            .put(
                AUTH_PATH,
                include_str!("fixtures/codex/auth_no_access.json"),
            )
            .await?;
        let usage = harness.one(&login(), Refresh::Cached).await?;
        let message = provider_message(unavailable(&usage.outcome));
        assert!(!message.contains("SECRET_TOKEN_CODEX_REFRESH"), "{message}");
        Ok(())
    })
    .await
}
