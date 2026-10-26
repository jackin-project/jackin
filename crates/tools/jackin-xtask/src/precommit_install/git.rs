// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::{
    ffi::{OsStr, OsString},
    fs,
    os::unix::ffi::OsStringExt,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use jackin_process::{ExecRequest, StdioMode, exec_sync};

const OUTPUT_LIMIT: usize = 64 * 1024;

pub(super) fn repository_root() -> Result<PathBuf> {
    let bytes = git_output_bytes(None, &["rev-parse", "--show-toplevel"])?;
    let mut path = path_record(bytes, false)?;
    let canonical = fs::canonicalize(&path).context("resolving physical repository root")?;
    ensure!(canonical == path, "repository root is not physical");
    path = canonical;
    Ok(path)
}

pub(super) fn installation_paths(repository: &Path) -> Result<(PathBuf, PathBuf, PathBuf)> {
    let git_dir = git_path(
        repository,
        &["rev-parse", "--path-format=absolute", "--git-dir"],
    )?;
    let common_config = git_path(
        repository,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "config",
        ],
    )?;
    let selected_config = git_path(
        repository,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "config.worktree",
        ],
    )?;
    let physical_git_dir =
        fs::canonicalize(&git_dir).context("resolving worktree Git directory")?;
    ensure!(
        physical_git_dir == git_dir,
        "worktree Git directory is not physical"
    );
    Ok((physical_git_dir, common_config, selected_config))
}

pub(super) fn require_worktree_config(repository: &Path) -> Result<()> {
    let bytes = git_output_bytes(
        Some(repository),
        &[
            "config",
            "--local",
            "--bool",
            "--get",
            "extensions.worktreeConfig",
        ],
    )?;
    let value = text_record(&bytes)?;
    ensure!(
        value == "true",
        "extensions.worktreeConfig must be enabled before installing hooks"
    );
    Ok(())
}

pub(super) fn require_named_hook_support(repository: &Path) -> Result<()> {
    for enabled in ["true", "false"] {
        let result = git_result(
            Some(repository),
            [
                OsString::from("-c"),
                OsString::from("hook.jackin-capability-probe.event=jackin-capability-event"),
                OsString::from("-c"),
                OsString::from("hook.jackin-capability-probe.command=:"),
                OsString::from("-c"),
                OsString::from(format!("hook.jackin-capability-probe.enabled={enabled}")),
                OsString::from("-c"),
                OsString::from("hook.jackin-capability-event.enabled=true"),
                OsString::from("hook"),
                OsString::from("list"),
                OsString::from("--allow-unknown-hook-name"),
                OsString::from("jackin-capability-event"),
            ],
        )?;
        ensure!(
            result.success,
            "Git with config-based named-hook disabling is required"
        );
        let expected = if enabled == "true" {
            "jackin-capability-probe"
        } else {
            "disabled\tjackin-capability-probe"
        };
        ensure!(
            text_record(&result.stdout)? == expected,
            "Git does not enforce config-based named-hook disabling"
        );
    }
    Ok(())
}

pub(super) fn require_events_enabled(repository: &Path) -> Result<()> {
    for event in ["pre-commit", "pre-push"] {
        let result = hook_config_result(
            repository,
            [
                OsString::from("-c"),
                OsString::from(format!("hook.jackin-event-preflight.event={event}")),
                OsString::from("-c"),
                OsString::from("hook.jackin-event-preflight.command=:"),
                OsString::from("-c"),
                OsString::from("hook.jackin-event-preflight.enabled=true"),
                OsString::from("hook"),
                OsString::from("list"),
                OsString::from(event),
            ],
        )?;
        ensure!(
            result.success,
            "Git hook event preflight failed for {event}"
        );
        let output = multi_text_record(&result.stdout)?;
        ensure!(
            output.lines().any(|line| line == "jackin-event-preflight")
                && !output
                    .lines()
                    .any(|line| line == "event-disabled\tjackin-event-preflight"),
            "Git hook event {event} is disabled in effective configuration"
        );
    }
    Ok(())
}

pub(super) fn verify_installed_hooks(repository: &Path, owner: &Path) -> Result<()> {
    require_events_enabled(repository)?;
    let owner_command = format!("{} run-snapshot", shell_quote_path(owner)?);
    for (key, expected) in [
        ("hook.hk-pre-commit.enabled", "false"),
        ("hook.jackin-pre-commit.command", owner_command.as_str()),
        ("hook.jackin-pre-commit.event", "pre-commit"),
        ("hook.jackin-pre-commit.enabled", "true"),
        ("hook.hk-pre-push.enabled", "false"),
        (
            "hook.jackin-pre-push.command",
            "sh scripts/pre-push-security",
        ),
        ("hook.jackin-pre-push.event", "pre-push"),
        ("hook.jackin-pre-push.enabled", "true"),
    ] {
        ensure!(
            text_record(&git_output_bytes(
                Some(repository),
                &["config", "--worktree", "--get", key],
            )?)? == expected,
            "installed Git hook config has an invalid {key}"
        );
    }
    for (event, expected, rejected) in [
        ("pre-commit", "jackin-pre-commit", "hk-pre-commit"),
        ("pre-push", "jackin-pre-push", "hk-pre-push"),
    ] {
        let bytes = git_output_bytes(Some(repository), &["hook", "list", event])?;
        let list = multi_text_record(&bytes)?;
        let lines = list.lines().collect::<Vec<_>>();
        ensure!(
            lines.contains(&expected),
            "{event} does not list {expected}"
        );
        ensure!(!lines.contains(&rejected), "{event} still lists {rejected}");
    }
    Ok(())
}

pub(super) fn git_output_bytes(cwd: Option<&Path>, args: &[&str]) -> Result<Vec<u8>> {
    let args = args.iter().map(OsString::from).collect::<Vec<_>>();
    let result = git_result(cwd, args)?;
    ensure!(
        result.success,
        "Git query failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    Ok(result.stdout)
}

pub(super) fn config_set(path: &Path, key: &str, value: &str) -> Result<()> {
    let result = git_result(
        path.parent(),
        [
            OsString::from("config"),
            OsString::from("--file"),
            path.as_os_str().to_os_string(),
            OsString::from("--replace-all"),
            OsString::from(key),
            OsString::from(value),
        ],
    )?;
    ensure!(
        result.success,
        "Git config could not set {key}: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    Ok(())
}

pub(super) fn config_unset_all(path: &Path, key: &str) -> Result<()> {
    let result = git_result(
        path.parent(),
        [
            OsString::from("config"),
            OsString::from("--file"),
            path.as_os_str().to_os_string(),
            OsString::from("--unset-all"),
            OsString::from(key),
        ],
    )?;
    ensure!(
        result.success || result.code == Some(5) && result.stderr.is_empty(),
        "Git config could not remove {key}: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    Ok(())
}

pub(super) fn config_get(path: &Path, key: &str) -> Result<String> {
    let result = git_result(
        path.parent(),
        [
            OsString::from("config"),
            OsString::from("--file"),
            path.as_os_str().to_os_string(),
            OsString::from("--includes"),
            OsString::from("--get-all"),
            OsString::from(key),
        ],
    )?;
    ensure!(
        result.success,
        "Git config value {key} is missing or ambiguous"
    );
    text_record(&result.stdout).map(str::to_owned)
}

fn git_path(repository: &Path, args: &[&str]) -> Result<PathBuf> {
    path_record(git_output_bytes(Some(repository), args)?, false)
}

fn path_record(mut bytes: Vec<u8>, allow_empty: bool) -> Result<PathBuf> {
    if allow_empty && bytes.is_empty() {
        return Ok(PathBuf::new());
    }
    ensure!(
        bytes.pop() == Some(b'\n'),
        "Git path output lacks its record terminator"
    );
    ensure!(!bytes.contains(&0), "Git path output contains NUL");
    ensure!(allow_empty || !bytes.is_empty(), "Git path output is empty");
    Ok(PathBuf::from(OsString::from_vec(bytes)))
}

fn text_record(bytes: &[u8]) -> Result<&str> {
    let text = std::str::from_utf8(bytes).context("Git text output is not UTF-8")?;
    let text = text
        .strip_suffix('\n')
        .context("Git text output lacks its record terminator")?;
    ensure!(
        !text.contains('\n') && !text.contains('\r') && !text.contains('\0'),
        "Git text output is not one canonical record"
    );
    Ok(text)
}

fn multi_text_record(bytes: &[u8]) -> Result<&str> {
    let text = std::str::from_utf8(bytes).context("Git text output is not UTF-8")?;
    let text = text
        .strip_suffix('\n')
        .context("Git text output lacks its record terminator")?;
    ensure!(
        !text.contains('\r') && !text.contains('\0'),
        "Git text output contains invalid bytes"
    );
    Ok(text)
}

fn git_result(
    cwd: Option<&Path>,
    args: impl IntoIterator<Item = OsString>,
) -> Result<jackin_process::ExecResult> {
    let request = ExecRequest::new("git", args)
        .env_clear()
        .envs([
            ("PATH", std::env::var_os("PATH").unwrap_or_default()),
            ("GIT_CONFIG_NOSYSTEM", OsStr::new("1").to_os_string()),
            ("GIT_CONFIG_GLOBAL", OsString::from("/dev/null")),
            ("GIT_ATTR_NOSYSTEM", OsString::from("1")),
            ("GIT_TERMINAL_PROMPT", OsString::from("0")),
        ])
        .cwd(cwd.map_or_else(
            || std::env::current_dir().unwrap_or_default(),
            Path::to_path_buf,
        ))
        .stdout_mode(StdioMode::Capture)
        .stderr_mode(StdioMode::Capture)
        .output_limits(OUTPUT_LIMIT, OUTPUT_LIMIT);
    exec_sync(&request).context("running fixed Git hook-installation query")
}

fn hook_config_result(
    repository: &Path,
    args: impl IntoIterator<Item = OsString>,
) -> Result<jackin_process::ExecResult> {
    let request = ExecRequest::new("git", args)
        .cwd(repository.to_path_buf())
        .env_remove([
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_COMMON_DIR",
            "GIT_INDEX_FILE",
            "GIT_OBJECT_DIRECTORY",
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            "GIT_PREFIX",
            "GIT_CONFIG",
            "GIT_CONFIG_PARAMETERS",
            "GIT_CONFIG_COUNT",
        ])
        .envs([("GIT_TERMINAL_PROMPT", "0")])
        .stdout_mode(StdioMode::Capture)
        .stderr_mode(StdioMode::Capture)
        .output_limits(OUTPUT_LIMIT, OUTPUT_LIMIT);
    exec_sync(&request).context("checking effective Git hook event configuration")
}

pub(super) fn shell_quote_path(path: &Path) -> Result<String> {
    use std::os::unix::ffi::OsStrExt;

    let bytes = path.as_os_str().as_bytes();
    ensure!(
        !bytes.iter().any(|byte| matches!(byte, b'\n' | b'\r' | 0)),
        "installed owner path contains an unsupported control byte"
    );
    let text = std::str::from_utf8(bytes).context("installed owner path is not UTF-8")?;
    Ok(format!("'{}'", text.replace('\'', "'\\''")))
}
