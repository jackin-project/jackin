//! Runtime setup that is better expressed as deterministic Rust than
//! entrypoint shell. The shell entrypoint remains responsible for
//! sourcing role hooks and `exec`-ing the selected agent.

use std::fs;
use std::io;
use std::io::Write as _;
use std::os::unix::fs::PermissionsExt as _;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use anyhow::{Context, Result, bail};
use serde_json::json;
use tempfile::Builder as TempfileBuilder;

const CONTAINER_INIT_MARKER: &str = "/jackin/state/container-init.done";

// Container home for the `agent` user. Every default agent config/credential
// location hangs off this. The per-agent resolvers below honor an agent's
// standard config-dir env var (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`,
// `XDG_DATA_HOME`) when the role sets one, falling back here otherwise — so a
// role that exports e.g. `CLAUDE_CONFIG_DIR` has its credentials written where
// the CLI actually looks, instead of the fixed default the CLI no longer reads.
const AGENT_HOME: &str = "/home/agent";

// Grok has no standard config-dir env var, so its credential path is fixed.
const GROK_AUTH_PATH: &str = "/home/agent/.grok/auth.json";

// Each resolver pairs a thin env-reading wrapper with a pure `_from` core, so
// path composition is unit-tested without mutating process-global env (which is
// racy across parallel tests and `unsafe` under Rust 2024). Same split as
// `write_codex_provider_config` / `write_codex_provider_config_inner`.

/// Resolve Claude Code's config directory, honoring `CLAUDE_CONFIG_DIR` when the
/// role sets it (default `~/.claude`). Claude reads `.credentials.json` — and,
/// when the env var is set, `.claude.json` — from this directory.
fn claude_config_dir() -> PathBuf {
    claude_config_dir_from(nonempty_env("CLAUDE_CONFIG_DIR").as_deref())
}

fn claude_config_dir_from(env: Option<&str>) -> PathBuf {
    env.map_or_else(|| Path::new(AGENT_HOME).join(".claude"), PathBuf::from)
}

/// `.credentials.json` always lives inside the resolved Claude config dir.
fn claude_credentials_path() -> PathBuf {
    claude_config_dir().join(".credentials.json")
}

/// `.claude.json` placement is asymmetric: with `CLAUDE_CONFIG_DIR` set it lives
/// inside that dir; with it unset Claude reads `$HOME/.claude.json` (home root).
/// Writing it on the wrong side leaves the CLI unable to find its onboarding
/// state, so it falls back to the interactive login screen even though a valid
/// `.credentials.json` was forwarded.
fn claude_account_path() -> PathBuf {
    claude_account_path_from(nonempty_env("CLAUDE_CONFIG_DIR").as_deref())
}

fn claude_account_path_from(env: Option<&str>) -> PathBuf {
    Path::new(env.unwrap_or(AGENT_HOME)).join(".claude.json")
}

/// Codex reads `auth.json` and `config.toml` from `CODEX_HOME` (default `~/.codex`).
fn codex_home() -> PathBuf {
    codex_home_from(nonempty_env("CODEX_HOME").as_deref())
}

fn codex_home_from(env: Option<&str>) -> PathBuf {
    env.map_or_else(|| Path::new(AGENT_HOME).join(".codex"), PathBuf::from)
}

fn codex_auth_path() -> PathBuf {
    codex_home().join("auth.json")
}

/// XDG data root honored by Amp and opencode (default `~/.local/share`).
fn xdg_data_home() -> PathBuf {
    xdg_data_home_from(nonempty_env("XDG_DATA_HOME").as_deref())
}

fn xdg_data_home_from(env: Option<&str>) -> PathBuf {
    env.map_or_else(|| Path::new(AGENT_HOME).join(".local/share"), PathBuf::from)
}

fn amp_secrets_path() -> PathBuf {
    xdg_data_home().join("amp/secrets.json")
}

fn opencode_auth_path() -> PathBuf {
    xdg_data_home().join("opencode/auth.json")
}
const CAPSULE_RUNTIME_BIN: &str = "/jackin/runtime/jackin-capsule";
const GIT_HOOKS_DIR: &str = "/jackin/state/git-hooks";
const GIT_HOOK_PATH: &str = "/jackin/state/git-hooks/prepare-commit-msg";
const GIT_HOOK_MARKER: &str = "/jackin/state/git-hooks/prepare-commit-msg.v3.done";
/// Cached DCO identity written at daemon startup so the hook never calls
/// `git config` at commit time (avoids transient-empty-config silent skips).
const GIT_DCO_IDENTITY_CACHE: &str = "/jackin/state/git-dco-identity";
#[cfg(debug_assertions)]
const GIT_DCO_IDENTITY_CACHE_ENV: &str = "JACKIN_GIT_DCO_IDENTITY_CACHE";

pub fn run() -> Result<()> {
    run_runtime_setup_concurrently(
        run_container_init_once,
        install_git_trailer_hook_if_requested,
        cache_dco_identity_if_needed,
        run_agent_setup,
    )
}

fn run_runtime_setup_concurrently(
    container_init: impl FnOnce() -> Result<()> + Send + 'static,
    git_hook: impl FnOnce() -> Result<()>,
    dco_cache: impl FnOnce(),
    agent_setup: impl FnOnce() -> Result<()> + Send + 'static,
) -> Result<()> {
    let agent_setup = std::thread::spawn(agent_setup);
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
fn write_done_marker(marker: &Path, what: &str) -> Result<()> {
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

fn run_container_init_once() -> Result<()> {
    let marker = Path::new(CONTAINER_INIT_MARKER);
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

    write_done_marker(marker, "container init")?;
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
    let agent = std::env::var("JACKIN_AGENT").unwrap_or_else(|_| "unknown".to_owned());
    crate::output::stdout_line(format_args!(
        "[entrypoint] git trailer hook installed (agent: {agent}, active: {})",
        active.join(" ")
    ));
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

fn run_agent_setup() -> Result<()> {
    let agent = std::env::var("JACKIN_AGENT").context("JACKIN_AGENT must be set")?;
    // D6/D19: home emptiness is the gate. First seed copies auth; subsequent
    // starts leave in-container credentials untouched (agent refreshes tokens
    // in-place inside the durable home). No external marker file.
    match agent.as_str() {
        "claude" => setup_claude(),
        "codex" => setup_codex(),
        "amp" => setup_amp(),
        "kimi" => setup_kimi(),
        "opencode" => setup_opencode(),
        "grok" => setup_grok(),
        other => bail!("unknown JACKIN_AGENT: {other}"),
    }?;

    // Install/repair the agent-status reporter on every launch (drift repair).
    // Observability must never break the agent: a failure is logged, not fatal.
    if let Err(e) = install_agent_status_reporter(&agent) {
        crate::clog!("agent-status: reporter install for {agent} failed (non-fatal): {e:#}");
    }

    Ok(())
}

/// Install the container-local agent-status reporter for `agent` into the agent
/// home, repairing drift each launch. Claude/Codex install too — their forwarded
/// events are gated to identity/freshness only (Decision 0a), the screen pack
/// owns their state. Kimi is rule-pack-only (no reporter); Amp installs no
/// reporter (it has no plugins.json node-plugin mechanism — writing one crashes
/// it; a real Amp reporter needs its MCP/toolbox surface); unknown/grok have no
/// reporter yet.
fn install_agent_status_reporter(agent: &str) -> Result<()> {
    use crate::agent_status::hook_installer::{
        ClaudeHookInstaller, CodexHookInstaller, HookInstaller, PluginInstaller,
    };
    let installer: Option<Box<dyn HookInstaller>> = match agent {
        "claude" => Some(Box::new(ClaudeHookInstaller::default())),
        "codex" => Some(Box::new(CodexHookInstaller::default())),
        // Amp has no `~/.config/amp/plugins.json` node-plugin mechanism (that
        // was assumed from OpenCode's model); writing one crashes Amp on load. A
        // reporter must never break the agent it observes, so Amp installs no
        // reporter — its status falls back to screen + physics evidence. A real
        // Amp reporter needs Amp's actual extension surface (MCP/toolbox) and is
        // tracked as remaining work.
        "opencode" => Some(Box::new(PluginInstaller::opencode())),
        _ => None,
    };
    if let Some(installer) = installer {
        let home = Path::new("/home/agent");
        if !installer.verify(home) {
            installer
                .install(home)
                .with_context(|| format!("install {agent} agent-status reporter"))?;
        }
    }
    Ok(())
}

fn setup_claude() -> Result<()> {
    let copy_auth = seed_agent_home_from_enum(jackin_core::Agent::Claude)?.is_first_seed();
    if copy_auth {
        if Path::new("/jackin/claude/account.json").is_file() {
            copy_file_with_mode("/jackin/claude/account.json", claude_account_path(), 0o600)?;
        }
        let credentials_path = claude_credentials_path();
        if Path::new("/jackin/claude/credentials.json").is_file() {
            copy_file_with_mode("/jackin/claude/credentials.json", &credentials_path, 0o600)?;
        } else {
            // First-setup only (inside `if copy_auth`): never clear a token a
            // later tab refreshed. See the run_agent_setup gate comment.
            remove_file_if_exists(&credentials_path)?;
            crate::output::stderr_line(format_args!(
                "[entrypoint] claude: no credentials.json forwarded - agent will start unauthenticated unless ANTHROPIC_API_KEY is set"
            ));
        }
    }

    if env_is_one("JACKIN_DISABLE_TIRITH") {
        crate::output::stdout_line(format_args!(
            "[entrypoint] tirith disabled (JACKIN_DISABLE_TIRITH=1)"
        ));
    } else {
        run_optional_command(
            "claude",
            &["mcp", "add", "tirith", "--", "tirith", "mcp-server"],
        );
    }
    if env_is_one("JACKIN_DISABLE_SHELLFIRM") {
        crate::output::stdout_line(format_args!(
            "[entrypoint] shellfirm disabled (JACKIN_DISABLE_SHELLFIRM=1)"
        ));
    } else {
        run_optional_command(
            "claude",
            &["mcp", "add", "shellfirm", "--", "shellfirm", "mcp"],
        );
    }
    setup_claude_plugins();
    Ok(())
}

/// Install the Claude plugin marketplaces and plugins declared by the role
/// manifest, once per home.
///
/// Plugin setup moved here from the image build: the `claude` binary is now
/// bind-mounted read-only at `docker run` (not baked into the derived image), so
/// there is no longer a build step to run `claude plugin install`. Idempotent via
/// a marker so re-launches and sibling tabs do not re-run it.
fn setup_claude_plugins() {
    let Some(config) = crate::config::load_optional() else {
        return;
    };
    if config.claude_marketplaces.is_empty() && config.claude_plugins.is_empty() {
        return;
    }
    // Re-run when the declared plugin set changes. The marker records the exact
    // marketplaces+plugins it was written for (the old image build keyed its
    // bundle cache on a hash of the same commands); a bare exists() check would
    // shadow a `jackin.role.toml` plugin edit forever.
    let fingerprint = claude_plugin_fingerprint(&config);
    let marker = Path::new("/home/agent/.claude/.jackin-plugins.done");
    if fs::read_to_string(marker).ok().as_deref() == Some(fingerprint.as_str()) {
        return;
    }
    // The official marketplace backs the common plugins; tolerate it already
    // being registered.
    run_optional_command(
        "claude",
        &[
            "plugin",
            "marketplace",
            "add",
            "anthropics/claude-plugins-official",
        ],
    );
    for marketplace in &config.claude_marketplaces {
        let mut args = vec!["plugin", "marketplace", "add", marketplace.source.as_str()];
        if !marketplace.sparse.is_empty() {
            args.push("--sparse");
            for path in &marketplace.sparse {
                args.push(path.as_str());
            }
        }
        run_optional_command("claude", &args);
    }
    for plugin in &config.claude_plugins {
        run_optional_command("claude", &["plugin", "install", plugin.as_str()]);
    }
    if let Err(e) = fs::create_dir_all("/home/agent/.claude") {
        crate::output::stderr_line(format_args!(
            "[entrypoint] claude plugins: failed to create marker dir: {e}"
        ));
    }
    if let Err(e) = fs::write(marker, &fingerprint) {
        crate::output::stderr_line(format_args!(
            "[entrypoint] claude plugins: failed to write install marker (plugins will re-run next launch): {e}"
        ));
    }
}

/// Stable fingerprint of the declared Claude marketplaces + plugins, stored as
/// the install marker's contents so a changed plugin set re-triggers install.
fn claude_plugin_fingerprint(config: &jackin_protocol::CapsuleConfig) -> String {
    let mut out = String::new();
    for marketplace in &config.claude_marketplaces {
        out.push_str("m:");
        out.push_str(&marketplace.source);
        for path in &marketplace.sparse {
            out.push(' ');
            out.push_str(path);
        }
        out.push('\n');
    }
    for plugin in &config.claude_plugins {
        out.push_str("p:");
        out.push_str(plugin);
        out.push('\n');
    }
    out
}

fn setup_codex() -> Result<()> {
    let copy_auth = seed_agent_home_from_enum(jackin_core::Agent::Codex)?.is_first_seed();
    // Provider config is idempotent and runs every start; the credential copy is
    // gated on first-seed only (D6/D19 home-emptiness, decided above), so it never
    // re-copies over an in-container-refreshed token regardless of ordering.
    write_codex_provider_config(Path::new("/home/agent/.codex"))?;
    if copy_auth {
        let auth_path = codex_auth_path();
        if Path::new("/jackin/codex/auth.json").is_file() {
            copy_file_with_mode("/jackin/codex/auth.json", &auth_path, 0o600)?;
        } else {
            remove_file_if_exists(&auth_path)?;
            crate::output::stderr_line(format_args!(
                "[entrypoint] codex: no auth.json forwarded - agent will start unauthenticated unless OPENAI_API_KEY is set"
            ));
        }
    }
    Ok(())
}

/// Appends the `[model_providers.minimax]` block to `config.toml` and writes
/// the v2 profile file `minimax.config.toml` under `codex_dir`. `MiniMax` is
/// the only deliverable Codex cell (Responses-API compatible); GLM and Kimi
/// are deferred. Both writes are idempotent across repeated setup invocations.
fn write_codex_provider_config(codex_dir: &Path) -> Result<()> {
    write_codex_provider_config_inner(
        codex_dir,
        nonempty_env("MINIMAX_API_KEY").is_some(),
        &codex_minimax_model(),
    )
}

/// `MiniMax` model Codex routes to: the role's `[codex.providers.minimax].model`
/// override (carried in the capsule config) when set, else the built-in default.
fn codex_minimax_model() -> String {
    crate::config::load_optional()
        .and_then(|config| config.provider_model("codex", "minimax").map(str::to_owned))
        .unwrap_or_else(|| jackin_protocol::MINIMAX_DEFAULT_MODEL.to_owned())
}

/// Core of [`write_codex_provider_config`] with env reading lifted out so tests
/// drive the MiniMax-present decision and the model directly (no process-global
/// env or config mutation).
fn write_codex_provider_config_inner(
    codex_dir: &Path,
    minimax_present: bool,
    model: &str,
) -> Result<()> {
    if !minimax_present {
        return Ok(());
    }
    fs::create_dir_all(codex_dir)
        .with_context(|| format!("failed to create {}", codex_dir.display()))?;

    // ── config.toml: append [model_providers.minimax] if not already present ──
    // Duplicate TOML table keys are a parse error, so we guard with a
    // substring check before appending.
    let config_path = codex_dir.join("config.toml");
    let provider_block_missing = !config_path.exists()
        || !fs::read_to_string(&config_path)
            .with_context(|| {
                format!(
                    "failed to read {} for idempotency check",
                    config_path.display()
                )
            })?
            .contains("[model_providers.minimax]");
    if provider_block_missing {
        let provider_block = codex_minimax_provider_toml()?;
        #[expect(
            clippy::disallowed_methods,
            reason = "capsule runtime setup runs before entering the multiplexer render loop"
        )]
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&config_path)
            .with_context(|| {
                format!(
                    "failed to open {} for provider config",
                    config_path.display()
                )
            })?;
        file.write_all(provider_block.as_bytes()).with_context(|| {
            format!(
                "failed to write MiniMax provider block to {}",
                config_path.display()
            )
        })?;
        crate::output::stdout_line(format_args!(
            "[entrypoint] codex: wrote MiniMax provider block to {}",
            config_path.display()
        ));
    } else {
        crate::cdebug!(
            "codex: [model_providers.minimax] already present in {}; skipping append",
            config_path.display()
        );
    }

    // ── minimax.config.toml: Codex v2 profile file, loaded by `--profile minimax` ──
    // Do NOT also write `[profiles.minimax]` into config.toml: Codex errors when
    // `--profile` is passed alongside a legacy v1 profiles table.
    let profile_path = codex_dir.join("minimax.config.toml");
    if profile_path.exists() {
        crate::cdebug!(
            "codex: {} already exists; leaving operator/prior profile as-is",
            profile_path.display()
        );
    } else {
        let profile = codex_minimax_profile_toml(model)?;
        fs::write(&profile_path, profile.as_bytes()).with_context(|| {
            format!(
                "failed to write MiniMax profile config to {}",
                profile_path.display()
            )
        })?;
        crate::output::stdout_line(format_args!(
            "[entrypoint] codex: wrote MiniMax profile config to {}",
            profile_path.display()
        ));
    }

    // ── minimax.models.json: model catalog so the MiniMax model has real metadata ─
    write_codex_minimax_catalog(codex_dir, model)?;

    Ok(())
}

/// Serializes the `[model_providers.minimax]` block for `config.toml` via the
/// `toml` crate. A leading newline separates it from any existing content.
fn codex_minimax_provider_toml() -> Result<String> {
    #[derive(serde::Serialize)]
    struct ProviderEntry {
        name: &'static str,
        base_url: &'static str,
        env_key: &'static str,
        wire_api: &'static str,
    }
    #[derive(serde::Serialize)]
    struct CodexBlock {
        model_providers: std::collections::BTreeMap<&'static str, ProviderEntry>,
    }
    let block = CodexBlock {
        model_providers: [(
            "minimax",
            ProviderEntry {
                name: "MiniMax",
                base_url: jackin_protocol::MINIMAX_OPENAI_BASE_URL,
                env_key: "MINIMAX_API_KEY",
                wire_api: "responses",
            },
        )]
        .into_iter()
        .collect(),
    };
    let body =
        toml::to_string(&block).context("failed to serialize Codex MiniMax provider block")?;
    Ok(format!("\n{body}"))
}

/// Serializes the Codex v2 profile file content (`minimax.config.toml`).
/// Loaded by `codex --profile minimax`; sets `model_provider` for that session.
/// The context window is NOT set here: a profile-scoped `model_context_window`
/// is clamped to the active model's fallback cap (~272k), so it can never raise
/// the window for a custom model. `minimax.models.json` carries the real 512k
/// window instead (see [`write_codex_minimax_catalog`]).
fn codex_minimax_profile_toml(model: &str) -> Result<String> {
    #[derive(serde::Serialize)]
    struct ProfileConfig<'a> {
        model_provider: &'static str,
        model: &'a str,
    }
    let config = ProfileConfig {
        model_provider: "minimax",
        model,
    };
    toml::to_string(&config).context("failed to serialize Codex MiniMax profile config")
}

/// Writes `minimax.models.json` — a Codex model catalog giving `MiniMax-M3` real
/// metadata (a 512k context window). MiniMax-M3 is absent from Codex's bundled
/// catalog, so without this Codex uses generic fallback metadata: a ~272k window
/// plus a "metadata not found, can degrade performance" warning on every turn,
/// and it clamps any `model_context_window` override to that fallback cap. A
/// catalog entry is the only mechanism that lifts the window. The entry is
/// derived at runtime from the installed Codex's own catalog (`codex debug
/// models`) so it always matches the running binary's `ModelInfo` schema rather
/// than a snapshot that drifts as Codex evolves. The entrypoint activates it
/// with `-c model_catalog_json=<file>` alongside `--profile minimax` (a
/// profile-file `model_catalog_json` key trips a Codex config-parse bug).
///
/// Best-effort: if Codex is missing or its output won't parse, the catalog is
/// skipped and Codex falls back to its generic metadata — the model still runs.
fn write_codex_minimax_catalog(codex_dir: &Path, model: &str) -> Result<()> {
    let catalog_path = codex_dir.join("minimax.models.json");
    if catalog_path.exists() {
        crate::cdebug!(
            "codex: {} already exists; leaving as-is",
            catalog_path.display()
        );
        return Ok(());
    }
    let Some(template) = codex_catalog_template_entry() else {
        crate::clog!(
            "codex: no usable entry from `codex debug models`; skipping MiniMax model catalog (Codex falls back to generic metadata)"
        );
        return Ok(());
    };
    let catalog = build_minimax_catalog(&template, model);
    let body = serde_json::to_string_pretty(&catalog)
        .context("failed to serialize MiniMax model catalog")?;
    fs::write(&catalog_path, body.as_bytes()).with_context(|| {
        format!(
            "failed to write MiniMax model catalog to {}",
            catalog_path.display()
        )
    })?;
    crate::output::stdout_line(format_args!(
        "[entrypoint] codex: wrote MiniMax model catalog to {}",
        catalog_path.display()
    ));
    Ok(())
}

/// First entry of the installed Codex's model catalog as an object map, used as a
/// schema-correct template. Any entry works: [`build_minimax_catalog`] overwrites
/// the identity and window fields and leaves the rest (tool config, capability
/// flags, base instructions) as the running binary already shaped them. `None`
/// when Codex is absent, fails, or its output has no model object to template.
fn codex_catalog_template_entry() -> Option<serde_json::Map<String, serde_json::Value>> {
    let mut command = Command::new("codex");
    command.args(["debug", "models"]);
    let output = runtime_setup_output(&mut command).ok()?;
    if !output.status.success() {
        return None;
    }
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    json.get("models")?
        .as_array()?
        .first()?
        .as_object()
        .cloned()
}

/// Patches a Codex catalog entry into the `MiniMax-M3` entry: real identity and
/// the `MiniMax` context window, with the template model's promo fields cleared.
fn build_minimax_catalog(
    template: &serde_json::Map<String, serde_json::Value>,
    model: &str,
) -> serde_json::Value {
    let mut entry = template.clone();
    entry.insert("slug".to_owned(), json!(model));
    entry.insert("display_name".to_owned(), json!(model));
    entry.insert(
        "description".to_owned(),
        json!("MiniMax Token Plan model (served via jackin)."),
    );
    let window = jackin_protocol::MINIMAX_CONTEXT_WINDOW;
    entry.insert("context_window".to_owned(), json!(window));
    entry.insert("max_context_window".to_owned(), json!(window));
    // Compact at 90% of the window so Codex compacts before truncating near the limit.
    entry.insert(
        "auto_compact_token_limit".to_owned(),
        json!(window * 9 / 10),
    );
    entry.insert("availability_nux".to_owned(), serde_json::Value::Null);
    entry.insert("upgrade".to_owned(), serde_json::Value::Null);
    json!({ "models": [entry] })
}

fn setup_amp() -> Result<()> {
    let copy_auth = seed_agent_home_from_enum(jackin_core::Agent::Amp)?.is_first_seed();
    let secrets_path = amp_secrets_path();
    if copy_auth {
        if Path::new("/jackin/amp/secrets.json").is_file() {
            crate::output::stderr_line(format_args!(
                "[entrypoint] amp: forwarding host secrets.json into ~/.local/share/amp/"
            ));
            copy_file_with_mode("/jackin/amp/secrets.json", &secrets_path, 0o600)?;
        } else if nonempty_env("AMP_API_KEY").is_some() {
            crate::output::stderr_line(format_args!(
                "[entrypoint] amp: AMP_API_KEY present in env; agent will use api-key auth"
            ));
        } else {
            remove_file_if_exists(&secrets_path)?;
            crate::output::stderr_line(format_args!(
                "[entrypoint] amp: no secrets.json mounted and AMP_API_KEY unset - agent will require interactive login"
            ));
        }
    }
    Ok(())
}

fn setup_kimi() -> Result<()> {
    let copy_auth = seed_agent_home_from_enum(jackin_core::Agent::Kimi)?.is_first_seed();
    if copy_auth {
        let kimi_src = Path::new("/jackin/kimi-code");
        if kimi_src.is_dir() && dir_nonempty(kimi_src)? {
            crate::output::stderr_line(format_args!(
                "[entrypoint] kimi: copying provisioned credentials into ~/.kimi-code/"
            ));
            copy_dir_contents(kimi_src, Path::new("/home/agent/.kimi-code"))?;
        } else if kimi_src.is_dir() {
            crate::output::stderr_line(format_args!(
                "[entrypoint] kimi: sync mode active but host ~/.kimi-code was absent at provision time - Kimi will start without forwarded auth"
            ));
        } else if nonempty_env("KIMI_CODE_API_KEY").is_some() {
            crate::output::stderr_line(format_args!(
                "[entrypoint] kimi: KIMI_CODE_API_KEY present in env; agent will use api-key auth"
            ));
        } else {
            crate::output::stderr_line(format_args!(
                "[entrypoint] kimi: KIMI_CODE_API_KEY unset - agent will require interactive login or config"
            ));
        }
    }
    Ok(())
}

fn setup_opencode() -> Result<()> {
    let copy_auth = seed_agent_home_from_enum(jackin_core::Agent::Opencode)?.is_first_seed();
    // Runtime provider config is written every start, layered on top of the
    // seeded `.config/opencode` defaults: it embeds live API keys from container
    // env, so it is never baked into default-home. Written before the credential
    // copy — see setup_codex for why the copy must be the last fallible step.
    let auth_path = opencode_auth_path();
    use std::os::unix::fs::DirBuilderExt as _;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create("/home/agent/.config/opencode")
        .context("failed to create /home/agent/.config/opencode")?;
    write_opencode_config(Path::new("/home/agent/.config/opencode/opencode.json"))?;
    if copy_auth {
        if Path::new("/jackin/opencode/auth.json").is_file() {
            crate::output::stderr_line(format_args!(
                "[entrypoint] opencode: forwarding host auth.json into ~/.local/share/opencode/"
            ));
            copy_file_with_mode("/jackin/opencode/auth.json", &auth_path, 0o600)?;
        } else if nonempty_env("OPENCODE_API_KEY").is_some() {
            crate::output::stderr_line(format_args!(
                "[entrypoint] opencode: OPENCODE_API_KEY present in env; agent will use api-key auth"
            ));
        } else {
            remove_file_if_exists(&auth_path)?;
            crate::output::stderr_line(format_args!(
                "[entrypoint] opencode: no auth.json mounted and OPENCODE_API_KEY unset - agent will require interactive login"
            ));
        }
    }
    Ok(())
}

fn setup_grok() -> Result<()> {
    let copy_auth = seed_agent_home_from_enum(jackin_core::Agent::Grok)?.is_first_seed();
    if copy_auth {
        if Path::new("/jackin/grok/auth.json").is_file() {
            crate::output::stderr_line(format_args!(
                "[entrypoint] grok: forwarding host auth.json into ~/.grok/"
            ));
            copy_file_with_mode("/jackin/grok/auth.json", GROK_AUTH_PATH, 0o600)?;
        } else if nonempty_env("XAI_API_KEY").is_some() {
            crate::output::stderr_line(format_args!(
                "[entrypoint] grok: XAI_API_KEY present in env; agent will use api-key auth"
            ));
        } else if nonempty_env("GROK_DEPLOYMENT_KEY").is_some() {
            crate::output::stderr_line(format_args!(
                "[entrypoint] grok: GROK_DEPLOYMENT_KEY present in env; agent will use deployment key auth"
            ));
        } else {
            remove_file_if_exists(GROK_AUTH_PATH)?;
            crate::output::stderr_line(format_args!(
                "[entrypoint] grok: no auth.json mounted and no XAI_API_KEY/GROK_DEPLOYMENT_KEY - agent will require interactive login"
            ));
        }
    }

    Ok(())
}

/// Writes `opencode.json` with `"permission":"allow"` plus a `provider` block
/// for every alt provider whose API key is present in the container env.
fn write_opencode_config(config: &Path) -> Result<()> {
    let cfg = build_opencode_config(
        nonempty_env("ZAI_API_KEY"),
        nonempty_env("MINIMAX_API_KEY"),
        nonempty_env("KIMI_CODE_API_KEY"),
    );
    write_opencode_json(config, &cfg)
}

/// Serializes `cfg` to `config` with mode 0o600 — the file embeds live API keys,
/// so it must never be group/world-readable. Env reading is lifted to the caller
/// so tests can assert the permission without process-global env mutation.
fn write_opencode_json(config: &Path, cfg: &serde_json::Value) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt as _;
    let mut content = serde_json::to_vec(cfg).context("failed to serialize opencode.json")?;
    content.push(b'\n');
    #[expect(
        clippy::disallowed_methods,
        reason = "capsule runtime setup runs before entering the multiplexer render loop"
    )]
    let mut f = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(0o600)
        .open(config)
        .context("failed to open opencode.json for writing")?;
    f.write_all(&content)
        .context("failed to write opencode.json")?;
    Ok(())
}

/// Builds the `opencode.json` value: base `"permission":"allow"` plus a
/// self-contained `provider` block for each alt provider whose key is present.
///
/// Each block fully defines the provider (npm SDK, baseURL, apiKey, the one
/// model id) instead of relying on `OpenCode`'s bundled models.dev registry. Two
/// reasons it must be self-contained: the registry keys Z.AI's credential off
/// `ZHIPU_API_KEY` (a name jackin never sets — so an apiKey-less block would
/// fail to authenticate), and the registry has no `kimi` provider at all (its
/// entry is `kimi-for-coding`), so a bare `{baseURL}` block leaves `OpenCode`
/// with no SDK or model list to resolve `-m kimi/kimi-for-coding`. The model id
/// is the suffix [`jackin_protocol::Provider::opencode_model`] emits for the
/// `-m <provider>/<model>` flag; the test binds the two so they cannot drift.
fn build_opencode_config(
    zai_key: Option<String>,
    minimax_key: Option<String>,
    kimi_key: Option<String>,
) -> serde_json::Value {
    let mut providers = serde_json::Map::new();
    if let Some(key) = zai_key {
        providers.insert(
            "zai".to_owned(),
            opencode_provider_block(
                "Z.AI",
                "@ai-sdk/openai-compatible",
                jackin_protocol::ZAI_OPENAI_BASE_URL,
                &key,
                jackin_protocol::ZAI_DEFAULT_OPUS_MODEL,
            ),
        );
    }
    if let Some(key) = minimax_key {
        providers.insert(
            "minimax".to_owned(),
            opencode_provider_block(
                "MiniMax",
                "@ai-sdk/anthropic",
                // `@ai-sdk/anthropic` appends `/messages` to baseURL (its default
                // is `…/v1`), whereas Claude Code's SDK appends `/v1/messages`.
                // So the OpenCode block needs the `/v1` the Claude-path constant omits.
                &format!("{}/v1", jackin_protocol::MINIMAX_BASE_URL),
                &key,
                jackin_protocol::MINIMAX_DEFAULT_MODEL,
            ),
        );
    }
    if let Some(key) = kimi_key {
        providers.insert(
            "kimi".to_owned(),
            opencode_provider_block(
                "Kimi",
                "@ai-sdk/anthropic",
                // See MiniMax note: `@ai-sdk/anthropic` needs `/v1` in baseURL.
                &format!("{}/v1", jackin_protocol::KIMI_BASE_URL),
                &key,
                jackin_protocol::KIMI_DEFAULT_MODEL,
            ),
        );
    }
    let mut cfg = json!({"permission": "allow"});
    if !providers.is_empty() {
        cfg["provider"] = serde_json::Value::Object(providers);
    }
    cfg
}

/// One `OpenCode` custom-provider block. `model_id` is both the sole entry in the
/// `models` map and the suffix `OpenCode` matches after the provider id in
/// `-m <provider>/<model_id>`. `MiniMax` and Kimi speak the Anthropic wire format
/// (npm `@ai-sdk/anthropic`), but with a `/v1`-suffixed baseURL since that SDK
/// appends only `/messages`; Z.AI's coding-plan endpoint is OpenAI-compatible.
fn opencode_provider_block(
    name: &str,
    npm: &str,
    base_url: &str,
    api_key: &str,
    model_id: &str,
) -> serde_json::Value {
    let mut models = serde_json::Map::new();
    models.insert(model_id.to_owned(), json!({ "name": model_id }));
    json!({
        "name": name,
        "npm": npm,
        "options": { "baseURL": base_url, "apiKey": api_key },
        "models": serde_json::Value::Object(models),
    })
}

/// Whether a durable home was empty and got seeded on this start. Named instead
/// of a bare `bool` so the seed/auth contract is explicit at every call site:
/// auth handoff is copied only on [`SeedOutcome::FirstSeed`] (D19).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SeedOutcome {
    /// The home was empty (or absent); defaults were seeded and first-start auth
    /// handoff should be copied.
    FirstSeed,
    /// The home already held durable state; nothing was touched and auth must
    /// not be re-copied over in-container credentials.
    AlreadySeeded,
}

impl SeedOutcome {
    /// True on the first seed, when first-start auth handoff must run (D19).
    fn is_first_seed(self) -> bool {
        matches!(self, Self::FirstSeed)
    }
}

/// D5/D6: empty-dir gate + first seed.
///
/// Returns [`SeedOutcome::FirstSeed`] when dst was empty, [`SeedOutcome::AlreadySeeded`]
/// when dst already has entries (seeded on a prior start; in-container files are
/// authoritative). Auth is copied by the caller only on `FirstSeed` (D19).
///
/// If `dst` already exists, it may be a Docker bind mount target; seed it in
/// place because POSIX cannot rename over a mount point.
fn seed_home_dir(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> Result<SeedOutcome> {
    let src = src.as_ref();
    let dst = dst.as_ref();

    // D5: gate on emptiness — non-empty dst is authoritative, skip
    if dst.is_dir() && !is_dir_empty(dst) {
        return Ok(SeedOutcome::AlreadySeeded);
    }

    if !src.is_dir() {
        // No baked defaults; dst stays empty (or absent) — still first setup
        return Ok(SeedOutcome::FirstSeed);
    }

    if dst.exists() {
        // In-place copy is NOT atomic (a crash mid-copy leaves a partial home the
        // emptiness gate then treats as durable). Accepted because `dst` here is a
        // Docker bind-mount target, which POSIX cannot `rename` over — the atomic
        // rename path below applies only when `dst` is absent.
        copy_dir_contents(src, dst)?;
        return Ok(SeedOutcome::FirstSeed);
    }

    // Atomic seed: copy to sibling temp (same mount → rename is atomic on POSIX)
    let parent = dst.parent().unwrap_or(Path::new("/"));
    fs::create_dir_all(parent)
        .with_context(|| format!("failed to create parent {}", parent.display()))?;
    let tmp = TempfileBuilder::new()
        .prefix(".jackin-seed")
        .tempdir_in(parent)
        .with_context(|| format!("failed to create seed temp dir in {}", parent.display()))?;
    copy_dir_contents(src, tmp.path())?;
    let tmp_path = tmp.keep(); // keep() returns PathBuf, prevents Drop removal
    // dst does not exist here (the dst.exists() branch above handled that), so
    // rename the staged tree onto it directly.
    if let Err(err) = fs::rename(&tmp_path, dst) {
        // keep() defused the Drop guard, so a failed rename would orphan the
        // staging dir next to the durable home — remove it before surfacing.
        let _unused = fs::remove_dir_all(&tmp_path);
        return Err(err).with_context(|| {
            format!(
                "atomic seed: rename {} → {}",
                tmp_path.display(),
                dst.display()
            )
        });
    }

    Ok(SeedOutcome::FirstSeed)
}

/// D4/D6: seed an agent's durable home, gated on the primary data root's
/// emptiness, and — for agents that persist a separate config root — seed that
/// paired config root in the same first-seed pass (two sequential seeds, not one
/// atomic transaction). Both roots share one lifecycle: empty data root means
/// first start (seed both, returning
/// [`SeedOutcome::FirstSeed`] so the caller copies auth); if *either* root already
/// holds durable content, treat the agent as existing state and leave both
/// untouched ([`SeedOutcome::AlreadySeeded`]).
fn seed_agent_home(
    data_default: &str,
    data_dst: &str,
    config: Option<(&str, &str)>,
) -> Result<SeedOutcome> {
    if let Some((config_default, config_dst)) = config {
        // A config root with durable content means the agent is already set up,
        // even if the data root looks empty (e.g. a partially recreated mount):
        // never re-seed or re-copy auth over it.
        let config_path = Path::new(config_dst);
        if config_path.is_dir() && !is_dir_empty(config_path) {
            return Ok(SeedOutcome::AlreadySeeded);
        }
        let outcome = seed_home_dir(data_default, data_dst)?;
        if outcome.is_first_seed() {
            seed_home_dir(config_default, config_dst)?;
        }
        return Ok(outcome);
    }
    seed_home_dir(data_default, data_dst)
}

/// Seed `agent`'s durable home from `/jackin/default-home`, deriving the data and
/// paired-config roots from the agent enum
/// ([`AgentStatePaths`](jackin_core::agent::runtime::AgentStatePaths)) so the
/// per-agent folder layout has one source of truth. Returns the first-seed
/// outcome; the caller copies auth only on [`SeedOutcome::FirstSeed`] (D19).
fn seed_agent_home_from_enum(agent: jackin_core::Agent) -> Result<SeedOutcome> {
    let paths = agent.runtime().state_paths();
    let data_default = format!("/jackin/default-home/{}", paths.credential_dir);
    let data_dst = format!("/home/agent/{}", paths.credential_dir);
    match paths.config_dir {
        Some(config_dir) => {
            let config_default = format!("/jackin/default-home/{config_dir}");
            let config_dst = format!("/home/agent/{config_dir}");
            seed_agent_home(
                &data_default,
                &data_dst,
                Some((&config_default, &config_dst)),
            )
        }
        None => seed_agent_home(&data_default, &data_dst, None),
    }
}

fn is_dir_empty(path: &Path) -> bool {
    fs::read_dir(path).map_or(true, |mut d| d.next().is_none())
}

fn copy_dir_contents(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst).with_context(|| format!("failed to create {}", dst.display()))?;
    for entry in fs::read_dir(src).with_context(|| format!("failed to read {}", src.display()))? {
        let entry = entry?;
        let entry_src = entry.path();
        let entry_dst = dst.join(entry.file_name());
        let metadata = entry
            .metadata()
            .with_context(|| format!("failed to stat {}", entry_src.display()))?;
        if metadata.is_dir() {
            copy_dir_contents(&entry_src, &entry_dst)?;
        } else {
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
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
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
    let mut command = Command::new("git");
    command.args(["config", "--global", "core.hooksPath"]);
    let Ok(output) = runtime_setup_output(&mut command) else {
        return false;
    };
    output.status.success() && String::from_utf8_lossy(&output.stdout).trim_end() == GIT_HOOKS_DIR
}

fn hook_points_to_capsule() -> bool {
    fs::read_link(GIT_HOOK_PATH).is_ok_and(|target| target == Path::new(CAPSULE_RUNTIME_BIN))
}

fn coauthor_trailer_for_agent(agent: &str) -> Option<&'static str> {
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
    let cache_path = git_dco_identity_cache_path();
    if let Err(err) = fs::write(&cache_path, format!("{name}\n{email}\n")) {
        // A failed cache write means every commit shells out to live git
        // config — the exact failure this cache exists to prevent.
        crate::clog!(
            "dco identity cache write to {} failed: {err} (errno={:?})",
            cache_path.display(),
            err.raw_os_error()
        );
    }
}

fn read_cached_dco_identity() -> Option<(String, String)> {
    let content = fs::read_to_string(git_dco_identity_cache_path()).ok()?;
    let mut lines = content.lines();
    let name = lines.next().filter(|s| !s.is_empty())?.to_owned();
    let email = lines.next().filter(|s| !s.is_empty())?.to_owned();
    Some((name, email))
}

fn git_dco_identity_cache_path() -> PathBuf {
    #[cfg(debug_assertions)]
    if let Some(path) = std::env::var_os(GIT_DCO_IDENTITY_CACHE_ENV) {
        return PathBuf::from(path);
    }
    PathBuf::from(GIT_DCO_IDENTITY_CACHE)
}

fn git_config_value(key: &str) -> Option<String> {
    let mut command = Command::new("git");
    command.args(["config", key]);
    let output = runtime_setup_output(&mut command).ok()?;
    if !output.status.success() {
        return None;
    }
    Some(
        String::from_utf8_lossy(&output.stdout)
            .trim_end()
            .to_owned(),
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
    command.args(["--trailer", trailer]).arg(message_path);
    let output = runtime_setup_output(&mut command)
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
    let mut command = Command::new("git");
    command.args(["config", "--global", "--get-all", key]);
    let output = runtime_setup_output(&mut command)
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

pub(crate) fn run_command(program: &str, args: &[&str]) -> Result<()> {
    let mut command = Command::new(program);
    command.args(args);
    let output = runtime_setup_output(&mut command)
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

fn runtime_setup_output(command: &mut Command) -> io::Result<Output> {
    #[expect(
        clippy::disallowed_methods,
        reason = "capsule runtime setup runs before entering the multiplexer render loop"
    )]
    command.output()
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
    fs::metadata(path).is_ok_and(|metadata| metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(test)]
mod tests;
