use anyhow::{Result, bail};
use jackin_capsule::{
    client, config, daemon, output, protocol::attach::SpawnRequest, runtime_setup,
    session::validate_agent_slug, socket,
};
use std::path::Path;
use tokio::io::AsyncWriteExt as _;
use tokio::net::UnixStream;

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
    match rustls::crypto::ring::default_provider().install_default() {
        Ok(()) | Err(_) => {}
    }

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
    status explain <session_id>    Print agent-status evidence as JSON
    status capture <session_id>    Capture status evidence under /jackin/state/
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
            Some("status") => match args.get(2).map(String::as_str) {
                Some("explain") => {
                    let session_id = parse_session_id_arg(&args, 3, "status explain")?;
                    client::run_status_explain(session_id).await
                }
                Some("capture") => {
                    let session_id = parse_session_id_arg(&args, 3, "status capture")?;
                    client::run_status_capture(session_id).await
                }
                Some(other) => bail!(
                    "unknown status subcommand {other:?} — known: explain <session_id>, capture <session_id>"
                ),
                None => client::run_status().await,
            },
            Some("snapshot") => client::run_snapshot().await,
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
            Some("report-event") => run_report_event(&args[2..]).await,
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
                    "unknown jackin-capsule subcommand {other:?} — known: status [explain|capture], snapshot, agents [--format json], report-event, runtime-setup, prepare-commit-msg, new <agent>, --focus <session_id>, --version, --help"
                )
            }
        }
    }
}

fn parse_session_id_arg(args: &[String], index: usize, command: &str) -> Result<u64> {
    let Some(raw) = args.get(index) else {
        bail!("{command} requires a session_id");
    };
    raw.parse::<u64>()
        .map_err(|_| anyhow::anyhow!("{command} session_id must be a u64, got {raw:?}"))
}

async fn run_report_event(args: &[String]) -> Result<()> {
    let payload = if args.iter().any(|arg| arg == "--payload-stdin") {
        let mut input = String::new();
        if std::io::Read::read_to_string(&mut std::io::stdin(), &mut input).is_ok()
            && !input.trim().is_empty()
        {
            serde_json::from_str::<serde_json::Value>(&input).ok()
        } else {
            None
        }
    } else {
        None
    };
    let event = report_event_name(args, payload.as_ref());
    let (Ok(session_id), Ok(source_id), Ok(runtime)) = (
        std::env::var("JACKIN_SESSION_ID").and_then(|value| {
            value
                .parse::<u64>()
                .map_err(|_| std::env::VarError::NotPresent)
        }),
        std::env::var("JACKIN_STATUS_SOURCE"),
        std::env::var("JACKIN_AGENT_RUNTIME"),
    ) else {
        return Ok(());
    };
    let socket_path =
        std::env::var("JACKIN_STATUS_SOCKET").unwrap_or_else(|_| socket::SOCKET_PATH.to_owned());
    let msg = jackin_capsule::protocol::control::ClientMsg::ReportRuntimeEvent {
        session_id,
        source_id,
        runtime,
        event,
        payload,
    };
    if let Ok(mut stream) = UnixStream::connect(socket_path).await {
        let _write_result = stream
            .write_all(&jackin_capsule::protocol::control::frame(&msg))
            .await;
    }
    Ok(())
}

fn report_event_name(args: &[String], payload: Option<&serde_json::Value>) -> String {
    let event = parse_named_arg(args, "--event")
        .or_else(|| {
            payload
                .and_then(|payload| payload.get("hook_event_name"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "heartbeat".to_owned());
    if event == "Notification" {
        payload
            .and_then(|payload| payload.get("notification_type"))
            .and_then(serde_json::Value::as_str)
            .map(|kind| format!("Notification:{kind}"))
            .unwrap_or(event)
    } else {
        event
    }
}

fn parse_named_arg(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == name)
        .and_then(|index| args.get(index + 1))
        .cloned()
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
mod tests {
    use super::*;

    fn args(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn report_event_name_uses_explicit_event() {
        assert_eq!(
            report_event_name(&args(&["--event", "PreToolUse"]), None),
            "PreToolUse"
        );
    }

    #[test]
    fn report_event_name_extracts_claude_notification_type() {
        let payload = serde_json::json!({
            "hook_event_name": "Notification",
            "notification_type": "permission_prompt",
        });
        assert_eq!(
            report_event_name(&args(&["--event", "Notification"]), Some(&payload)),
            "Notification:permission_prompt"
        );
    }

    #[test]
    fn report_event_name_extracts_payload_hook_event() {
        let payload = serde_json::json!({
            "hook_event_name": "SessionEnd",
        });
        assert_eq!(report_event_name(&[], Some(&payload)), "SessionEnd");
    }

    #[test]
    fn report_event_name_falls_back_to_heartbeat() {
        assert_eq!(report_event_name(&[], None), "heartbeat");
    }

    #[test]
    fn parse_focus_flag_no_subcommand_finds_global_flag() {
        // Bare client mode: `jackin-capsule --focus 5` must resolve to
        // session 5 — the original use case the flag was added for.
        assert_eq!(
            parse_focus_flag(&args(&["jackin-capsule", "--focus", "5"])),
            Some(5)
        );
        assert_eq!(
            parse_focus_flag(&args(&["jackin-capsule", "--focus=7"])),
            Some(7)
        );
    }

    #[test]
    fn parse_focus_flag_new_with_agent_finds_trailing_focus() {
        // `new <agent> --focus N` is a legitimate combination — spawn
        // the agent AND switch focus to N once the daemon answers.
        assert_eq!(
            parse_focus_flag(&args(&["jackin-capsule", "new", "claude", "--focus", "9"])),
            Some(9)
        );
    }

    #[test]
    fn parse_focus_flag_new_without_agent_ignores_focus() {
        // `new --focus 5` is the typo this regression guards against.
        // Without scoping, --focus at index 2 (where the agent slug
        // would belong) would silently route the operator to session 5
        // AND spawn a default Shell because validate_agent_slug rejects
        // "--focus" as an agent. After the scope fix, --focus at index
        // 2 is treated as a malformed agent argument; focus stays None.
        assert_eq!(
            parse_focus_flag(&args(&["jackin-capsule", "new", "--focus", "5"])),
            None
        );
        assert_eq!(
            parse_focus_flag(&args(&["jackin-capsule", "new", "--focus=5"])),
            None
        );
    }

    #[test]
    fn parse_focus_flag_other_subcommands_ignore_focus_positional() {
        // status/snapshot/runtime-setup take no arguments at all; any
        // --focus after them is residual.
        assert_eq!(
            parse_focus_flag(&args(&["jackin-capsule", "status", "--focus", "5"])),
            None
        );
    }

    #[test]
    fn parse_provider_flag_extracts_label_after_agent() {
        assert_eq!(
            parse_provider_flag(&args(&[
                "jackin-capsule",
                "new",
                "claude",
                "--provider=Z.AI"
            ])),
            Some("Z.AI".to_owned())
        );
    }

    #[test]
    fn parse_provider_flag_absent_or_no_agent_is_none() {
        assert_eq!(
            parse_provider_flag(&args(&["jackin-capsule", "new", "claude"])),
            None
        );
        // No agent positional → nothing at index 3+ to scan.
        assert_eq!(parse_provider_flag(&args(&["jackin-capsule", "new"])), None);
    }

    #[test]
    fn parse_provider_flag_empty_value_is_empty_label() {
        // The daemon treats an empty label as an unknown provider (no redirect).
        assert_eq!(
            parse_provider_flag(&args(&["jackin-capsule", "new", "claude", "--provider="])),
            Some(String::new())
        );
    }

    #[test]
    fn hook_invocation_detects_symlink_name() {
        assert!(invoked_as_prepare_commit_msg_hook(&args(&[
            "/jackin/state/git-hooks/prepare-commit-msg",
            ".git/COMMIT_EDITMSG",
        ])));
        assert!(!invoked_as_prepare_commit_msg_hook(&args(&[
            "/jackin/runtime/jackin-capsule",
            "prepare-commit-msg",
            ".git/COMMIT_EDITMSG",
        ])));
    }
}
