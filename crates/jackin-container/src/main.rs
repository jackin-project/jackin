use anyhow::Result;
use jackin_container::{client, daemon, session};

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
        // When `JACKIN_AGENT` is unset, take the positional arg as the
        // agent slug — but validate so `docker exec ... jackin-container
        // --debug` does not silently set `JACKIN_AGENT=--debug`. Empty
        // or rejected positional values fall back to the default so the
        // derived image's entrypoint always has something to run.
        let agent = std::env::var("JACKIN_AGENT").unwrap_or_else(|_| {
            args.get(1)
                .and_then(|raw| match validate_agent_slug(raw) {
                    Ok(s) => Some(s.to_string()),
                    Err(reason) => {
                        eprintln!(
                            "[jackin-container] ignoring agent argv {raw:?}: {reason}; using default {DEFAULT_AGENT:?}"
                        );
                        None
                    }
                })
                .unwrap_or_else(|| DEFAULT_AGENT.to_string())
        });
        daemon::run_daemon(agent).await
    } else {
        let subcommand = args.get(1).map(String::as_str);
        match subcommand {
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
            _ => client::run_client(None).await,
        }
    }
}

/// Reject argv values that are flags (start with `-`), empty, contain
/// whitespace, or — when the derived image set `JACKIN_SUPPORTED_AGENTS`
/// — do not appear in that allowlist.
fn validate_agent_slug(raw: &str) -> Result<&str, &'static str> {
    if raw.is_empty() {
        return Err("empty value");
    }
    if raw.starts_with('-') {
        return Err("looks like a flag");
    }
    if raw.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("contains whitespace or control characters");
    }
    let supported = session::available_agents();
    if !supported.is_empty() && !supported.iter().any(|a| a == raw) {
        return Err("not in JACKIN_SUPPORTED_AGENTS allowlist");
    }
    Ok(raw)
}
