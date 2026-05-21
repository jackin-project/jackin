use anyhow::Result;
use jackin_container::{client, daemon};

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
        let agent = std::env::var("JACKIN_AGENT")
            .unwrap_or_else(|_| args.get(1).cloned().unwrap_or_default());
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
                let agent = args.get(2).cloned().unwrap_or_default();
                client::run_client(Some(agent)).await
            }
            _ => client::run_client(None).await,
        }
    }
}
