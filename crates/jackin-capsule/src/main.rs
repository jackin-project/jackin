use anyhow::{Result, bail};
use jackin_capsule::{
    client, daemon, protocol::attach::SpawnRequest, runtime_setup, session::validate_agent_slug,
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
        let agent = resolve_initial_agent(&args)?;
        daemon::run_daemon(agent).await
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
                let spawn = match args.get(2) {
                    None => Some(SpawnRequest::Shell),
                    Some(raw) => match validate_agent_slug(raw) {
                        Ok(s) => Some(SpawnRequest::Agent(s.to_string())),
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
                // Bare `jackin-capsule --focus <id>` → plain attach
                // with focus.
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
/// Returns `None` when the flag is missing or the value is not a
/// `u64`. Invalid values fall through to "no focus" so a stale or
/// hand-typed id never blocks the attach — the daemon ignores
/// unknown focus targets anyway.
fn parse_focus_flag(args: &[String]) -> Option<u64> {
    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        if let Some(value) = arg.strip_prefix("--focus=") {
            return value.parse::<u64>().ok();
        }
        if arg == "--focus" {
            return iter.next().and_then(|raw| raw.parse::<u64>().ok());
        }
    }
    None
}

/// Resolve the initial agent slug for PID-1 daemon mode. The host launcher
/// passes this as the container command argument after the image name so the
/// container's global environment does not claim one agent for every session.
/// `JACKIN_AGENT` is reserved for per-agent entrypoint processes.
fn resolve_initial_agent(args: &[String]) -> Result<String> {
    let Some(raw) = args.get(1) else {
        return Ok(DEFAULT_AGENT.to_string());
    };
    let validated = validate_agent_slug(raw)
        .map_err(|reason| anyhow::anyhow!("initial agent argv {raw:?} rejected: {reason}"))?;
    Ok(validated.to_string())
}
