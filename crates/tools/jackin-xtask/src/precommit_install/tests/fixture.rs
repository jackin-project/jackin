use std::{
    path::Path,
    process::{Command, Output, Stdio},
};

use anyhow::{Context, Result, ensure};

pub(super) fn private_dir() -> Result<tempfile::TempDir> {
    let home = std::env::var_os("HOME").context("HOME is required by custody tests")?;
    Ok(tempfile::Builder::new()
        .prefix("jackin-owner-install-")
        .tempdir_in(home)?)
}

pub(super) fn git(repository: &Path, args: &[&str]) -> Result<Output> {
    let output = Command::new("git")
        .args([
            "-C",
            repository.to_str().context("fixture path is not UTF-8")?,
        ])
        .args(args)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?
        .wait_with_output()?;
    ensure!(
        output.status.success(),
        "git fixture command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output)
}

pub(super) fn init_repository(root: &Path) -> Result<()> {
    std::fs::create_dir_all(root)?;
    git(root, &["init", "-q"])?;
    git(root, &["config", "user.name", "Installer Fixture"])?;
    git(
        root,
        &["config", "user.email", "installer-fixture@example.invalid"],
    )?;
    git(root, &["config", "extensions.worktreeConfig", "true"])?;
    std::fs::write(root.join("README.md"), "fixture\n")?;
    git(root, &["add", "README.md"])?;
    git(root, &["commit", "-q", "-m", "fixture"])?;
    Ok(())
}
