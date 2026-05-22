use anyhow::{Result, bail};
use jackin_container::{client, daemon, session::validate_agent_slug};

const DEFAULT_AGENT: &str = "claude";

/// CLI for `jackin-container`.
///
/// Mode is determined by:
/// - PID == 1 → daemon mode (supervisor + multiplexer server)
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
        match subcommand {
            None => client::run_client(None).await,
            Some("--version") | Some("-V") => {
                println!("jackin-container {}", env!("JACKIN_CONTAINER_VERSION"));
                Ok(())
            }
            Some("status") => client::run_status().await,
            Some("new") => {
                let agent = args.get(2).and_then(|raw| match validate_agent_slug(raw) {
                    Ok(s) => Some(s.to_string()),
                    Err(reason) => {
                        eprintln!(
                            "[jackin-container] ignoring agent argv {raw:?}: {reason}; daemon will pick the default"
                        );
                        None
                    }
                });
                client::run_client(agent).await
            }
            Some(other) => {
                bail!(
                    "unknown jackin-container subcommand {other:?} — known: status, new <agent>, --version"
                )
            }
        }
    }
}

/// Resolve the initial agent slug for PID-1 daemon mode. Priority:
/// `JACKIN_AGENT` env (validated, hard-error on invalid because the
/// operator set it explicitly and a silent fallback hides the typo)
/// → positional argv (validated, soft-fall back to `DEFAULT_AGENT`
/// because argv might be a wrapper-passed flag in a future refactor)
/// → `DEFAULT_AGENT`. Both ingress points share `validate_agent_slug`
/// so injection / typo'd flags get the same gate.
fn resolve_initial_agent(args: &[String]) -> Result<String> {
    if let Ok(env_agent) = std::env::var("JACKIN_AGENT") {
        let validated = validate_agent_slug(&env_agent)
            .map_err(|reason| anyhow::anyhow!("JACKIN_AGENT={env_agent:?} rejected: {reason}"))?;
        return Ok(validated.to_string());
    }
    let resolved = args
        .get(1)
        .and_then(|raw| match validate_agent_slug(raw) {
            Ok(s) => Some(s.to_string()),
            Err(reason) => {
                eprintln!(
                    "[jackin-container] ignoring agent argv {raw:?}: {reason}; using default {DEFAULT_AGENT:?}"
                );
                None
            }
        })
        .unwrap_or_else(|| DEFAULT_AGENT.to_string());
    Ok(resolved)
}
