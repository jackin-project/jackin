use anyhow::{Result, bail};
use jackin_capsule::{
    client, config, daemon, protocol::attach::SpawnRequest, runtime_setup,
    session::validate_agent_slug,
};

const DEFAULT_AGENT: &str = "claude";

/// CLI for `jackin-capsule`.
///
/// Mode is determined by:
/// - PID == 1 → daemon mode (supervisor + multiplexer + socket control plane)
/// - PID != 1 → client mode (connect to daemon, run interactive UI)
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
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
            Some("--version") | Some("-V") => {
                println!("jackin-capsule {}", env!("JACKIN_CAPSULE_VERSION"));
                Ok(())
            }
            Some("status") => client::run_status().await,
            Some("snapshot") => client::run_snapshot().await,
            Some("runtime-setup") => runtime_setup::run(),
            Some("new") => {
                let supported_agents = config::load_optional()
                    .map(|config| config.supported_agents())
                    .unwrap_or_default();
                let spawn = match args.get(2) {
                    None => Some(SpawnRequest::Shell),
                    Some(raw) => match validate_agent_slug(raw, &supported_agents) {
                        Ok(s) => match SpawnRequest::agent(s) {
                            Ok(req) => Some(req),
                            Err(reason) => {
                                eprintln!(
                                    "[jackin-capsule] rejecting agent argv {raw:?}: {reason}; no new session will be spawned"
                                );
                                None
                            }
                        },
                        Err(reason) => {
                            eprintln!(
                                "[jackin-capsule] ignoring agent argv {raw:?}: {reason}; no new session will be spawned"
                            );
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
                    "unknown jackin-capsule subcommand {other:?} — known: status, snapshot, runtime-setup, new <agent>, --focus <session_id>, --version"
                )
            }
        }
    }
}

/// Parse `--focus <id>` / `--focus=<id>` out of the client argv.
/// Returns `None` when the flag is missing or the value cannot be
/// parsed as `u64`. A malformed value emits a stderr warning so the
/// operator sees the rejection instead of silently attaching to the
/// daemon-picked default pane.
fn parse_focus_flag(args: &[String]) -> Option<u64> {
    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        if let Some(value) = arg.strip_prefix("--focus=") {
            return match value.parse::<u64>() {
                Ok(n) => Some(n),
                Err(_) => {
                    eprintln!("[jackin-capsule] ignoring --focus={value:?}: not a u64");
                    None
                }
            };
        }
        if arg == "--focus" {
            return iter.next().and_then(|raw| match raw.parse::<u64>() {
                Ok(n) => Some(n),
                Err(_) => {
                    eprintln!("[jackin-capsule] ignoring --focus {raw:?}: not a u64");
                    None
                }
            });
        }
    }
    None
}

/// Resolve the initial agent slug for PID-1 daemon mode. The host launcher
/// passes this as the container command argument after the image name so the
/// container's global environment does not claim one agent for every session.
/// `JACKIN_AGENT` is reserved for per-agent entrypoint processes.
fn resolve_initial_agent(args: &[String], supported_agents: &[String]) -> Result<String> {
    let Some(raw) = args.get(1) else {
        return Ok(DEFAULT_AGENT.to_string());
    };
    let validated = validate_agent_slug(raw, supported_agents)
        .map_err(|reason| anyhow::anyhow!("initial agent argv {raw:?} rejected: {reason}"))?;
    Ok(validated.to_string())
}
