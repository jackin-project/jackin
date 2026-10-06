// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Top-level setup orchestration: state dir, container init, and hooks.

use super::{
    CAPSULE_RUNTIME_BIN, cache_dco_identity_if_needed, coauthor_trailer_for_agent,
    ensure_git_config_multivalue, ensure_message_trailer, env_is_one, gh_auth_status_ok,
    git_config_value, git_trailer_hook_ready, is_executable, nonempty_env,
    read_cached_dco_identity, remove_file_if_exists, run_agent_setup, run_command,
};

use std::fs;

use anyhow::{Context, Result, bail};
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use jackin_core::container_paths;

/// Resolve the mutable setup root supplied by the daemon for this PTY.
/// Reject every path shape except `/jackin/run/sessions/<numeric>/state` so a
/// compromised child cannot redirect setup writes into capsule-wide state.
pub(crate) fn session_state_dir() -> Result<PathBuf> {
    let raw = std::env::var_os(jackin_protocol::SESSION_STATE_DIR_ENV)
        .context("isolated runtime setup requires JACKIN_SESSION_STATE_DIR")?;
    let raw = raw
        .to_str()
        .context("JACKIN_SESSION_STATE_DIR must be UTF-8")?;
    parse_session_state_dir(raw)
}

pub(crate) fn parse_session_state_dir(raw: &str) -> Result<PathBuf> {
    let prefix = format!("{}/", container_paths::SESSION_ROOTS_DIR);
    let relative = raw
        .strip_prefix(&prefix)
        .context("JACKIN_SESSION_STATE_DIR is outside the session roots")?;
    let mut components = relative.split('/');
    let session_id = components.next().unwrap_or_default();
    let leaf = components.next().unwrap_or_default();
    anyhow::ensure!(
        !session_id.is_empty()
            && session_id.parse::<u64>().is_ok()
            && leaf == "state"
            && components.next().is_none(),
        "JACKIN_SESSION_STATE_DIR must name one numeric session state root"
    );
    Ok(PathBuf::from(raw))
}

pub(crate) fn session_state_path(name: &str) -> Result<PathBuf> {
    anyhow::ensure!(
        !name.is_empty() && !name.contains('/') && !name.contains('\\'),
        "runtime setup path component is invalid"
    );
    Ok(session_state_dir()?.join(name))
}

/// # Errors
///
/// Returns an error when container initialization, hook installation, or
/// agent setup fails.
pub fn run() -> Result<()> {
    run_runtime_setup_concurrently(
        run_container_init_once,
        install_git_trailer_hook_if_requested,
        cache_dco_identity_if_needed,
        run_agent_setup,
    )
}

pub(crate) fn run_runtime_setup_concurrently(
    container_init: impl FnOnce() -> Result<()> + Send + 'static,
    git_hook: impl FnOnce() -> Result<()>,
    dco_cache: impl FnOnce(),
    agent_setup: impl FnOnce() -> Result<()> + Send + 'static,
) -> Result<()> {
    let agent_setup = jackin_telemetry::spawn::thread_joined(agent_setup);
    let foreground: Result<()> = (|| {
        container_init()?;
        git_hook()?;
        dco_cache();
        Ok(())
    })();
    let agent_result = agent_setup
        .join()
        .map_err(|_| anyhow::anyhow!("runtime agent setup thread panicked"))?;
    foreground?;
    agent_result
}

/// Write a run-once marker (`ok\n`), creating its parent directory first.
pub(crate) fn write_done_marker(marker: &Path, what: &str) -> Result<()> {
    if let Some(parent) = marker.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create {what} marker directory {}",
                parent.display()
            )
        })?;
    }
    fs::write(marker, b"ok\n")
        .with_context(|| format!("failed to write {what} marker at {}", marker.display()))
}

pub(crate) fn run_container_init_once() -> Result<()> {
    let marker = session_state_path("container-init.done")?;
    if marker.exists() {
        return Ok(());
    }

    crate::output::stdout_line(format_args!("[entrypoint] running container init..."));

    if let Some(name) = nonempty_env("GIT_AUTHOR_NAME") {
        run_command("git", &["config", "--global", "user.name", &name])?;
    }
    if let Some(email) = nonempty_env("GIT_AUTHOR_EMAIL") {
        run_command("git", &["config", "--global", "user.email", &email])?;
    }

    ensure_git_config_multivalue("url.https://github.com/.insteadOf", "git@github.com:")?;
    ensure_git_config_multivalue("url.https://github.com/.insteadOf", "ssh://git@github.com/")?;

    if is_executable("/usr/bin/gh") {
        run_command(
            "git",
            &[
                "config",
                "--global",
                "credential.helper",
                "!gh auth git-credential",
            ],
        )?;
        if nonempty_env("GH_TOKEN").is_some() || gh_auth_status_ok() {
            crate::output::stdout_line(format_args!(
                "[entrypoint] GitHub CLI authenticated (host: github.com)"
            ));
            run_command("gh", &["auth", "setup-git"])?;
        } else {
            crate::output::stdout_line(format_args!(
                "[entrypoint] GitHub CLI not authenticated - run 'gh auth login' inside the runtime if needed"
            ));
        }
    } else {
        crate::output::stdout_line(format_args!(
            "[entrypoint] GitHub CLI not installed - skipping gh setup"
        ));
    }

    write_done_marker(&marker, "container init")?;
    Ok(())
}

pub(crate) fn install_git_trailer_hook_if_requested() -> Result<()> {
    if !env_is_one("JACKIN_GIT_COAUTHOR_TRAILER") && !env_is_one("JACKIN_GIT_DCO") {
        return Ok(());
    }
    if git_trailer_hook_ready() {
        return Ok(());
    }

    let hooks_dir = session_state_path("git-hooks")?;
    let hook_path = hooks_dir.join("prepare-commit-msg");
    let hook_marker = hooks_dir.join("prepare-commit-msg.v3.done");

    fs::create_dir_all(&hooks_dir)
        .with_context(|| format!("failed to create git hooks dir {}", hooks_dir.display()))?;
    if !is_executable(CAPSULE_RUNTIME_BIN) {
        bail!("git trailer hook target {CAPSULE_RUNTIME_BIN} is not executable");
    }
    remove_file_if_exists(&hook_path)?;
    symlink(CAPSULE_RUNTIME_BIN, &hook_path).with_context(|| {
        format!(
            "failed to symlink {} to {CAPSULE_RUNTIME_BIN}",
            hook_path.display()
        )
    })?;
    let hooks_dir = hooks_dir
        .to_str()
        .context("session hooks path is not UTF-8")?;
    run_command("git", &["config", "--global", "core.hooksPath", hooks_dir])?;
    fs::write(&hook_marker, b"v3\n")
        .with_context(|| format!("failed to write {}", hook_marker.display()))?;

    let mut active = Vec::new();
    if env_is_one("JACKIN_GIT_COAUTHOR_TRAILER") {
        active.push("coauthor_trailer");
    }
    if env_is_one("JACKIN_GIT_DCO") {
        active.push("dco");
    }
    let agent = std::env::var("JACKIN_AGENT").unwrap_or_else(|_| "unknown".to_owned());
    crate::output::stdout_line(format_args!(
        "[entrypoint] git trailer hook installed (agent: {agent}, active: {})",
        active.join(" ")
    ));
    Ok(())
}

/// # Errors
///
/// Returns an error when the commit-message path is missing or the hook cannot
/// read, update, or write the message.
pub fn run_prepare_commit_msg_hook(args: &[String]) -> Result<()> {
    let message_path = args
        .first()
        .map(Path::new)
        .context("prepare-commit-msg hook requires a commit message path")?;

    if env_is_one("JACKIN_GIT_DCO") {
        let (dco_name, dco_email) = read_cached_dco_identity().unwrap_or_else(|| {
            // Cache absent (e.g. daemon was started without JACKIN_GIT_DCO=1 then
            // env changed): fall back to live git config so the hook still works.
            let name = git_config_value("user.name").unwrap_or_default();
            let email = git_config_value("user.email").unwrap_or_default();
            (name, email)
        });
        if dco_name.is_empty() || dco_email.is_empty() {
            crate::output::stderr_line(format_args!(
                "[jackin prepare-commit-msg] WARNING: JACKIN_GIT_DCO=1 but git identity is not configured (user.name='{dco_name}' user.email='{dco_email}'); no Signed-off-by trailer written"
            ));
        } else {
            ensure_message_trailer(
                message_path,
                &format!("Signed-off-by: {dco_name} <{dco_email}>"),
                "Signed-off-by",
                Some("before"),
            )?;
        }
    }

    if env_is_one("JACKIN_GIT_COAUTHOR_TRAILER") {
        let agent = std::env::var("JACKIN_AGENT").unwrap_or_default();
        if let Some(trailer) = coauthor_trailer_for_agent(&agent) {
            ensure_message_trailer(message_path, trailer, "Co-authored-by", None)?;
        } else {
            crate::output::stderr_line(format_args!(
                "[jackin prepare-commit-msg] WARNING: JACKIN_GIT_COAUTHOR_TRAILER=1 but JACKIN_AGENT='{agent}' is not a recognized agent slug; no Co-authored-by trailer written"
            ));
        }
    }

    Ok(())
}
