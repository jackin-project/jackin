//! Runtime setup that is better expressed as deterministic Rust than
//! entrypoint shell. The shell entrypoint remains responsible for
//! sourcing role hooks and `exec`-ing the selected agent.

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

const CONTAINER_INIT_MARKER: &str = "/jackin/state/container-init.done";
const CAPSULE_RUNTIME_BIN: &str = "/jackin/runtime/jackin-capsule";
const GIT_HOOKS_DIR: &str = "/jackin/state/git-hooks";
const GIT_HOOK_PATH: &str = "/jackin/state/git-hooks/prepare-commit-msg";
const GIT_HOOK_MARKER: &str = "/jackin/state/git-hooks/prepare-commit-msg.v3.done";
/// Cached DCO identity written at daemon startup so the hook never calls
/// `git config` at commit time (avoids transient-empty-config silent skips).
const GIT_DCO_IDENTITY_CACHE: &str = "/jackin/state/git-dco-identity";

pub fn run() -> Result<()> {
    run_container_init_once()?;
    install_git_trailer_hook_if_requested()?;
    cache_dco_identity_if_needed();
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
    if !is_executable(CAPSULE_RUNTIME_BIN) {
        bail!("git trailer hook target {CAPSULE_RUNTIME_BIN} is not executable");
    }
    remove_file_if_exists(GIT_HOOK_PATH)?;
    symlink(CAPSULE_RUNTIME_BIN, GIT_HOOK_PATH)
        .with_context(|| format!("failed to symlink {GIT_HOOK_PATH} to {CAPSULE_RUNTIME_BIN}"))?;
    run_command(
        "git",
        &["config", "--global", "core.hooksPath", GIT_HOOKS_DIR],
    )?;
    fs::write(GIT_HOOK_MARKER, b"v3\n")
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
            eprintln!(
                "[jackin prepare-commit-msg] WARNING: JACKIN_GIT_DCO=1 but git identity is not configured (user.name='{dco_name}' user.email='{dco_email}'); no Signed-off-by trailer written"
            );
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
            eprintln!(
                "[jackin prepare-commit-msg] WARNING: JACKIN_GIT_COAUTHOR_TRAILER=1 but JACKIN_AGENT='{agent}' is not a recognized agent slug; no Co-authored-by trailer written"
            );
        }
    }

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
    seed_home_dir("/jackin/default-home/.kimi-code", "/home/agent/.kimi-code")?;
    let kimi_src = Path::new("/jackin/kimi-code");
    if kimi_src.is_dir() && dir_nonempty(kimi_src)? {
        eprintln!("[entrypoint] kimi: copying provisioned credentials into ~/.kimi-code/");
        copy_dir_contents(
            kimi_src,
            Path::new("/home/agent/.kimi-code"),
            CopyMode::Overwrite,
        )?;
    } else if kimi_src.is_dir() {
        eprintln!(
            "[entrypoint] kimi: sync mode active but host ~/.kimi-code was absent at provision time - Kimi will start without forwarded auth"
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
    if !is_executable(GIT_HOOK_PATH)
        || !hook_points_to_capsule()
        || !Path::new(GIT_HOOK_MARKER).exists()
    {
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

fn hook_points_to_capsule() -> bool {
    fs::read_link(GIT_HOOK_PATH)
        .map(|target| target == Path::new(CAPSULE_RUNTIME_BIN))
        .unwrap_or(false)
}

fn coauthor_trailer_for_agent(agent: &str) -> Option<&'static str> {
    match agent {
        "claude" => Some("Co-authored-by: Claude <noreply@anthropic.com>"),
        "codex" => Some("Co-authored-by: Codex <codex@openai.com>"),
        "amp" => Some("Co-authored-by: Amp <amp@ampcode.com>"),
        "opencode" => Some(
            "Co-authored-by: opencode-agent[bot] <opencode-agent[bot]@users.noreply.github.com>",
        ),
        _ => None,
    }
}

/// Write `user.name` and `user.email` to `GIT_DCO_IDENTITY_CACHE` at startup
/// so the prepare-commit-msg hook never shells out to `git config` at commit
/// time (eliminates the class of transient-empty-config silent-skip failures).
fn cache_dco_identity_if_needed() {
    if !env_is_one("JACKIN_GIT_DCO") {
        return;
    }
    let (Some(name), Some(email)) = (
        git_config_value("user.name"),
        git_config_value("user.email"),
    ) else {
        // DCO is on but git identity is unreadable at startup; the commit-time
        // hook will fall back to live `git config` (and warn) per commit.
        crate::clog!("dco identity cache skipped: user.name/user.email not configured at startup");
        return;
    };
    if let Err(err) = fs::write(GIT_DCO_IDENTITY_CACHE, format!("{name}\n{email}\n")) {
        // A failed cache write means every commit shells out to live git
        // config — the exact failure this cache exists to prevent.
        crate::clog!(
            "dco identity cache write to {GIT_DCO_IDENTITY_CACHE} failed: {err} (errno={:?})",
            err.raw_os_error()
        );
    }
}

fn read_cached_dco_identity() -> Option<(String, String)> {
    let content = fs::read_to_string(GIT_DCO_IDENTITY_CACHE).ok()?;
    let mut lines = content.lines();
    let name = lines.next().filter(|s| !s.is_empty())?.to_string();
    let email = lines.next().filter(|s| !s.is_empty())?.to_string();
    Some((name, email))
}

fn git_config_value(key: &str) -> Option<String> {
    let output = Command::new("git").args(["config", key]).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(
        String::from_utf8_lossy(&output.stdout)
            .trim_end()
            .to_string(),
    )
    .filter(|value| !value.is_empty())
}

fn ensure_message_trailer(
    message_path: &Path,
    trailer: &str,
    label: &str,
    where_arg: Option<&str>,
) -> Result<()> {
    remove_exact_trailer_lines(message_path, trailer, label)?;
    let mut command = Command::new("git");
    command.args([
        "interpret-trailers",
        "--in-place",
        "--if-exists=addIfDifferent",
    ]);
    if let Some(where_arg) = where_arg {
        command.arg(format!("--where={where_arg}"));
    }
    let output = command
        .args(["--trailer", trailer])
        .arg(message_path)
        .output()
        .with_context(|| format!("failed to run git interpret-trailers for {label}"))?;
    if output.status.success() {
        return Ok(());
    }
    bail!(
        "failed to append {label} trailer to {}: {}",
        message_path.display(),
        String::from_utf8_lossy(&output.stderr).trim()
    );
}

fn remove_exact_trailer_lines(message_path: &Path, trailer: &str, label: &str) -> Result<()> {
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
    // "Optional" means "do not abort runtime_setup", not "swallow the
    // exit code." A failing `claude mcp add tirith` or `shellfirm`
    // call leaves the role launched without the MCP wired up, so log
    // the exact failure to the multiplexer log for operator triage.
    match command.status() {
        Ok(status) if status.success() => {}
        Ok(status) => {
            crate::clog!(
                "optional command {} exited with status {status}",
                format_command(program, args)
            );
        }
        Err(e) => {
            crate::clog!(
                "optional command {} failed to spawn: {e} (errno={:?})",
                format_command(program, args),
                e.raw_os_error()
            );
        }
    }
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
        assert_eq!(CONTAINER_INIT_MARKER, "/jackin/state/container-init.done");
    }

    #[test]
    fn git_hook_marker_is_versioned() {
        assert_eq!(
            GIT_HOOK_MARKER,
            "/jackin/state/git-hooks/prepare-commit-msg.v3.done"
        );
    }

    #[test]
    fn hook_uses_canonical_agent_trailers() {
        assert_eq!(
            coauthor_trailer_for_agent("claude"),
            Some("Co-authored-by: Claude <noreply@anthropic.com>")
        );
        assert_eq!(
            coauthor_trailer_for_agent("codex"),
            Some("Co-authored-by: Codex <codex@openai.com>")
        );
        assert_eq!(
            coauthor_trailer_for_agent("amp"),
            Some("Co-authored-by: Amp <amp@ampcode.com>")
        );
        assert_eq!(
            coauthor_trailer_for_agent("opencode"),
            Some(
                "Co-authored-by: opencode-agent[bot] <opencode-agent[bot]@users.noreply.github.com>"
            )
        );
        assert_eq!(coauthor_trailer_for_agent("kimi"), None);
    }

    #[test]
    fn hook_marker_points_at_capsule_runtime_binary() {
        assert_eq!(CAPSULE_RUNTIME_BIN, "/jackin/runtime/jackin-capsule");
    }
}
