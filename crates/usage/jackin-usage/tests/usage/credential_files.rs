// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use jackin_core::{CredentialSource, FileSystem, HostPath, Provider, StdFileSystem};
use jackin_testkit::{Method, Route, WorkingFs};
use jackin_usage::{AccountUsage, Refresh, UnavailableReason, UsageAccount, UsageRequest};
use tokio_util::sync::CancellationToken;

use crate::providers::openrouter_serve;
use crate::support::{
    AUTH_CHATGPT, CODEX_TOKEN, TestResult, UsageHarness, account, codex_body, codex_login,
    codex_route, guard, serve_codex, unavailable, windows_of,
};

const TOKEN: &str = "SECRET_TOKEN_FILE";
const CAP: u64 = 1 << 20;

fn host_path(path: &Path) -> Result<HostPath, Box<dyn std::error::Error>> {
    Ok(HostPath::parse(path)?)
}

fn file_account(path: &Path) -> Result<UsageAccount, Box<dyn std::error::Error>> {
    Ok(account(
        "router",
        Provider::Openrouter,
        CredentialSource::File {
            path: host_path(path)?,
        },
    ))
}

async fn read_through(
    directory: &tempfile::TempDir,
    account: UsageAccount,
) -> Result<AccountUsage, Box<dyn std::error::Error>> {
    let harness = UsageHarness::new().await?;
    openrouter_serve(&harness, TOKEN);
    serve_codex(&harness, CODEX_TOKEN, codex_route(&codex_body(42)));
    let fs: Arc<dyn FileSystem> = Arc::new(WorkingFs::new(
        Arc::new(StdFileSystem::default()),
        &harness.clock,
    ));
    let host = harness.host_over(fs, host_path(&directory.path().join("state"))?);
    let request = UsageRequest {
        accounts: vec![account],
        refresh: Refresh::Force,
    };
    let mut report = host.report(&request, &CancellationToken::new()).await?;
    Ok(report.accounts.remove(0))
}

fn refusal(usage: &AccountUsage) -> &str {
    match unavailable(&usage.outcome) {
        UnavailableReason::Provider(message) => message,
        other => panic!("expected a provider refusal, got {other:?}"),
    }
}

fn padded(path: &Path, bytes: u64) -> TestResult {
    let mut text = TOKEN.to_owned();
    text.push_str(&" ".repeat(usize::try_from(bytes)? - text.len()));
    std::fs::write(path, text)?;
    Ok(())
}

#[tokio::test]
async fn a_regular_token_file_up_to_the_size_cap_is_the_bearer_token() -> TestResult {
    guard(async {
        let directory = tempfile::tempdir()?;
        let key = directory.path().join("key");
        padded(&key, CAP)?;
        let usage = read_through(&directory, file_account(&key)?).await?;
        assert_eq!(windows_of(&usage.outcome).len(), 5);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_token_file_one_byte_over_the_cap_is_refused_as_too_large() -> TestResult {
    guard(async {
        let directory = tempfile::tempdir()?;
        let key = directory.path().join("key");
        padded(&key, CAP + 1)?;
        let usage = read_through(&directory, file_account(&key)?).await?;
        assert_eq!(refusal(&usage), "the credential file is too large");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_symbolic_link_to_a_good_token_file_is_the_bearer_token() -> TestResult {
    guard(async {
        let directory = tempfile::tempdir()?;
        let real = directory.path().join("real");
        padded(&real, 64)?;
        let link = directory.path().join("key");
        symlink(&real, &link)?;
        let usage = read_through(&directory, file_account(&link)?).await?;
        assert_eq!(windows_of(&usage.outcome).len(), 5);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_symbolic_link_to_an_oversized_file_is_refused_as_too_large() -> TestResult {
    guard(async {
        let directory = tempfile::tempdir()?;
        let real = directory.path().join("real");
        padded(&real, CAP + 1)?;
        let link = directory.path().join("key");
        symlink(&real, &link)?;
        let usage = read_through(&directory, file_account(&link)?).await?;
        assert_eq!(refusal(&usage), "the credential file is too large");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_symbolic_link_to_a_pipe_is_refused_without_blocking() -> TestResult {
    guard(async {
        let directory = tempfile::tempdir()?;
        let pipe = directory.path().join("pipe");
        assert!(Command::new("mkfifo").arg(&pipe).status()?.success());
        let link = directory.path().join("key");
        symlink(&pipe, &link)?;
        let usage = read_through(&directory, file_account(&link)?).await?;
        assert_eq!(refusal(&usage), "the credential file is not a regular file");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_dangling_symbolic_link_is_no_credentials() -> TestResult {
    guard(async {
        let directory = tempfile::tempdir()?;
        let link = directory.path().join("key");
        symlink(directory.path().join("gone"), &link)?;
        let usage = read_through(&directory, file_account(&link)?).await?;
        assert_eq!(
            unavailable(&usage.outcome),
            &UnavailableReason::NoCredentials
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_directory_in_place_of_the_credential_file_is_refused() -> TestResult {
    guard(async {
        let directory = tempfile::tempdir()?;
        let key = directory.path().join("key");
        std::fs::create_dir(&key)?;
        let usage = read_through(&directory, file_account(&key)?).await?;
        assert_eq!(refusal(&usage), "the credential file is not a regular file");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_missing_credential_file_stays_no_credentials() -> TestResult {
    guard(async {
        let directory = tempfile::tempdir()?;
        let usage =
            read_through(&directory, file_account(&directory.path().join("absent"))?).await?;
        assert_eq!(
            unavailable(&usage.outcome),
            &UnavailableReason::NoCredentials
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_symbolic_link_in_place_of_a_codex_auth_json_is_the_login() -> TestResult {
    guard(async {
        let directory = tempfile::tempdir()?;
        let home = directory.path().join("codex");
        std::fs::create_dir(&home)?;
        let real = directory.path().join("real.json");
        std::fs::write(&real, AUTH_CHATGPT)?;
        symlink(&real, home.join("auth.json"))?;
        let login = codex_login("work", &home.display().to_string(), "acct-1");
        let usage = read_through(&directory, login).await?;
        assert_eq!(windows_of(&usage.outcome).len(), 1);
        Ok(())
    })
    .await
}

const KIMI_USAGES: &str = "{\"usages\":{\"limit_5h\":{\"used_ratio\":0.25}}}";
const KIMI_PATH: &str = "/coding/v1/usages";

fn working_host(harness: &UsageHarness, directory: &tempfile::TempDir) -> jackin_usage::HostUsage {
    let fs: Arc<dyn FileSystem> = Arc::new(WorkingFs::new(
        Arc::new(StdFileSystem::default()),
        &harness.clock,
    ));
    harness.host_on(
        fs,
        host_path(&directory.path().join("state").join("usage")).expect("the state path"),
        host_path(directory.path()).expect("the home path"),
    )
}

async fn kimi_report(
    harness: &UsageHarness,
    directory: &tempfile::TempDir,
) -> Result<jackin_usage::UsageOutcome, Box<dyn std::error::Error>> {
    let account = account(
        "kimi",
        Provider::Kimi,
        CredentialSource::HostLogin {
            path: host_path(&directory.path().join("kimi-home"))?,
        },
    );
    let request = UsageRequest {
        accounts: vec![account],
        refresh: Refresh::Force,
    };
    let mut report = working_host(harness, directory)
        .report(&request, &CancellationToken::new())
        .await?;
    Ok(report.accounts.remove(0).outcome)
}

fn write_kimi_store(directory: &tempfile::TempDir, token: &str) -> TestResult {
    let home = directory.path().join("kimi-home");
    std::fs::create_dir_all(home.join("credentials"))?;
    std::fs::write(
        home.join("config.toml"),
        "[providers.\"managed:kimi-code\"]\ntype = \"kimi\"\nbase_url = \"https://api.kimi.com/coding/v1\"\n\n[providers.\"managed:kimi-code\".oauth]\nstorage = \"file\"\nkey = \"oauth/slot\"\n",
    )?;
    std::fs::write(
        home.join("credentials").join("slot.json"),
        format!("{{\"access_token\":\"{token}\",\"expires_at\":1767232800}}"),
    )?;
    Ok(())
}

#[tokio::test]
async fn a_host_login_reads_the_nested_credential_file() -> TestResult {
    guard(async {
        let directory = tempfile::tempdir()?;
        write_kimi_store(&directory, "SECRET_TOKEN_KIMI")?;
        let harness = UsageHarness::new().await?;
        harness.server.replace(
            Provider::Kimi,
            Method::Get,
            KIMI_PATH,
            Route::json(200, KIMI_USAGES)
                .requires_header("authorization", "Bearer SECRET_TOKEN_KIMI"),
        );
        let outcome = kimi_report(&harness, &directory).await?;
        assert_eq!(windows_of(&outcome).len(), 1);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_host_login_whose_nested_credential_is_replaced_is_fetched_again() -> TestResult {
    guard(async {
        let directory = tempfile::tempdir()?;
        write_kimi_store(&directory, "SECRET_TOKEN_KIMI")?;
        let harness = UsageHarness::new().await?;
        harness.server.replace(
            Provider::Kimi,
            Method::Get,
            KIMI_PATH,
            Route::json(200, KIMI_USAGES)
                .requires_header("authorization", "Bearer SECRET_TOKEN_KIMI"),
        );
        let outcome = kimi_report(&harness, &directory).await?;
        assert_eq!(windows_of(&outcome).len(), 1);
        write_kimi_store(&directory, "SECRET_TOKEN_KIMI_TWO")?;
        harness
            .server
            .replace(Provider::Kimi, Method::Get, KIMI_PATH, Route::status(500));
        let outcome = kimi_report(&harness, &directory).await?;
        assert!(
            matches!(unavailable(&outcome), UnavailableReason::Provider(_)),
            "the cached windows of the replaced credential were served"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_token_rewritten_in_place_with_the_same_length_is_fetched_again() -> TestResult {
    guard(async {
        let directory = tempfile::tempdir()?;
        let key = directory.path().join("key");
        std::fs::write(&key, "SECRET_TOKEN_FIRST")?;
        let harness = UsageHarness::new().await?;
        harness.server.replace(
            Provider::Openrouter,
            Method::Get,
            "/api/v1/key",
            Route::json(
                200,
                "{\"data\":{\"limit\":50.0,\"limit_remaining\":37.66,\"usage\":12.34}}",
            )
            .requires_header("authorization", "Bearer SECRET_TOKEN_FIRST"),
        );
        let account = account(
            "router",
            Provider::Openrouter,
            CredentialSource::File {
                path: host_path(&key)?,
            },
        );
        let request = UsageRequest {
            accounts: vec![account],
            refresh: Refresh::Cached,
        };
        let host = working_host(&harness, &directory);
        let mut report = host.report(&request, &CancellationToken::new()).await?;
        assert_eq!(windows_of(&report.accounts.remove(0).outcome).len(), 2);
        std::fs::write(&key, "SECRET_TOKEN_SECON")?;
        harness.server.replace(
            Provider::Openrouter,
            Method::Get,
            "/api/v1/key",
            Route::status(500),
        );
        let mut report = host.report(&request, &CancellationToken::new()).await?;
        let outcome = report.accounts.remove(0).outcome;
        assert!(
            matches!(unavailable(&outcome), UnavailableReason::Provider(_)),
            "the cached windows of the rewritten token were served"
        );
        Ok(())
    })
    .await
}
