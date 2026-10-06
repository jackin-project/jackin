// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Agent home and credential path resolvers.

use super::nonempty_env;

use std::path::{Path, PathBuf};

// Container home for the `agent` user. Every default agent config/credential
// location hangs off this. The per-agent resolvers below honor an agent's
// standard config-dir env var (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`,
// `XDG_DATA_HOME`) when the role sets one, falling back here otherwise — so a
// role that exports e.g. `CLAUDE_CONFIG_DIR` has its credentials written where
// the CLI actually looks, instead of the fixed default the CLI no longer reads.
pub(crate) const AGENT_HOME: &str = "/home/agent";

// Grok has no standard config-dir env var, so its credential path is fixed.
pub(crate) const GROK_AUTH_PATH: &str = "/home/agent/.grok/auth.json";

// Each resolver pairs a thin env-reading wrapper with a pure `_from` core, so
// path composition is unit-tested without mutating process-global env (which is
// racy across parallel tests and `unsafe` under Rust 2024).

/// Resolve a path under `AGENT_HOME` honoring an optional env override.
/// When `env` is set, its value is used verbatim; otherwise falls back to
/// `AGENT_HOME/subpath`. Shared by all three standard env-override resolvers.
pub(crate) fn env_or_agent_home(env: Option<&str>, subpath: &str) -> PathBuf {
    env.map_or_else(|| Path::new(AGENT_HOME).join(subpath), PathBuf::from)
}

/// Resolve Claude Code's config directory, honoring `CLAUDE_CONFIG_DIR` when the
/// role sets it (default `~/.claude`). Claude reads `.credentials.json` — and,
/// when the env var is set, `.claude.json` — from this directory.
pub(crate) fn claude_config_dir() -> PathBuf {
    claude_config_dir_from(nonempty_env("CLAUDE_CONFIG_DIR").as_deref())
}

pub(crate) fn claude_config_dir_from(env: Option<&str>) -> PathBuf {
    env_or_agent_home(env, ".claude")
}

/// `.credentials.json` always lives inside the resolved Claude config dir.
pub(crate) fn claude_credentials_path() -> PathBuf {
    claude_config_dir().join(".credentials.json")
}

/// `.claude.json` placement is asymmetric: with `CLAUDE_CONFIG_DIR` set it lives
/// inside that dir; with it unset Claude reads `$HOME/.claude.json` (home root).
/// Writing it on the wrong side leaves the CLI unable to find its onboarding
/// state, so it falls back to the interactive login screen even though a valid
/// `.credentials.json` was forwarded.
pub(crate) fn claude_account_path() -> PathBuf {
    claude_account_path_from(nonempty_env("CLAUDE_CONFIG_DIR").as_deref())
}

pub(crate) fn claude_account_path_from(env: Option<&str>) -> PathBuf {
    Path::new(env.unwrap_or(AGENT_HOME)).join(".claude.json")
}

/// Codex reads `auth.json` and `config.toml` from `CODEX_HOME` (default `~/.codex`).
pub(crate) fn codex_home() -> PathBuf {
    codex_home_from(nonempty_env("CODEX_HOME").as_deref())
}

pub(crate) fn codex_home_from(env: Option<&str>) -> PathBuf {
    env_or_agent_home(env, ".codex")
}

pub(crate) fn codex_auth_path() -> PathBuf {
    codex_home().join("auth.json")
}

/// XDG data root honored by Amp and opencode (default `~/.local/share`).
pub(crate) fn xdg_data_home() -> PathBuf {
    xdg_data_home_from(nonempty_env("XDG_DATA_HOME").as_deref())
}

pub(crate) fn xdg_data_home_from(env: Option<&str>) -> PathBuf {
    env_or_agent_home(env, ".local/share")
}

pub(crate) fn amp_secrets_path() -> PathBuf {
    xdg_data_home().join("amp/secrets.json")
}

pub(crate) fn opencode_auth_path() -> PathBuf {
    xdg_data_home().join("opencode/auth.json")
}

/// Gemini home: `GEMINI_CLI_HOME` names the *parent* to which `.gemini` is
/// appended (it is not the config dir itself); default `~/.gemini`.
pub(crate) fn gemini_home() -> PathBuf {
    gemini_home_from(nonempty_env("GEMINI_CLI_HOME").as_deref())
}

pub(crate) fn gemini_home_from(env: Option<&str>) -> PathBuf {
    match env {
        Some(parent) => Path::new(parent).join(".gemini"),
        None => Path::new(AGENT_HOME).join(".gemini"),
    }
}

pub(crate) fn gemini_oauth_creds_path() -> PathBuf {
    gemini_home().join("oauth_creds.json")
}

pub(crate) fn antigravity_settings_path() -> PathBuf {
    gemini_home().join("antigravity-cli/settings.json")
}

/// Cursor reads `auth.json` from `CURSOR_CONFIG_DIR` (default `~/.cursor`).
/// Verbatim-dir semantics assumed from the variable name and docs citation.
pub(crate) fn cursor_home() -> PathBuf {
    cursor_home_from(nonempty_env("CURSOR_CONFIG_DIR").as_deref())
}

pub(crate) fn cursor_home_from(env: Option<&str>) -> PathBuf {
    env_or_agent_home(env, ".cursor")
}

pub(crate) fn cursor_auth_path() -> PathBuf {
    cursor_home().join("auth.json")
}

// Muse has no observed config-dir env var, so its credential path is fixed.
pub(crate) const MUSE_AUTH_PATH: &str = "/home/agent/.config/muse/auth.json";

/// `omp` reads its `SQLite` store from `PI_CODING_AGENT_DIR` (default `~/.omp`);
/// `OMP_PROFILE` selects a named profile within that dir.
pub(crate) fn omp_home() -> PathBuf {
    omp_home_from(nonempty_env("PI_CODING_AGENT_DIR").as_deref())
}

pub(crate) fn omp_home_from(env: Option<&str>) -> PathBuf {
    env_or_agent_home(env, ".omp")
}

pub(crate) fn omp_agent_db_path() -> PathBuf {
    omp_home().join("agent/agent.db")
}

/// Hermes reads its store from `HERMES_HOME` (default `~/.hermes`).
pub(crate) fn hermes_home() -> PathBuf {
    hermes_home_from(nonempty_env("HERMES_HOME").as_deref())
}

pub(crate) fn hermes_home_from(env: Option<&str>) -> PathBuf {
    env_or_agent_home(env, ".hermes")
}

/// Resolve a host-forwarded credential file for this instance: when the
/// daemon set `JACKIN_FORWARDED_DIR`, join the legacy file name onto it;
/// otherwise use the legacy path (primary slots).
pub(crate) fn forwarded_file(legacy: &str) -> PathBuf {
    forwarded_file_from(
        nonempty_env(jackin_protocol::INSTANCE_FORWARDED_DIR_ENV).as_deref(),
        legacy,
    )
}

pub(crate) fn forwarded_file_from(env: Option<&str>, legacy: &str) -> PathBuf {
    match env {
        Some(dir) => Path::new(dir).join(Path::new(legacy).file_name().unwrap_or_default()),
        None => PathBuf::from(legacy),
    }
}

/// Resolve a host-forwarded credential directory for this instance:
/// the daemon's `JACKIN_FORWARDED_DIR` when set, else the legacy dir.
pub(crate) fn forwarded_dir(legacy: &str) -> PathBuf {
    forwarded_dir_from(
        nonempty_env(jackin_protocol::INSTANCE_FORWARDED_DIR_ENV).as_deref(),
        legacy,
    )
}

pub(crate) fn forwarded_dir_from(env: Option<&str>, legacy: &str) -> PathBuf {
    env.map_or_else(|| PathBuf::from(legacy), PathBuf::from)
}
