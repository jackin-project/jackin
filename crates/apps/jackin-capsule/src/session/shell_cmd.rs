// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Shell command construction and capability env sanitation.

use super::EXPLICIT_CAPABILITY_ENV_NAMES;
#[cfg(not(test))]
use jackin_core::container_paths;
use std::path::Path;

use portable_pty::CommandBuilder;

/// Build a `CommandBuilder` for an interactive shell session.
///
/// See `build_agent_command` for the `cwd` rationale.
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
    // Shells have no instance home: restore the daemon's container `HOME`
    // (`/home/agent`, container-controlled rather than operator-controlled)
    // so the shell and its tools resolve the shared container home instead
    // of running with no home at all.
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

/// Build the internal root-supervisor wrapper command. The wrapper validates
/// the identity against the launch config, installs Landlock, drops to the
/// slot UID, and only then executes the requested program.
pub(crate) fn isolated_command(
    identity: jackin_protocol::SessionIdentity,
    instance: Option<&str>,
    program: impl AsRef<std::ffi::OsStr>,
) -> CommandBuilder {
    #[cfg(test)]
    {
        // Session unit tests run on the host, where the container-only capsule
        // binary and entrypoint paths do not exist. The production path below
        // is exercised by the dedicated process-isolation boundary tests.
        let _ = (identity, instance);
        CommandBuilder::new(program)
    }
    #[cfg(not(test))]
    {
        let mut cmd = CommandBuilder::new(container_paths::CAPSULE_BIN);
        cmd.args(isolated_wrapper_args(identity, instance, program));
        cmd
    }
}

/// Exact argv passed to the capsule root supervisor for one session. Kept
/// pure so tests can prove the admitted identity is actually wired into the
/// production spawn command even though host-side PTY tests bypass the
/// container-only wrapper.
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

#[cfg(not(test))]
pub(crate) fn shell_executable() -> std::ffi::OsString {
    "/bin/zsh".into()
}

#[cfg(test)]
pub(crate) fn shell_executable() -> std::ffi::OsString {
    "/bin/sh".into()
}

/// Apply the stable pane terminal environment. The active outer terminal is
/// reported per attach through the Capsule protocol; pane PTYs keep a
/// conservative baseline so a running session can be reattached from Ghostty,
/// Kitty, iTerm, Warp, or any other xterm-compatible client without retaining
/// assumptions from the terminal that launched the container. `COLORTERM`
/// intentionally advertises jackin❯'s 24-bit color path without tying the pane
/// to a host-specific terminfo entry.
pub(crate) fn apply_terminal_env(cmd: &mut CommandBuilder) {
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    for key in ["LANG", "LC_ALL"] {
        if let Ok(value) = std::env::var(key) {
            cmd.env(key, value);
        }
    }
}

/// Inject only the account selected for this pane — the env of one instance
/// config ID — after ambient credentials were stripped.
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
        // `config::validate_agent_credentials` is the authoritative launch
        // gate. Keep the same closed agent policy here as a second boundary
        // so a hand-built credential envelope cannot inject a foreign
        // provider key even if it bypasses config loading.
        // NOTE: this capsule-side surface string (credential routing) is a
        // different concept from the usage-side `provider_surface()`
        // (quota/discovery ownership); both derive from
        // `HostSurfaceId::from_provider_alias`, so they compose.
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
