// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{
    apply_terminal_env, is_explicit_capability_env, isolated_command, remove_ambient_capability_env,
};
use jackin_core::container_paths;
use std::path::{Path, PathBuf};

use portable_pty::CommandBuilder;

pub(crate) fn inject_status_env(
    cmd: &mut CommandBuilder,
    session_id: u64,
    agent: Option<&str>,
    cache_dir: Option<&str>,
    control_capability: &str,
) {
    let session_root = session_root_path(session_id);
    let session_state = session_root.join("state");
    let session_tmp = session_root.join("tmp");
    let session_runtime = session_root.join("runtime");
    let session_cache = session_root.join("cache");

    cmd.env("JACKIN_SESSION_ROOT", &session_root);
    cmd.env(jackin_protocol::SESSION_STATE_DIR_ENV, &session_state);
    cmd.env("TMPDIR", &session_tmp);
    cmd.env("TMP", &session_tmp);
    cmd.env("TEMP", &session_tmp);
    cmd.env("XDG_RUNTIME_DIR", &session_runtime);
    if let Some(cache_dir) = cache_dir {
        cmd.env("XDG_CACHE_HOME", cache_dir);
    } else {
        cmd.env("XDG_CACHE_HOME", &session_cache);
    }
    cmd.env("GIT_CONFIG_GLOBAL", session_root.join("gitconfig"));
    cmd.env(jackin_protocol::SESSION_CAPABILITY_ENV, control_capability);
    cmd.env(
        jackin_protocol::ISOLATION_SESSION_ID_ENV,
        session_id.to_string(),
    );
    cmd.env_remove(jackin_protocol::SESSION_ID_ENV);
    cmd.env("JACKIN_STATUS_SOCKET", crate::socket::SOCKET_PATH);
    if let Some(runtime) = agent {
        cmd.env(jackin_protocol::SESSION_ID_ENV, session_id.to_string());
        cmd.env("JACKIN_AGENT_RUNTIME", runtime);
        cmd.env(
            "JACKIN_STATUS_SOURCE",
            format!("hook-{runtime}-{session_id}"),
        );
    } else {
        cmd.env_remove("JACKIN_AGENT_RUNTIME");
        cmd.env_remove("JACKIN_STATUS_SOURCE");
    }
}

pub(crate) fn session_root_path(session_id: u64) -> PathBuf {
    Path::new(container_paths::SESSION_ROOTS_DIR).join(session_id.to_string())
}

#[derive(Debug)]
pub struct AgentSpawnSpec<'a> {
    pub agent: &'a str,
    pub instance: &'a str,
    pub home_dir: &'a str,
    pub forwarded_dir: &'a str,
    pub model: Option<&'a str>,
    pub effort: Option<&'a str>,
    pub auth_mode: Option<&'a str>,
    pub env_passthrough: &'a [(String, String)],
    pub cwd: &'a Path,
    pub codename: &'a str,

    pub identity: jackin_protocol::SessionIdentity,
}

pub(crate) fn is_folder_env(name: &str) -> bool {
    jackin_core::Agent::ALL.iter().any(|agent| {
        agent
            .runtime()
            .state_paths()
            .folder_env_var
            .is_some_and(|var| var.name == name)
    })
}

#[must_use]
pub fn build_agent_command(spec: &AgentSpawnSpec<'_>) -> CommandBuilder {
    let mut cmd = isolated_command(
        spec.identity,
        Some(spec.instance),
        container_paths::ENTRYPOINT,
    );
    remove_ambient_capability_env(&mut cmd);
    for arg in agent_model_args(spec.agent, spec.model) {
        cmd.arg(arg);
    }
    for name in jackin_core::account_env_names() {
        cmd.env_remove(name);
    }
    for agent in jackin_core::Agent::ALL {
        if let Some(var) = agent.runtime().state_paths().folder_env_var {
            cmd.env_remove(var.name);
        }
    }
    for (k, v) in spec.env_passthrough {
        if !jackin_core::is_account_env(k) && !is_folder_env(k) && !is_explicit_capability_env(k) {
            cmd.env(k, v);
        }
    }
    apply_lane_env(&mut cmd, spec.agent, spec.model, spec.effort);
    if let Some(agent) = jackin_core::Agent::from_slug(spec.agent)
        && let Some(var) = agent.runtime().state_paths().folder_env_var
    {
        cmd.env(var.name, spec.home_dir);
    }

    cmd.env("HOME", spec.home_dir);
    cmd.env("JACKIN_AGENT", spec.agent);
    cmd.env(jackin_protocol::INSTANCE_ENV, spec.instance);
    cmd.env(
        jackin_protocol::INSTANCE_FORWARDED_DIR_ENV,
        spec.forwarded_dir,
    );
    if let Some(auth_mode) = spec.auth_mode {
        cmd.env(jackin_protocol::AUTH_MODE_ENV, auth_mode);
    } else {
        cmd.env_remove(jackin_protocol::AUTH_MODE_ENV);
    }
    cmd.env("JACKIN_AGENT_CODENAME", spec.codename);
    apply_terminal_env(&mut cmd);
    cmd.cwd(spec.cwd);
    cmd
}

pub(crate) fn agent_model_args<'a>(agent: &str, model: Option<&'a str>) -> Vec<&'a str> {
    let Some(model) = model else {
        return Vec::new();
    };
    match agent {
        "claude" | "kimi" | "omp" | "hermes" => vec!["--model", model],
        "codex" | "opencode" | "grok" => vec!["-m", model],
        _ => Vec::new(),
    }
}

pub(crate) fn apply_lane_env(
    cmd: &mut CommandBuilder,
    agent: &str,
    model: Option<&str>,
    effort: Option<&str>,
) {
    for name in [
        jackin_core::CODEX_LANE_MODEL_ENV_NAME,
        jackin_core::CODEX_LANE_EFFORT_ENV_NAME,
        jackin_core::CLAUDE_MODEL_ENV_NAME,
        jackin_core::CLAUDE_EFFORT_ENV_NAME,
    ] {
        cmd.env_remove(name);
    }
    let (model_env, effort_env) = match agent {
        "codex" => (
            jackin_core::CODEX_LANE_MODEL_ENV_NAME,
            jackin_core::CODEX_LANE_EFFORT_ENV_NAME,
        ),
        "claude" => (
            jackin_core::CLAUDE_MODEL_ENV_NAME,
            jackin_core::CLAUDE_EFFORT_ENV_NAME,
        ),
        _ => return,
    };
    if let Some(model) = model.map(str::trim).filter(|model| !model.is_empty()) {
        cmd.env(model_env, model);
    }
    if let Some(effort) = effort {
        cmd.env(effort_env, effort);
    }
}
