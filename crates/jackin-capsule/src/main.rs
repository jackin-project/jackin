//! jackin-capsule: in-container capsule daemon, sessions, and TUI.
//!
//! **Architecture Invariant:** T4.
//! Entry point: [`daemon`] — capsule daemon module the binary runs.

use anyhow::{Result, bail};
use jackin_capsule::{
    client, config, daemon, exec, firewall, mcp_server, output, protocol::attach::SpawnRequest,
    runtime_setup, session::validate_agent_slug, sudo_provision,
};
use std::path::Path;

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

const DEFAULT_AGENT: &str = "claude";

/// CLI for `jackin-capsule`.
///
/// Mode is determined by:
/// - PID == 1 → daemon mode (supervisor + multiplexer + socket control plane)
/// - PID != 1 → client mode (connect to daemon, run interactive UI)
#[tokio::main(flavor = "current_thread")]
#[allow(
    clippy::excessive_nesting,
    reason = "Top-level CLI dispatch match over subcommands, where some arms (notably \
              `new` and `--focus`) nest further matches to validate argv. Each branch \
              is a thin adapter into a client-side runtime call; extracting each arm \
              into its own helper is the deferred-parallel-pass — the inline shape keeps \
              the dispatch table co-located for review."
)]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if invoked_as_prepare_commit_msg_hook(&args) {
        return runtime_setup::run_prepare_commit_msg_hook(&args[1..]);
    }

    // `jackin-exec` (argv0 symlink) and `jackin-capsule exec …` are always
    // client-side credential execs, never the daemon. Check before the daemon
    // gate below so an inherited `JACKIN_CAPSULE_FORCE_DAEMON` in the
    // apple-container VM env cannot capture this invocation into daemon mode.
    if let Some(exec_args) = exec_invocation(&args) {
        return exec::run(exec_args).await;
    }

    // Daemon mode when PID 1 (Docker backend, capsule is the entrypoint) or when
    // the apple-container `JACKIN_CAPSULE_FORCE_DAEMON` marker applies to *this*
    // invocation (see `forced_daemon_mode` — the env is inherited by
    // `container exec` children, so it cannot mark the entrypoint on its own).
    let is_pid1 = std::process::id() == 1 || forced_daemon_mode(&args);

    if is_pid1 {
        let launch_config = config::load()?;
        let supported_agents = launch_config.supported_agents();
        let agent = resolve_initial_agent(&args, &supported_agents)?;
        daemon::run_daemon(agent, launch_config).await
    } else {
        let subcommand = args.get(1).map(String::as_str);
        let focus_session = parse_focus_flag(&args);
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
    attach-proxy                   Relay attach protocol bytes over stdio
    usage accounts                 Print cached account quota rows as JSON
    usage verify                   Verify all provider quota rows are cached and trusted
    usage claude-cli               Explicitly run Claude Code /usage diagnostic
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
                client::run_status_explain(&args).await
            }
            Some("status") if args.get(2).map(String::as_str) == Some("capture") => {
                client::run_status_capture(&args).await
            }
            Some("status") => client::run_status().await,
            Some("snapshot") => client::run_snapshot().await,
            Some("report-event") => client::run_report_event(&args).await,
            Some("token-usage") => client::run_token_usage(&args).await,
            Some("attach-proxy") => client::run_attach_proxy().await,
            Some("usage") => run_usage_subcommand(&args).await,
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
                let supported_agents = config::load_optional()
                    .map(|config| config.supported_agents())
                    .unwrap_or_default();
                let provider_label = parse_provider_flag(&args);
                let spawn = match args.get(2) {
                    None => Some(SpawnRequest::Shell),
                    Some(raw) => match validate_agent_slug(raw, &supported_agents) {
                        Ok(slug) => {
                            let req = if let Some(label) = provider_label {
                                SpawnRequest::AgentWithProvider {
                                    slug: slug.to_owned(),
                                    provider_label: label,
                                }
                            } else {
                                match SpawnRequest::agent(slug) {
                                    Ok(req) => req,
                                    Err(reason) => {
                                        output::stderr_line(format_args!(
                                            "[jackin-capsule] rejecting agent argv {raw:?}: {reason}; no new session will be spawned"
                                        ));
                                        return client::run_client(None, focus_session).await;
                                    }
                                }
                            };
                            Some(req)
                        }
                        Err(reason) => {
                            output::stderr_line(format_args!(
                                "[jackin-capsule] ignoring agent argv {raw:?}: {reason}; no new session will be spawned"
                            ));
                            None
                        }
                    },
                };
                client::run_client(spawn, focus_session).await
            }
            Some(other) if other.starts_with("--focus") => {
                client::run_client(None, focus_session).await
            }
            Some(other) => {
                bail!(
                    "unknown jackin-capsule subcommand {other:?} — known: status, status explain <id>, status capture <id>, snapshot, attach-proxy, usage accounts, usage verify, usage claude-cli, token-usage <id>, agents [--format json], report-event --event <name> [--payload-stdin], exec <command>, mcp-server, runtime-setup, sudo-provision, firewall-apply, prepare-commit-msg, new <agent>, --focus <session_id>, --version, --help"
                )
            }
        }
    }
}

async fn run_usage_subcommand(args: &[String]) -> Result<()> {
    match args.get(2).map(String::as_str) {
        Some("accounts") => client::run_usage_accounts().await,
        Some("verify") => client::run_usage_verify().await,
        Some("claude-cli") => client::run_usage_claude_cli(),
        Some(other) => {
            bail!("unknown usage subcommand {other:?} — known: accounts, verify, claude-cli")
        }
        None => bail!("usage requires a subcommand: accounts, verify, or claude-cli"),
    }
}

fn invoked_as_prepare_commit_msg_hook(args: &[String]) -> bool {
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
fn forced_daemon_mode(args: &[String]) -> bool {
    std::env::var_os("JACKIN_CAPSULE_FORCE_DAEMON").is_some() && is_daemon_entrypoint_args(args)
}

/// The argv-shape half of [`forced_daemon_mode`], split out so the
/// entrypoint-vs-client classification is unit-testable without touching the
/// process environment. `true` only for the initial-agent-slug entrypoint form.
fn is_daemon_entrypoint_args(args: &[String]) -> bool {
    match args.get(1).map(String::as_str) {
        // Bare `jackin-capsule` (attach) or `--focus N` (attach) → client.
        None => false,
        Some(focus) if focus.starts_with("--focus") => false,
        // Known client subcommands → client (must keep this list in sync with
        // the client-mode dispatch match in `main`).
        Some(
            "status" | "snapshot" | "usage" | "agents" | "runtime-setup" | "mcp-server"
            | "prepare-commit-msg" | "new" | "--version" | "-V" | "--help" | "-h",
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
fn exec_invocation(args: &[String]) -> Option<&[String]> {
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
fn parse_focus_flag(args: &[String]) -> Option<u64> {
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
            "status" | "snapshot" | "attach-proxy" | "usage" | "agents" | "runtime-setup"
            | "mcp-server" | "prepare-commit-msg" | "sudo-provision" | "firewall-apply"
            | "--version" | "-V" | "--help" | "-h",
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

/// Extract the `--provider=<label>` flag from a `new <agent> --provider=…`
/// argv. Scans past the subcommand (index 1) and its agent positional
/// (index 2). An empty `--provider=` yields `Some("")`, which the daemon
/// routes through its unknown-provider fallback (no env redirect).
fn parse_provider_flag(args: &[String]) -> Option<String> {
    args.get(3..)?
        .iter()
        .find_map(|arg| arg.strip_prefix("--provider=").map(str::to_owned))
}

/// Resolve the initial agent slug for PID-1 daemon mode. The host launcher
/// passes this as the container command argument after the image name so the
/// container's global environment does not claim one agent for every session.
/// `JACKIN_AGENT` is reserved for per-agent entrypoint processes.
fn resolve_initial_agent(args: &[String], supported_agents: &[String]) -> Result<String> {
    let Some(raw) = args.get(1) else {
        return Ok(DEFAULT_AGENT.to_owned());
    };
    let validated = validate_agent_slug(raw, supported_agents)
        .map_err(|reason| anyhow::anyhow!("initial agent argv {raw:?} rejected: {reason}"))?;
    Ok(validated.to_owned())
}
