// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Argv classification helpers for the capsule entrypoint.

use anyhow::Result;
use jackin_capsule::{output, session::validate_spawn_token_syntax};
use std::path::Path;

pub(crate) const DEFAULT_AGENT: &str = "claude";

pub(crate) fn invoked_as_prepare_commit_msg_hook(args: &[String]) -> bool {
    args.first()
        .and_then(|arg0| Path::new(arg0).file_name())
        .is_some_and(|file_name| file_name == "prepare-commit-msg")
}

/// Whether `JACKIN_CAPSULE_FORCE_DAEMON` should put this invocation into daemon
/// mode (apple-container backend, where `vminitd` is PID 1 and the entrypoint
/// capsule runs at PID 2+).
///
/// The env var is set via `container run`, so every `container exec` child
/// (attach, `status`, `mcp-server`, `snapshot`, …) inherits it — the env alone
/// cannot mark the entrypoint without also hijacking those client invocations.
/// The entrypoint is the only form invoked with the initial agent slug as
/// `argv[1]`; every client form is bare (attach), a `--focus` flag, or a known
/// subcommand. So only the agent-slug form daemonizes.
///
/// (The capsule-not-PID-1 path itself is gated on apple-container Phase 0
/// validation — see the apple-container roadmap item. The Docker backend never
/// sets this env and stays on the PID-1 check.)
pub(crate) fn forced_daemon_mode(args: &[String]) -> bool {
    std::env::var_os("JACKIN_CAPSULE_FORCE_DAEMON").is_some() && is_daemon_entrypoint_args(args)
}

/// The argv-shape half of [`forced_daemon_mode`], split out so the
/// entrypoint-vs-client classification is unit-testable without touching the
/// process environment. `true` only for the initial-agent-slug entrypoint form.
pub(crate) fn is_daemon_entrypoint_args(args: &[String]) -> bool {
    match args.get(1).map(String::as_str) {
        // Bare `jackin-capsule` (attach) or `--focus N` (attach) → client.
        None => false,
        Some(focus) if focus.starts_with("--focus") => false,
        // Known client subcommands → client (must keep this list in sync with
        // the client-mode dispatch match in `main`).
        Some(
            "status" | "snapshot" | "send" | "events" | "usage" | "agents" | "runtime-setup"
            | "mcp-server" | "prepare-commit-msg" | "new" | "usage-relay-proxy" | "protocol-check"
            | "--version" | "-V" | "--help" | "-h",
        ) => false,
        // Anything else is the initial agent slug → daemon entrypoint.
        Some(_) => true,
    }
}

/// Detect a `jackin-exec` invocation and return the command + its args.
///
/// Two forms route here: the `jackin-exec` argv0 symlink
/// (`jackin-exec ssh sentry` → `["ssh", "sentry"]`) and the explicit
/// `jackin-capsule exec ssh sentry` subcommand (→ `["ssh", "sentry"]`).
/// Returns `None` for any other invocation.
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

/// Parse `--focus <id>` / `--focus=<id>` out of the client argv.
/// Returns `None` when the flag is missing or the value cannot be
/// parsed as `u64`. A malformed value emits a stderr warning so the
/// operator sees the rejection instead of silently attaching to the
/// daemon-picked default pane.
///
/// Scope: scans from the first arg AFTER the subcommand consumes its
/// positional. Without this, `jackin-capsule new --focus 5` (the user
/// typo'd `new` in front of an intended `--focus 5`) would silently
/// match `--focus` as if `--focus` were a global flag, attach to
/// session 5, AND spawn an extra Shell because `new` with no agent
/// defaults to Shell. The fix is to start the scan at the index where
/// the subcommand's own arguments end.
pub(crate) fn parse_focus_flag(args: &[String]) -> Option<u64> {
    let scan_start = match args.get(1).map(String::as_str) {
        // `new [<agent>]` consumes index 2 as its positional. The
        // global --focus only applies when it appears past the
        // subcommand's own positional — otherwise `new --focus 5`
        // (the typo the original report names) would silently
        // succeed as "spawn shell + jump to session 5".
        Some("new") => 3,
        // Subcommands that take no positional and never accept
        // --focus. Scan past the end of args so a stray --focus is
        // ignored instead of silently consumed.
        Some(
            "status" | "snapshot" | "send" | "events" | "attach-proxy" | "usage" | "agents"
            | "runtime-setup" | "mcp-server" | "prepare-commit-msg" | "sudo-provision"
            | "firewall-apply" | "usage-relay-proxy" | "--version" | "-V" | "--help" | "-h",
        ) => args.len(),
        // `jackin-capsule --focus 5` (no subcommand) or no args at
        // all — scan from index 1.
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

/// Resolve the initial instance for PID-1 daemon mode. The host launcher
/// passes this as the container command argument after the image name so the
/// container's global environment does not claim one agent for every session.
/// `JACKIN_AGENT` is reserved for per-agent entrypoint processes.
/// Accepts an instance config ID or, when unambiguous, an agent slug.
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
