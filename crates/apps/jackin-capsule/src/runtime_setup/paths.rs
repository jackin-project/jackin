// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::nonempty_env;

use std::path::{Path, PathBuf};

pub(crate) const AGENT_HOME: &str = "/home/agent";

pub(crate) const GROK_AUTH_PATH: &str = "/home/agent/.grok/auth.json";

pub(crate) fn env_or_agent_home(env: Option<&str>, subpath: &str) -> PathBuf {
    env.map_or_else(|| Path::new(AGENT_HOME).join(subpath), PathBuf::from)
}

pub(crate) fn claude_config_dir() -> PathBuf {
    claude_config_dir_from(nonempty_env("CLAUDE_CONFIG_DIR").as_deref())
}

pub(crate) fn claude_config_dir_from(env: Option<&str>) -> PathBuf {
    env_or_agent_home(env, ".claude")
}

pub(crate) fn claude_credentials_path() -> PathBuf {
    claude_config_dir().join(".credentials.json")
}

pub(crate) fn claude_account_path() -> PathBuf {
    claude_account_path_from(nonempty_env("CLAUDE_CONFIG_DIR").as_deref())
}

pub(crate) fn claude_account_path_from(env: Option<&str>) -> PathBuf {
    Path::new(env.unwrap_or(AGENT_HOME)).join(".claude.json")
}

pub(crate) fn codex_home() -> PathBuf {
    codex_home_from(nonempty_env("CODEX_HOME").as_deref())
}

pub(crate) fn codex_home_from(env: Option<&str>) -> PathBuf {
    env_or_agent_home(env, ".codex")
}

pub(crate) fn codex_auth_path() -> PathBuf {
    codex_home().join("auth.json")
}

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

pub(crate) fn cursor_home() -> PathBuf {
    cursor_home_from(nonempty_env("CURSOR_CONFIG_DIR").as_deref())
}

pub(crate) fn cursor_home_from(env: Option<&str>) -> PathBuf {
    env_or_agent_home(env, ".cursor")
}

pub(crate) fn cursor_auth_path() -> PathBuf {
    cursor_home().join("auth.json")
}

pub(crate) const MUSE_AUTH_PATH: &str = "/home/agent/.config/muse/auth.json";

pub(crate) fn omp_home() -> PathBuf {
    omp_home_from(nonempty_env("PI_CODING_AGENT_DIR").as_deref())
}

pub(crate) fn omp_home_from(env: Option<&str>) -> PathBuf {
    env_or_agent_home(env, ".omp")
}

pub(crate) fn omp_agent_db_path() -> PathBuf {
    omp_home().join("agent/agent.db")
}

pub(crate) fn hermes_home() -> PathBuf {
    hermes_home_from(nonempty_env("HERMES_HOME").as_deref())
}

pub(crate) fn hermes_home_from(env: Option<&str>) -> PathBuf {
    env_or_agent_home(env, ".hermes")
}

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

pub(crate) fn forwarded_dir(legacy: &str) -> PathBuf {
    forwarded_dir_from(
        nonempty_env(jackin_protocol::INSTANCE_FORWARDED_DIR_ENV).as_deref(),
        legacy,
    )
}

pub(crate) fn forwarded_dir_from(env: Option<&str>, legacy: &str) -> PathBuf {
    env.map_or_else(|| PathBuf::from(legacy), PathBuf::from)
}
