// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::Provider;
use jackin_testkit::guard;
use jackin_testkit::{ProcessReply, ScriptedProgram};
use jackin_usage::{Refresh, UnavailableReason, UsageAccount};

use crate::providers::openrouter_serve;
use crate::support::{
    TestResult, UsageHarness, account, env_source, file_source, op_source, op_source_in,
    unavailable, windows_of,
};

const VAR: &str = "OPENROUTER_TEST_KEY";

fn router(source: jackin_core::CredentialSource) -> UsageAccount {
    account("router", Provider::Openrouter, source)
}

async fn serving(token: &str) -> Result<UsageHarness, Box<dyn std::error::Error>> {
    let harness = UsageHarness::new().await?;
    openrouter_serve(&harness, token);
    Ok(harness)
}

#[tokio::test]
async fn an_env_variable_in_the_snapshot_is_the_bearer_token() -> TestResult {
    guard(async {
        let mut harness = serving("SECRET_TOKEN_ENV").await?;
        harness.env(VAR, "SECRET_TOKEN_ENV");
        let usage = harness
            .one(&router(env_source(VAR)), Refresh::Cached)
            .await?;
        assert_eq!(windows_of(&usage.outcome).len(), 2);
        Ok(())
    })
    .await
}

async fn env_without_credential(value: Option<&str>) -> TestResult {
    let mut harness = serving("SECRET_TOKEN_ENV").await?;
    if let Some(value) = value {
        harness.env(VAR, value);
    }
    let usage = harness
        .one(&router(env_source(VAR)), Refresh::Cached)
        .await?;
    assert_eq!(
        unavailable(&usage.outcome),
        &UnavailableReason::NoCredentials
    );
    Ok(())
}

#[tokio::test]
async fn an_absent_env_variable_is_no_credentials() -> TestResult {
    guard(env_without_credential(None)).await
}

#[tokio::test]
async fn an_empty_env_variable_is_no_credentials() -> TestResult {
    guard(env_without_credential(Some(""))).await
}

#[tokio::test]
async fn a_whitespace_env_variable_is_no_credentials() -> TestResult {
    guard(env_without_credential(Some("  \t"))).await
}

#[tokio::test]
async fn the_process_environment_is_never_read() -> TestResult {
    guard(async {
        let harness = serving("SECRET_TOKEN_ENV").await?;
        let usage = harness
            .one(&router(env_source("HOME")), Refresh::Cached)
            .await?;
        assert_eq!(
            unavailable(&usage.outcome),
            &UnavailableReason::NoCredentials
        );
        Ok(())
    })
    .await
}

const REFERENCE: &str = "op://v/i/f";

fn script_op(harness: &UsageHarness, args: &[&str], reply: ProcessReply) {
    harness.runner.expect(ScriptedProgram::Op, args, reply);
}

#[tokio::test]
async fn a_one_password_value_is_the_bearer_token_without_its_trailing_newline() -> TestResult {
    guard(async {
        let harness = serving("SECRET_TOKEN_OP").await?;
        script_op(
            &harness,
            &["read", REFERENCE],
            ProcessReply::exited(0, b"SECRET_TOKEN_OP\n", b""),
        );
        let usage = harness
            .one(&router(op_source(REFERENCE)), Refresh::Cached)
            .await?;
        assert_eq!(windows_of(&usage.outcome).len(), 2);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_one_password_account_is_read_through_that_account_and_another_script_is_not_used()
-> TestResult {
    guard(async {
        let harness = serving("SECRET_TOKEN_OP").await?;
        script_op(
            &harness,
            &["read", "--account", "other-account", REFERENCE],
            ProcessReply::exited(0, b"WRONG_TOKEN\n", b""),
        );
        script_op(
            &harness,
            &["read", "--account", "work-account", REFERENCE],
            ProcessReply::exited(0, b"SECRET_TOKEN_OP\n", b""),
        );
        let usage = harness
            .one(
                &router(op_source_in(REFERENCE, "work-account")),
                Refresh::Cached,
            )
            .await?;
        assert_eq!(windows_of(&usage.outcome).len(), 2);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_one_password_account_is_not_read_by_the_script_of_the_default_account() -> TestResult {
    guard(async {
        let harness = serving("SECRET_TOKEN_OP").await?;
        script_op(
            &harness,
            &["read", REFERENCE],
            ProcessReply::exited(0, b"SECRET_TOKEN_OP\n", b""),
        );
        let usage = harness
            .one(
                &router(op_source_in(REFERENCE, "work-account")),
                Refresh::Cached,
            )
            .await?;
        assert_eq!(
            unavailable(&usage.outcome),
            &UnavailableReason::NoCredentials
        );
        Ok(())
    })
    .await
}

async fn op_without_credential(
    args: &[&str],
    reply: ProcessReply,
) -> Result<UsageHarness, Box<dyn std::error::Error>> {
    let harness = serving("SECRET_TOKEN_OP").await?;
    script_op(&harness, args, reply);
    let usage = harness
        .one(&router(op_source(REFERENCE)), Refresh::Cached)
        .await?;
    assert_eq!(
        unavailable(&usage.outcome),
        &UnavailableReason::NoCredentials
    );
    Ok(harness)
}

#[tokio::test]
async fn an_op_script_for_another_reference_is_no_credentials() -> TestResult {
    guard(async {
        op_without_credential(
            &["read", "op://other/item/field"],
            ProcessReply::exited(0, b"SECRET_TOKEN_OP\n", b""),
        )
        .await?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_failing_op_is_no_credentials_and_its_stderr_stays_out_of_the_report() -> TestResult {
    guard(async {
        let stderr = b"[ERROR] item \"SECRET_TOKEN_OP-LEAK\" not found";
        let harness = op_without_credential(
            &["read", REFERENCE],
            ProcessReply::exited(1, b"SECRET_TOKEN_OP\n", stderr),
        )
        .await?;
        let report = harness
            .report(&[router(op_source(REFERENCE))], Refresh::Force)
            .await?;
        assert!(!serde_json::to_string(&report)?.contains("SECRET_TOKEN_OP-LEAK"));
        assert!(!format!("{report:?}").contains("SECRET_TOKEN_OP-LEAK"));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_op_timeout_is_no_credentials() -> TestResult {
    guard(async {
        op_without_credential(&["read", REFERENCE], ProcessReply::timeout()).await?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_op_that_cannot_start_is_no_credentials() -> TestResult {
    guard(async {
        op_without_credential(&["read", REFERENCE], ProcessReply::spawn("no op")).await?;
        Ok(())
    })
    .await
}

const KEY_FILE: &str = "/keys/openrouter";

#[tokio::test]
async fn a_bare_token_file_is_the_bearer_token() -> TestResult {
    guard(async {
        let harness = serving("SECRET_TOKEN_FILE").await?;
        harness.put(KEY_FILE, "SECRET_TOKEN_FILE\n").await?;
        let usage = harness
            .one(&router(file_source(KEY_FILE)), Refresh::Cached)
            .await?;
        assert_eq!(windows_of(&usage.outcome).len(), 2);
        Ok(())
    })
    .await
}

async fn file_without_credential(text: Option<&str>) -> TestResult {
    let harness = serving("SECRET_TOKEN_FILE").await?;
    if let Some(text) = text {
        harness.put(KEY_FILE, text).await?;
    }
    let usage = harness
        .one(&router(file_source(KEY_FILE)), Refresh::Cached)
        .await?;
    assert_eq!(
        unavailable(&usage.outcome),
        &UnavailableReason::NoCredentials
    );
    Ok(())
}

#[tokio::test]
async fn a_missing_token_file_is_no_credentials() -> TestResult {
    guard(file_without_credential(None)).await
}

#[tokio::test]
async fn an_empty_token_file_is_no_credentials() -> TestResult {
    guard(file_without_credential(Some(""))).await
}

#[tokio::test]
async fn a_whitespace_token_file_is_no_credentials() -> TestResult {
    guard(file_without_credential(Some(" \n"))).await
}

#[tokio::test]
async fn a_login_json_file_is_a_provider_error_for_a_provider_without_login_json() -> TestResult {
    guard(async {
        let harness = serving("SECRET_TOKEN_FILE").await?;
        harness
            .put(KEY_FILE, "{\"access_token\":\"SECRET_TOKEN_FILE\"}")
            .await?;
        let usage = harness
            .one(&router(file_source(KEY_FILE)), Refresh::Cached)
            .await?;
        assert!(matches!(
            unavailable(&usage.outcome),
            UnavailableReason::Provider(_)
        ));
        Ok(())
    })
    .await
}
