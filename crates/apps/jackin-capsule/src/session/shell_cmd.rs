// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::EXPLICIT_CAPABILITY_ENV_NAMES;
use jackin_core::container_paths;
use std::path::Path;

use portable_pty::CommandBuilder;

#[must_use]
pub fn build_shell_command(
    env_passthrough: &[(String, String)],
    cwd: &Path,
    codename: &str,
    identity: jackin_protocol::SessionIdentity,
) -> CommandBuilder {
    let shell = shell_executable();
    let mut cmd = isolated_command(identity, None, &shell);
    remove_ambient_capability_env(&mut cmd);
    for name in jackin_core::account_env_names() {
        cmd.env_remove(name);
    }
    if let Ok(home) = std::env::var("HOME") {
        cmd.env("HOME", home);
    }
    for (k, v) in env_passthrough {
        if !jackin_core::is_account_env(k) && !is_explicit_capability_env(k) {
            cmd.env(k, v);
        }
    }
    cmd.env_remove("JACKIN_AGENT");
    cmd.env("JACKIN_AGENT_CODENAME", codename);
    apply_terminal_env(&mut cmd);
    cmd.cwd(cwd);
    cmd
}

pub(crate) fn is_explicit_capability_env(name: &str) -> bool {
    EXPLICIT_CAPABILITY_ENV_NAMES.contains(&name)
}

pub(crate) fn remove_ambient_capability_env(cmd: &mut CommandBuilder) {
    for name in EXPLICIT_CAPABILITY_ENV_NAMES {
        cmd.env_remove(name);
    }
}

pub(crate) fn isolated_command(
    identity: jackin_protocol::SessionIdentity,
    instance: Option<&str>,
    program: impl AsRef<std::ffi::OsStr>,
) -> CommandBuilder {
    let mut cmd = CommandBuilder::new(container_paths::CAPSULE_BIN);
    cmd.args(isolated_wrapper_args(identity, instance, program));
    cmd
}

pub(crate) fn isolated_wrapper_args(
    identity: jackin_protocol::SessionIdentity,
    instance: Option<&str>,
    program: impl AsRef<std::ffi::OsStr>,
) -> Vec<std::ffi::OsString> {
    vec![
        "__isolated-exec".into(),
        instance.unwrap_or("-").into(),
        identity.uid.to_string().into(),
        identity.gid.to_string().into(),
        program.as_ref().to_owned(),
    ]
}

pub(crate) fn shell_executable() -> std::ffi::OsString {
    "/bin/zsh".into()
}

pub(crate) fn apply_terminal_env(cmd: &mut CommandBuilder) {
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    for key in ["LANG", "LC_ALL"] {
        if let Ok(value) = std::env::var(key) {
            cmd.env(key, value);
        }
    }
}

pub(crate) fn apply_account_env(
    command: &mut CommandBuilder,
    instance: &str,
    auth_mode: Option<&str>,
    provider_surface: Option<&str>,
    credentials: &jackin_protocol::AgentCredentialEnv,
) {
    if !matches!(auth_mode, Some("api_key" | "oauth_token")) {
        return;
    }
    if let Some(env) = credentials.for_instance(instance) {
        let Some(entry) = credentials.instance(instance) else {
            return;
        };
        let Some(command_agent) = command
            .get_env("JACKIN_AGENT")
            .and_then(|value| value.to_str())
        else {
            return;
        };
        if entry.agent != command_agent {
            return;
        }
        let Some(provider_surface) = provider_surface else {
            return;
        };
        let Ok(allowed) = crate::config::allowed_account_env_names(
            &entry.agent,
            auth_mode.unwrap_or_default(),
            provider_surface,
        ) else {
            return;
        };
        for (name, value) in env {
            if allowed.contains(name.as_str()) {
                command.env(name, value);
            }
        }
    }
}
