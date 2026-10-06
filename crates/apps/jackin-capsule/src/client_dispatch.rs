// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Client-mode subcommand dispatch for the capsule entrypoint.

use crate::argv::parse_focus_flag;
use crate::usage_relay_proxy;
use anyhow::{Result, bail};
use jackin_capsule::{
    client, config, firewall, mcp_server, output, protocol::attach::SpawnRequest, runtime_setup,
    session::validate_spawn_token_syntax, sudo_provision,
};

pub(crate) async fn run_client_mode(args: &[String]) -> Result<()> {
    let subcommand = args.get(1).map(String::as_str);
    let focus_session = parse_focus_flag(args);
    match subcommand {
        None => client::run_client(None, focus_session).await,
        Some("--version" | "-V") => {
            output::stdout_line(format_args!(
                "jackin-capsule {}",
                env!("JACKIN_CAPSULE_VERSION")
            ));
            Ok(())
        }
        Some("--help" | "-h") => {
            output::stdout_line(format_args!(
                "jackin-capsule {version}

USAGE:
    jackin-capsule [SUBCOMMAND]

SUBCOMMANDS:
    (no subcommand)                Connect to the running multiplexer (client mode)
    new [<agent>]                  Spawn a new agent session (default: shell)
    status                         Print daemon status to stdout
    snapshot                       Write a screen snapshot to stdout
    send <session_id> <text>       Type text into a running session's PTY (verbatim)
    events [--session <id>]        Stream session state/exit/activity events as NDJSON
    attach-proxy                   Relay attach protocol bytes over stdio
    usage accounts                 Print cached account quota rows as JSON
    usage verify                   Verify all provider quota rows are cached and trusted
    protocol-check [--expected-major <major>]  Check the Capsule protocol major
    usage-relay-proxy              Internal scoped usage stdio tunnel
    --focus <session_id>           Connect and focus the given session
    exec <command> [args…]         Run a command with operator-approved on-demand credentials
    mcp-server                     Run the jackin-exec MCP stdio server (spawned by the agent)
    runtime-setup                  First-boot environment setup (run by entrypoint)
    sudo-provision                 Enforce per-profile sudo grant (run as root via docker exec)
    firewall-apply                 Apply the in-container network allowlist
    prepare-commit-msg <file>      Git hook integration

OPTIONS:
    --version, -V                  Print version and exit
    --help, -h                     Print this help and exit

When invoked as PID 1 the binary starts the multiplexer daemon instead of
connecting as a client.",
                version = env!("JACKIN_CAPSULE_VERSION")
            ));
            Ok(())
        }
        Some("status") if args.get(2).map(String::as_str) == Some("explain") => {
            client::run_status_explain(args).await
        }
        Some("status") if args.get(2).map(String::as_str) == Some("capture") => {
            client::run_status_capture(args).await
        }
        Some("protocol-check") => client::run_protocol_check(&args[2..]).await,
        Some("status") => client::run_status().await,
        Some("snapshot") => client::run_snapshot().await,
        Some("send") => client::run_session_send(args).await,
        Some("events") => client::run_session_events(args).await,
        Some("report-event") => client::run_report_event(args).await,
        Some("token-usage") => client::run_token_usage(args).await,
        Some("attach-proxy") => client::run_attach_proxy().await,
        Some("usage") => run_usage_subcommand(args).await,
        Some("usage-relay-proxy") => usage_relay_proxy::run().await,
        Some("agents") => {
            let json_format = args.iter().any(|a| a == "--format=json")
                || args
                    .windows(2)
                    .any(|w| w[0] == "--format" && w[1] == "json");
            let format = if json_format {
                client::AgentsFormat::Json
            } else {
                client::AgentsFormat::Human
            };
            client::run_agents(format).await
        }
        Some("runtime-setup") => runtime_setup::run(),
        Some("mcp-server") => mcp_server::run().await,
        Some("sudo-provision") => sudo_provision::provision(),
        Some("firewall-apply") => firewall::apply(),
        Some("prepare-commit-msg") => runtime_setup::run_prepare_commit_msg_hook(&args[2..]),
        Some("new") => {
            let launch_config = config::load_optional();
            let spawn = match args.get(2) {
                None => Some(SpawnRequest::Shell),
                Some(raw) => {
                    let token = match validate_spawn_token_syntax(raw) {
                        Ok(token) => Some(token),
                        Err(reason) => {
                            output::stderr_line(format_args!(
                                "[jackin-capsule] ignoring agent argv {raw:?}: {reason}; no new session will be spawned"
                            ));
                            None
                        }
                    };
                    // Early client-side resolution for a precise error;
                    // the daemon re-resolves authoritatively at spawn.
                    if let (Some(token), Some(config)) = (token, &launch_config)
                        && let Err(reason) = config.resolve_instance(token)
                    {
                        output::stderr_line(format_args!(
                            "[jackin-capsule] ignoring agent argv {raw:?}: {reason}; no new session will be spawned"
                        ));
                        None
                    } else {
                        match token.map(SpawnRequest::instance) {
                            None => None,
                            Some(Ok(req)) => Some(req),
                            Some(Err(reason)) => {
                                output::stderr_line(format_args!(
                                    "[jackin-capsule] rejecting agent argv {raw:?}: {reason}; no new session will be spawned"
                                ));
                                return client::run_client(None, focus_session).await;
                            }
                        }
                    }
                }
            };
            client::run_client(spawn, focus_session).await
        }
        Some(other) if other.starts_with("--focus") => {
            client::run_client(None, focus_session).await
        }
        Some(other) => {
            bail!(
                "unknown jackin-capsule subcommand {other:?} — known: status, status explain <id>, status capture <id>, snapshot, attach-proxy, usage accounts, usage verify, protocol-check, usage-relay-proxy, token-usage <id>, agents [--format json], report-event --event <name> [--payload-stdin], exec <command>, mcp-server, runtime-setup, sudo-provision, firewall-apply, prepare-commit-msg, new <agent>, --focus <session_id>, --version, --help"
            )
        }
    }
}

async fn run_usage_subcommand(args: &[String]) -> Result<()> {
    match args.get(2).map(String::as_str) {
        Some("accounts") => client::run_usage_accounts().await,
        Some("verify") => client::run_usage_verify().await,
        Some(other) => {
            bail!("unknown usage subcommand {other:?} — known: accounts, verify")
        }
        None => bail!("usage requires a subcommand: accounts or verify"),
    }
}
