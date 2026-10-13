// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Git trailer hook and commit-message helpers.

use super::{
    CAPSULE_RUNTIME_BIN, env_is_one, is_executable, run_command, runtime_setup_output,
    session_state_path,
};
use std::ffi::OsString;
use std::fs;
use std::io;

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

#[cfg(debug_assertions)]
pub(crate) const GIT_DCO_IDENTITY_CACHE_ENV: &str = "JACKIN_GIT_DCO_IDENTITY_CACHE";

pub(crate) fn git_trailer_hook_ready() -> bool {
    let Ok(hooks_dir) = session_state_path("git-hooks") else {
        return false;
    };
    let hook_path = hooks_dir.join("prepare-commit-msg");
    let hook_marker = hooks_dir.join("prepare-commit-msg.v3.done");
    if !is_executable(&hook_path) || !hook_points_to_capsule(&hook_path) || !hook_marker.exists() {
        return false;
    }
    let Ok(output) = runtime_setup_output("git", ["config", "--global", "core.hooksPath"]) else {
        return false;
    };
    output.success
        && String::from_utf8_lossy(&output.stdout).trim_end()
            == hooks_dir.to_string_lossy().as_ref()
}

pub(crate) fn hook_points_to_capsule(path: &Path) -> bool {
    fs::read_link(path).is_ok_and(|target| target == Path::new(CAPSULE_RUNTIME_BIN))
}

pub(crate) fn coauthor_trailer_for_agent(agent: &str) -> Option<&'static str> {
    match agent {
        "claude" => Some("Co-authored-by: Claude <noreply@anthropic.com>"),
        "codex" => Some("Co-authored-by: Codex <codex@openai.com>"),
        "amp" => Some("Co-authored-by: Amp <amp@ampcode.com>"),
        "opencode" => Some(
            "Co-authored-by: opencode-agent[bot] <opencode-agent[bot]@users.noreply.github.com>",
        ),
        // Grok does not support trailers.
        "grok" => None,
        _ => None,
    }
}

/// Write `user.name` and `user.email` to `GIT_DCO_IDENTITY_CACHE` at startup
/// so the prepare-commit-msg hook never shells out to `git config` at commit
/// time (eliminates the class of transient-empty-config silent-skip failures).
pub(crate) fn cache_dco_identity_if_needed() {
    if !env_is_one("JACKIN_GIT_DCO") {
        return;
    }
    let (Some(name), Some(email)) = (
        git_config_value("user.name"),
        git_config_value("user.email"),
    ) else {
        // DCO is on but git identity is unreadable at startup; the commit-time
        // hook will fall back to live `git config` (and warn) per commit.
        record_recovered_degradation();
        return;
    };
    let Ok(cache_path) = git_dco_identity_cache_path() else {
        record_recovered_degradation();
        return;
    };
    if let Err(_error) = fs::write(&cache_path, format!("{name}\n{email}\n")) {
        // A failed cache write means every commit shells out to live git
        // config — the exact failure this cache exists to prevent.
        record_recovered_degradation();
    }
}

pub(crate) fn record_recovered_degradation() {
    let _warning = jackin_telemetry::record_recovered_degradation();
}

pub(crate) fn read_cached_dco_identity() -> Option<(String, String)> {
    let Ok(cache_path) = git_dco_identity_cache_path() else {
        record_recovered_degradation();
        return None;
    };
    let content = match fs::read_to_string(cache_path) {
        Ok(content) => content,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return None,
        Err(_error) => {
            record_recovered_degradation();
            return None;
        }
    };
    let mut lines = content.lines();
    let name = lines.next().filter(|s| !s.is_empty())?.to_owned();
    let email = lines.next().filter(|s| !s.is_empty())?.to_owned();
    Some((name, email))
}

pub(crate) fn git_dco_identity_cache_path() -> Result<PathBuf> {
    #[cfg(debug_assertions)]
    if let Some(path) = std::env::var_os(GIT_DCO_IDENTITY_CACHE_ENV) {
        return Ok(PathBuf::from(path));
    }
    session_state_path("git-dco-identity")
}

pub(crate) fn git_config_value(key: &str) -> Option<String> {
    let output = runtime_setup_output("git", ["config", key]).ok()?;
    if !output.success {
        return None;
    }
    Some(
        String::from_utf8_lossy(&output.stdout)
            .trim_end()
            .to_owned(),
    )
    .filter(|value| !value.is_empty())
}

pub(crate) fn ensure_message_trailer(
    message_path: &Path,
    trailer: &str,
    label: &str,
    where_arg: Option<&str>,
) -> Result<()> {
    remove_exact_trailer_lines(message_path, trailer, label)?;
    let mut args = vec![
        OsString::from("interpret-trailers"),
        OsString::from("--in-place"),
        OsString::from("--if-exists=addIfDifferent"),
    ];
    if let Some(where_arg) = where_arg {
        args.push(OsString::from(format!("--where={where_arg}")));
    }
    args.extend([OsString::from("--trailer"), OsString::from(trailer)]);
    args.push(message_path.as_os_str().to_owned());
    let output = runtime_setup_output("git", args)
        .with_context(|| format!("failed to run git interpret-trailers for {label}"))?;
    if output.success {
        return Ok(());
    }
    bail!(
        "failed to append {label} trailer to {}: {}",
        message_path.display(),
        String::from_utf8_lossy(&output.stderr).trim()
    );
}

pub(crate) fn remove_exact_trailer_lines(
    message_path: &Path,
    trailer: &str,
    label: &str,
) -> Result<()> {
    let input = fs::read(message_path)
        .with_context(|| format!("failed to read commit message {}", message_path.display()))?;
    let trailer = trailer.as_bytes();
    let mut output = Vec::with_capacity(input.len());
    for segment in input.split_inclusive(|byte| *byte == b'\n') {
        let mut line = segment;
        if let Some(stripped) = line.strip_suffix(b"\n") {
            line = stripped;
        }
        if let Some(stripped) = line.strip_suffix(b"\r") {
            line = stripped;
        }
        if line != trailer {
            output.extend_from_slice(segment);
        }
    }
    fs::write(message_path, output).with_context(|| {
        format!(
            "failed to normalize existing {label} trailer in {}",
            message_path.display()
        )
    })
}

pub(crate) fn ensure_git_config_multivalue(key: &str, value: &str) -> Result<()> {
    let output = runtime_setup_output("git", ["config", "--global", "--get-all", key])
        .with_context(|| format!("failed to read git config {key}"))?;
    if output.success
        && String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| line == value)
    {
        return Ok(());
    }
    if !output.success && output.code != Some(1) {
        bail!(
            "git config --global --get-all {key} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    run_command("git", &["config", "--global", "--add", key, value])
}
