use anyhow::{Result, bail};
use jackin_capsule::{
    client, config, daemon, output, protocol::attach::SpawnRequest, runtime_setup,
    session::validate_agent_slug,
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
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if invoked_as_prepare_commit_msg_hook(&args) {
        return runtime_setup::run_prepare_commit_msg_hook(&args[1..]);
    }

    let is_pid1 = std::process::id() == 1;

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
    --focus <session_id>           Connect and focus the given session
    runtime-setup                  First-boot environment setup (run by entrypoint)
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
                    "unknown jackin-capsule subcommand {other:?} — known: status, snapshot, agents [--format json], report-event --event <name> [--payload-stdin], runtime-setup, prepare-commit-msg, new <agent>, --focus <session_id>, --version, --help"
                )
            }
        }
    }
}

fn invoked_as_prepare_commit_msg_hook(args: &[String]) -> bool {
    args.first()
        .and_then(|arg0| Path::new(arg0).file_name())
        .is_some_and(|file_name| file_name == "prepare-commit-msg")
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
            "status" | "snapshot" | "agents" | "runtime-setup" | "prepare-commit-msg" | "--version"
            | "-V" | "--help" | "-h",
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

#[cfg(test)]
mod tests;
