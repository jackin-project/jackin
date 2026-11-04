// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use jackin_core::{CredentialSource, FileSystem, HostPath, Provider, StdFileSystem};
use jackin_testkit::{Method, WorkingFs};
use jackin_usage::{AccountUsage, Refresh, UnavailableReason, UsageAccount, UsageRequest};
use tokio_util::sync::CancellationToken;

use crate::providers::openrouter_serve;
use crate::support::{
    AUTH_CHATGPT, CODEX_PATH, TestResult, UsageHarness, account, codex_body, codex_login,
    codex_route, guard, unavailable, windows_of,
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
    harness.server.serve(
        Provider::Codex,
        Method::Get,
        CODEX_PATH,
        codex_route(&codex_body(42)),
    );
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
        assert_eq!(windows_of(&usage.outcome).len(), 2);
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
        assert_eq!(windows_of(&usage.outcome).len(), 2);
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
