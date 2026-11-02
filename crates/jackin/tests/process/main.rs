// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::process::{Output, Stdio};
use std::time::Duration;

use jackin_testkit::{RoleFixture, TestResult, guard_within};
use tempfile::TempDir;
use tokio::process::Command;

const LIMIT: Duration = Duration::from_secs(30);

async fn jackin(
    home: &Path,
    cwd: &Path,
    args: &[&str],
) -> Result<Output, Box<dyn std::error::Error>> {
    let child = Command::new(env!("CARGO_BIN_EXE_jackin"))
        .args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", home)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    Ok(child.wait_with_output().await?)
}

fn code(output: &Output) -> Option<i32> {
    output.status.code()
}

#[tokio::test]
async fn the_binary_prints_help_without_a_terminal_and_maps_a_usage_error_to_code_two() -> TestResult
{
    guard_within(LIMIT, async {
        let home = TempDir::new()?;
        let bare = jackin(home.path(), home.path(), &[]).await?;
        assert_eq!(code(&bare), Some(0));
        assert!(String::from_utf8_lossy(&bare.stdout).contains("Usage"));

        let bogus = jackin(home.path(), home.path(), &["bogus"]).await?;
        assert_eq!(code(&bogus), Some(2));
        assert!(bogus.stdout.is_empty());
        assert!(!bogus.stderr.is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn role_check_through_the_binary_passes_a_valid_role_and_fails_an_invalid_one() -> TestResult
{
    guard_within(LIMIT, async {
        let home = TempDir::new()?;
        let valid = RoleFixture::new("sentinel")?;
        valid.commit().await?;
        let ok = jackin(home.path(), valid.root().as_path(), &["role", "check", "."]).await?;
        assert_eq!(code(&ok), Some(0), "{}", String::from_utf8_lossy(&ok.stderr));

        let broken = RoleFixture::new("sentinel")?.manifest_text(
            "schema = 1\nmystery_field = 2\n\n[role]\nname = \"sentinel\"\ndockerfile = \"Dockerfile\"\npolicy = \"policy.yaml\"\n".to_owned(),
        );
        broken.commit().await?;
        let failed = jackin(home.path(), broken.root().as_path(), &["role", "check", "."]).await?;
        assert_eq!(code(&failed), Some(1));
        assert!(String::from_utf8_lossy(&failed.stderr).contains("mystery_field"));
        Ok(())
    })
    .await
}
