//! Runtime setup that is better expressed as deterministic Rust than
//! entrypoint shell. The shell entrypoint remains responsible for
//! sourcing role hooks and `exec`-ing the selected agent.

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

const CONTAINER_INIT_MARKER: &str = "/tmp/jackin-runtime/container-init.done";
const GIT_HOOKS_DIR: &str = "/jackin/state/git-hooks";
const GIT_HOOK_PATH: &str = "/jackin/state/git-hooks/prepare-commit-msg";
const GIT_HOOK_MARKER: &str = "/jackin/state/git-hooks/prepare-commit-msg.v1.done";

const PREPARE_COMMIT_MSG_HOOK: &str = r#"#!/bin/bash
set -euo pipefail
# Skip amend (-c/-C/--amend all pass $2=commit), squash, and merge:
# the original or consolidated message already has the trailers.
case "${2:-}" in
  commit|squash|merge) exit 0 ;;
esac

# Append $2 trailer to commit-msg file $1 unless it is already present.
# If the last non-empty line is already a trailer (Key: value), append
# directly so the block stays contiguous. Otherwise prepend a blank line
# to separate the new trailer from the body.
_append_trailer() {
    if ! grep -qF "$2" "$1"; then
        _last=$(grep -v '^[[:space:]]*$' "$1" | tail -1)
        if printf '%s' "$_last" | grep -qE '^[A-Za-z-]+: .+'; then
            printf '%s\n' "$2" >> "$1"
        else
            printf '\n%s\n' "$2" >> "$1"
        fi || {
            echo "[jackin prepare-commit-msg] ERROR: failed to append $3 to $1" >&2
            exit 1
        }
    fi
}

# Co-authored-by (agent-specific, only if JACKIN_GIT_COAUTHOR_TRAILER=1).
if [ "${JACKIN_GIT_COAUTHOR_TRAILER:-0}" = "1" ]; then
    _agent="${JACKIN_AGENT:-}"
    _coauthor_trailer=""
    if [ "$_agent" = "claude" ]; then
        _coauthor_trailer="Co-authored-by: Claude <noreply@anthropic.com>"
    elif [ "$_agent" = "codex" ]; then
        _coauthor_trailer="Co-authored-by: Codex <codex@openai.com>"
    elif [ "$_agent" = "amp" ]; then
        _coauthor_trailer="Co-authored-by: Amp <amp@ampcode.com>"
    elif [ "$_agent" = "opencode" ]; then
        _coauthor_trailer="Co-authored-by: opencode-agent[bot] <opencode-agent[bot]@users.noreply.github.com>"
    fi
    # kimi intentionally absent: no canonical GitHub App identity in AGENTS.md.
    if [ -n "$_coauthor_trailer" ]; then
        _append_trailer "$1" "$_coauthor_trailer" "Co-authored-by"
    else
        echo "[jackin prepare-commit-msg] WARNING: JACKIN_GIT_COAUTHOR_TRAILER=1 but JACKIN_AGENT='${_agent}' is not a recognized agent slug; no Co-authored-by trailer written" >&2
    fi
fi

# Signed-off-by / DCO (from git identity, only if JACKIN_GIT_DCO=1).
if [ "${JACKIN_GIT_DCO:-0}" = "1" ]; then
    _dco_name="$(git config user.name 2>/dev/null || true)"
    _dco_email="$(git config user.email 2>/dev/null || true)"
    if [ -n "$_dco_name" ] && [ -n "$_dco_email" ]; then
        _append_trailer "$1" "Signed-off-by: ${_dco_name} <${_dco_email}>" "Signed-off-by"
    else
        echo "[jackin prepare-commit-msg] WARNING: JACKIN_GIT_DCO=1 but git identity is not configured (user.name='${_dco_name}' user.email='${_dco_email}'); no Signed-off-by trailer written" >&2
    fi
fi
"#;

pub fn run() -> Result<()> {
    run_container_init_once()?;
    install_git_trailer_hook_if_requested()?;
    run_agent_setup()
}

fn run_container_init_once() -> Result<()> {
    let marker = Path::new(CONTAINER_INIT_MARKER);
    if marker.exists() {
        return Ok(());
    }
    if let Some(parent) = marker.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create container init marker directory {}",
                parent.display()
            )
        })?;
    }

    println!("[entrypoint] running container init...");

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
            println!("[entrypoint] GitHub CLI authenticated (host: github.com)");
            run_command("gh", &["auth", "setup-git"])?;
        } else {
            println!(
                "[entrypoint] GitHub CLI not authenticated - run 'gh auth login' inside the runtime if needed"
            );
        }
    } else {
        println!("[entrypoint] GitHub CLI not installed - skipping gh setup");
    }

    fs::write(marker, b"ok\n").with_context(|| {
        format!(
            "container init succeeded but marker write failed at {}",
            marker.display()
        )
    })?;
    Ok(())
}

fn install_git_trailer_hook_if_requested() -> Result<()> {
    if !env_is_one("JACKIN_GIT_COAUTHOR_TRAILER") && !env_is_one("JACKIN_GIT_DCO") {
        return Ok(());
    }
    if git_trailer_hook_ready() {
        return Ok(());
    }

    fs::create_dir_all(GIT_HOOKS_DIR)
        .with_context(|| format!("failed to create git hooks dir {GIT_HOOKS_DIR}"))?;
    fs::write(GIT_HOOK_PATH, PREPARE_COMMIT_MSG_HOOK)
        .with_context(|| format!("failed to write {GIT_HOOK_PATH}"))?;
    let mut permissions = fs::metadata(GIT_HOOK_PATH)
        .with_context(|| format!("failed to stat {GIT_HOOK_PATH}"))?
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(GIT_HOOK_PATH, permissions)
        .with_context(|| format!("failed to chmod +x {GIT_HOOK_PATH}"))?;
    run_command(
        "git",
        &["config", "--global", "core.hooksPath", GIT_HOOKS_DIR],
    )?;
    fs::write(GIT_HOOK_MARKER, b"v1\n")
        .with_context(|| format!("failed to write {GIT_HOOK_MARKER}"))?;

    let mut active = Vec::new();
    if env_is_one("JACKIN_GIT_COAUTHOR_TRAILER") {
        active.push("coauthor_trailer");
    }
    if env_is_one("JACKIN_GIT_DCO") {
        active.push("dco");
    }
    let agent = std::env::var("JACKIN_AGENT").unwrap_or_else(|_| "unknown".to_string());
    println!(
        "[entrypoint] git trailer hook installed (agent: {agent}, active: {})",
        active.join(" ")
    );
    Ok(())
}

fn run_agent_setup() -> Result<()> {
    let agent = std::env::var("JACKIN_AGENT").context("JACKIN_AGENT must be set")?;
    match agent.as_str() {
        "claude" => setup_claude(),
        "codex" => setup_codex(),
        "amp" => setup_amp(),
        "kimi" => setup_kimi(),
        "opencode" => setup_opencode(),
        other => bail!("unknown JACKIN_AGENT: {other}"),
    }
}

fn setup_claude() -> Result<()> {
    seed_home_dir("/jackin/default-home/.claude", "/home/agent/.claude")?;
    if Path::new("/jackin/claude/account.json").is_file() {
        copy_file_with_mode(
            "/jackin/claude/account.json",
            "/home/agent/.claude.json",
            0o600,
        )?;
    }
    if Path::new("/jackin/claude/credentials.json").is_file() {
        copy_file_with_mode(
            "/jackin/claude/credentials.json",
            "/home/agent/.claude/.credentials.json",
            0o600,
        )?;
    } else {
        remove_file_if_exists("/home/agent/.claude/.credentials.json")?;
    }

    if !env_is_one("JACKIN_DISABLE_TIRITH") {
        run_optional_command(
            "claude",
            &["mcp", "add", "tirith", "--", "tirith", "mcp-server"],
        );
    } else {
        println!("[entrypoint] tirith disabled (JACKIN_DISABLE_TIRITH=1)");
    }
    if !env_is_one("JACKIN_DISABLE_SHELLFIRM") {
        run_optional_command(
            "claude",
            &["mcp", "add", "shellfirm", "--", "shellfirm", "mcp"],
        );
    } else {
        println!("[entrypoint] shellfirm disabled (JACKIN_DISABLE_SHELLFIRM=1)");
    }
    Ok(())
}

fn setup_codex() -> Result<()> {
    seed_home_dir("/jackin/default-home/.codex", "/home/agent/.codex")?;
    if Path::new("/jackin/codex/auth.json").is_file() {
        copy_file_with_mode(
            "/jackin/codex/auth.json",
            "/home/agent/.codex/auth.json",
            0o600,
        )?;
    } else {
        remove_file_if_exists("/home/agent/.codex/auth.json")?;
    }
    Ok(())
}

fn setup_amp() -> Result<()> {
    seed_home_dir(
        "/jackin/default-home/.local/share/amp",
        "/home/agent/.local/share/amp",
    )?;
    if Path::new("/jackin/amp/secrets.json").is_file() {
        eprintln!("[entrypoint] amp: forwarding host secrets.json into ~/.local/share/amp/");
        copy_file_with_mode(
            "/jackin/amp/secrets.json",
            "/home/agent/.local/share/amp/secrets.json",
            0o600,
        )?;
    } else if nonempty_env("AMP_API_KEY").is_some() {
        eprintln!("[entrypoint] amp: AMP_API_KEY present in env; agent will use api-key auth");
    } else {
        remove_file_if_exists("/home/agent/.local/share/amp/secrets.json")?;
        eprintln!(
            "[entrypoint] amp: no secrets.json mounted and AMP_API_KEY unset - agent will require interactive login"
        );
    }
    Ok(())
}

fn setup_kimi() -> Result<()> {
    seed_home_dir("/jackin/default-home/.kimi", "/home/agent/.kimi")?;
    let kimi_src = Path::new("/jackin/kimi");
    if kimi_src.is_dir() && dir_nonempty(kimi_src)? {
        eprintln!("[entrypoint] kimi: copying provisioned credentials into ~/.kimi/");
        copy_dir_contents(
            kimi_src,
            Path::new("/home/agent/.kimi"),
            CopyMode::Overwrite,
        )?;
    } else if kimi_src.is_dir() {
        eprintln!(
            "[entrypoint] kimi: sync mode active but host ~/.kimi was absent at provision time - Kimi will start without forwarded auth"
        );
    } else if nonempty_env("KIMI_API_KEY").is_some() {
        eprintln!("[entrypoint] kimi: KIMI_API_KEY present in env; agent will use api-key auth");
    } else {
        eprintln!(
            "[entrypoint] kimi: KIMI_API_KEY unset - agent will require interactive login or config"
        );
    }
    Ok(())
}

fn setup_opencode() -> Result<()> {
    seed_home_dir(
        "/jackin/default-home/.local/share/opencode",
        "/home/agent/.local/share/opencode",
    )?;
    if Path::new("/jackin/opencode/auth.json").is_file() {
        eprintln!("[entrypoint] opencode: forwarding host auth.json into ~/.local/share/opencode/");
        copy_file_with_mode(
            "/jackin/opencode/auth.json",
            "/home/agent/.local/share/opencode/auth.json",
            0o600,
        )?;
    } else if nonempty_env("OPENCODE_API_KEY").is_some() {
        eprintln!(
            "[entrypoint] opencode: OPENCODE_API_KEY present in env; agent will use api-key auth"
        );
    } else {
        remove_file_if_exists("/home/agent/.local/share/opencode/auth.json")?;
        eprintln!(
            "[entrypoint] opencode: no auth.json mounted and OPENCODE_API_KEY unset - agent will require interactive login"
        );
    }
    fs::create_dir_all("/home/agent/.config/opencode")
        .context("failed to create /home/agent/.config/opencode")?;
    let config = Path::new("/home/agent/.config/opencode/opencode.json");
    if !config.exists() {
        fs::write(config, b"{\"permission\":\"allow\"}\n")
            .context("failed to write default opencode.json")?;
    }
    Ok(())
}

fn seed_home_dir(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> Result<()> {
    let src = src.as_ref();
    let dst = dst.as_ref();
    fs::create_dir_all(dst).with_context(|| format!("failed to create {}", dst.display()))?;
    if src.is_dir() {
        copy_dir_contents(src, dst, CopyMode::SkipExisting)?;
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum CopyMode {
    SkipExisting,
    Overwrite,
}

fn copy_dir_contents(src: &Path, dst: &Path, mode: CopyMode) -> Result<()> {
    fs::create_dir_all(dst).with_context(|| format!("failed to create {}", dst.display()))?;
    for entry in fs::read_dir(src).with_context(|| format!("failed to read {}", src.display()))? {
        let entry = entry?;
        let entry_src = entry.path();
        let entry_dst = dst.join(entry.file_name());
        let metadata = entry
            .metadata()
            .with_context(|| format!("failed to stat {}", entry_src.display()))?;
        if metadata.is_dir() {
            copy_dir_contents(&entry_src, &entry_dst, mode)?;
        } else if matches!(mode, CopyMode::Overwrite) || !entry_dst.exists() {
            copy_file_preserving_mode(&entry_src, &entry_dst)?;
        }
    }
    Ok(())
}

fn copy_file_with_mode(src: impl AsRef<Path>, dst: impl AsRef<Path>, mode: u32) -> Result<()> {
    copy_file_preserving_mode(src.as_ref(), dst.as_ref())?;
    let mut permissions = fs::metadata(dst.as_ref())
        .with_context(|| format!("failed to stat {}", dst.as_ref().display()))?
        .permissions();
    permissions.set_mode(mode);
    fs::set_permissions(dst.as_ref(), permissions)
        .with_context(|| format!("failed to chmod {}", dst.as_ref().display()))
}

fn copy_file_preserving_mode(src: &Path, dst: &Path) -> Result<()> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::copy(src, dst)
        .with_context(|| format!("failed to copy {} to {}", src.display(), dst.display()))?;
    Ok(())
}

fn remove_file_if_exists(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err).with_context(|| format!("failed to remove {}", path.display())),
    }
}

fn dir_nonempty(path: &Path) -> Result<bool> {
    Ok(fs::read_dir(path)
        .with_context(|| format!("failed to read {}", path.display()))?
        .next()
        .transpose()?
        .is_some())
}

fn git_trailer_hook_ready() -> bool {
    if !is_executable(GIT_HOOK_PATH) || !Path::new(GIT_HOOK_MARKER).exists() {
        return false;
    }
    let Ok(output) = Command::new("git")
        .args(["config", "--global", "core.hooksPath"])
        .output()
    else {
        return false;
    };
    output.status.success() && String::from_utf8_lossy(&output.stdout).trim_end() == GIT_HOOKS_DIR
}

fn ensure_git_config_multivalue(key: &str, value: &str) -> Result<()> {
    let output = Command::new("git")
        .args(["config", "--global", "--get-all", key])
        .output()
        .with_context(|| format!("failed to read git config {key}"))?;
    if output.status.success()
        && String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| line == value)
    {
        return Ok(());
    }
    if !output.status.success() && output.status.code() != Some(1) {
        bail!(
            "git config --global --get-all {key} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    run_command("git", &["config", "--global", "--add", key, value])
}

fn gh_auth_status_ok() -> bool {
    Command::new("gh")
        .args(["auth", "status"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn run_command(program: &str, args: &[&str]) -> Result<()> {
    let output = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("failed to run {}", format_command(program, args)))?;
    if output.status.success() {
        return Ok(());
    }
    bail!(
        "{} failed with {}: {}",
        format_command(program, args),
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    )
}

fn run_optional_command(program: &str, args: &[&str]) {
    let mut command = Command::new(program);
    command.args(args);
    if !env_is_one("JACKIN_DEBUG") {
        command.stdout(Stdio::null()).stderr(Stdio::null());
    }
    let _ = command.status();
}

fn format_command(program: &str, args: &[&str]) -> String {
    std::iter::once(program)
        .chain(args.iter().copied())
        .collect::<Vec<_>>()
        .join(" ")
}

fn nonempty_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

fn env_is_one(name: &str) -> bool {
    std::env::var(name).as_deref() == Ok("1")
}

fn is_executable(path: impl AsRef<Path>) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_init_marker_is_container_local() {
        assert_eq!(
            CONTAINER_INIT_MARKER,
            "/tmp/jackin-runtime/container-init.done"
        );
    }

    #[test]
    fn git_hook_marker_is_versioned() {
        assert_eq!(
            GIT_HOOK_MARKER,
            "/jackin/state/git-hooks/prepare-commit-msg.v1.done"
        );
    }

    #[test]
    fn hook_uses_canonical_agent_emails() {
        assert!(PREPARE_COMMIT_MSG_HOOK.contains("noreply@anthropic.com"));
        assert!(PREPARE_COMMIT_MSG_HOOK.contains("codex@openai.com"));
        assert!(PREPARE_COMMIT_MSG_HOOK.contains("amp@ampcode.com"));
        assert!(PREPARE_COMMIT_MSG_HOOK.contains("opencode-agent[bot]@users.noreply.github.com"));
    }

    #[test]
    fn hook_injects_dco_signed_off_by() {
        assert!(PREPARE_COMMIT_MSG_HOOK.contains("JACKIN_GIT_DCO"));
        assert!(PREPARE_COMMIT_MSG_HOOK.contains("Signed-off-by:"));
        assert!(PREPARE_COMMIT_MSG_HOOK.contains("git config user.name"));
        assert!(PREPARE_COMMIT_MSG_HOOK.contains("git config user.email"));
    }
}
