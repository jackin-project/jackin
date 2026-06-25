use anyhow::{Result, bail};
use jackin_capsule::{
    client, config, daemon, firewall, output, protocol::attach::SpawnRequest, runtime_setup,
    session::validate_agent_slug, sudo_provision,
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
    usage accounts                 Print cached account quota rows as JSON
    usage verify                   Verify all provider quota rows are cached and trusted
    usage claude-cli               Explicitly run Claude Code /usage diagnostic
    --focus <session_id>           Connect and focus the given session
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
            Some("status") => client::run_status().await,
            Some("snapshot") => client::run_snapshot().await,
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
                    "unknown jackin-capsule subcommand {other:?} — known: status, snapshot, usage accounts, usage verify, usage claude-cli, agents [--format json], runtime-setup, sudo-provision, firewall-apply, prepare-commit-msg, new <agent>, --focus <session_id>, --version, --help"
                )
            }
        }
    }
}

async fn run_usage_subcommand(args: &[String]) -> Result<()> {
    match args.get(2).map(String::as_str) {
        Some("accounts") => client::run_usage_accounts().await,
        Some("verify") => client::run_usage_verify().await,
        Some("claude-cli") => client::run_usage_claude_cli().await,
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
            "status" | "snapshot" | "usage" | "agents" | "runtime-setup" | "prepare-commit-msg"
            | "sudo-provision" | "firewall-apply" | "--version" | "-V" | "--help" | "-h",
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
        assert_eq!(
            parse_focus_flag(&args(&[
                "jackin-capsule",
                "usage",
                "session",
                "5",
                "--focus",
                "7",
            ])),
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
