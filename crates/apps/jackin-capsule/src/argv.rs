// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use jackin_capsule::{output, session::validate_spawn_token_syntax};
use std::path::Path;

pub(crate) const DEFAULT_AGENT: &str = "claude";

pub(crate) fn invoked_as_prepare_commit_msg_hook(args: &[String]) -> bool {
    args.first()
        .and_then(|arg0| Path::new(arg0).file_name())
        .is_some_and(|file_name| file_name == "prepare-commit-msg")
}

pub(crate) fn forced_daemon_mode(args: &[String]) -> bool {
    std::env::var_os("JACKIN_CAPSULE_FORCE_DAEMON").is_some() && is_daemon_entrypoint_args(args)
}

pub(crate) fn is_daemon_entrypoint_args(args: &[String]) -> bool {
    match args.get(1).map(String::as_str) {
        None => false,
        Some(focus) if focus.starts_with("--focus") => false,
        Some(
            "status" | "snapshot" | "send" | "events" | "usage" | "agents" | "runtime-setup"
            | "mcp-server" | "prepare-commit-msg" | "new" | "usage-relay-proxy" | "protocol-check"
            | "--version" | "-V" | "--help" | "-h",
        ) => false,
        Some(_) => true,
    }
}

pub(crate) fn exec_invocation(args: &[String]) -> Option<&[String]> {
    let invoked_as_symlink = args
        .first()
        .and_then(|arg0| Path::new(arg0).file_name())
        .is_some_and(|file_name| file_name == "jackin-exec");
    if invoked_as_symlink {
        return Some(&args[1..]);
    }
    if args.get(1).map(String::as_str) == Some("exec") {
        return Some(&args[2..]);
    }
    None
}

pub(crate) fn parse_focus_flag(args: &[String]) -> Option<u64> {
    let scan_start = match args.get(1).map(String::as_str) {
        Some("new") => 3,
        Some(
            "status" | "snapshot" | "send" | "events" | "attach-proxy" | "usage" | "agents"
            | "runtime-setup" | "mcp-server" | "prepare-commit-msg" | "sudo-provision"
            | "firewall-apply" | "usage-relay-proxy" | "--version" | "-V" | "--help" | "-h",
        ) => args.len(),
        _ => 1,
    };
    let mut iter = args.iter().skip(scan_start);
    while let Some(arg) = iter.next() {
        if let Some(value) = arg.strip_prefix("--focus=") {
            return if let Ok(n) = value.parse::<u64>() {
                Some(n)
            } else {
                output::stderr_line(format_args!(
                    "[jackin-capsule] ignoring --focus={value:?}: not a u64"
                ));
                None
            };
        }
        if arg == "--focus" {
            return iter.next().and_then(|raw| {
                if let Ok(n) = raw.parse::<u64>() {
                    Some(n)
                } else {
                    output::stderr_line(format_args!(
                        "[jackin-capsule] ignoring --focus {raw:?}: not a u64"
                    ));
                    None
                }
            });
        }
    }
    None
}

pub(crate) fn resolve_initial_agent(
    args: &[String],
    launch_config: &jackin_protocol::CapsuleConfig,
) -> Result<String> {
    let Some(raw) = args.get(1) else {
        return launch_config
            .resolve_instance(DEFAULT_AGENT)
            .map(str::to_owned)
            .map_err(|reason| anyhow::anyhow!("default initial instance rejected: {reason}"));
    };
    let token = validate_spawn_token_syntax(raw)
        .map_err(|reason| anyhow::anyhow!("initial agent argv {raw:?} rejected: {reason}"))?;
    launch_config
        .resolve_instance(token)
        .map(str::to_owned)
        .map_err(|reason| anyhow::anyhow!("initial agent argv {raw:?} rejected: {reason}"))
}
