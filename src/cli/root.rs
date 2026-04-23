use clap::{Parser, Subcommand};

use super::config::ConfigCommand;
use super::workspace::WorkspaceCommand;
use super::{BANNER, HELP_STYLES};

/// Operator's CLI for orchestrating AI coding agents in isolated containers
#[derive(Debug, Parser)]
#[command(name = "jackin", version = env!("JACKIN_VERSION"), styles = HELP_STYLES, before_help = BANNER)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum Command {
    /// Jack an agent into an isolated container
    ///
    /// TARGET can be a path (~/Projects/my-app), a path with container
    /// destination (~/Projects/my-app:/app), or a saved workspace name.
    /// When omitted, the current directory is used.
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin load                                          # use workspace + last agent for cwd
  jackin load --rebuild                                # same, with fresh Claude install
  jackin load agent-smith
  jackin load agent-smith ~/Projects/my-app
  jackin load agent-smith ~/Projects/my-app:/app
  jackin load agent-smith big-monorepo
  jackin load agent-smith big-monorepo --mount ~/extra-data
  jackin load agent-smith ~/app --mount ~/cache:/cache:ro"
    )]
    Load {
        /// Agent class selector (e.g. `agent-smith`, `chainargos/agent-brown`).
        /// When omitted, uses the last-used or default agent for the workspace.
        selector: Option<String>,
        /// Path, `path:container-dest`, or saved workspace name
        #[arg(value_name = "TARGET")]
        target: Option<String>,
        /// Additional bind-mount spec as `path[:ro]` or `src:dst[:ro]` (repeatable)
        #[arg(long = "mount")]
        mounts: Vec<String>,
        /// Force rebuild the Docker image (updates Claude to latest version)
        #[arg(long, default_value_t = false)]
        rebuild: bool,
        /// Skip the animated intro sequence
        #[arg(long, default_value_t = false)]
        no_intro: bool,
        /// Print raw container output for troubleshooting
        #[arg(long, default_value_t = false)]
        debug: bool,
    },
    /// Reattach to a running agent's session
    ///
    /// When omitted, finds the saved workspace for the current directory and
    /// reconnects to a running agent container belonging to it.
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin hardline                              # auto-detect workspace + running agent for cwd
  jackin hardline agent-smith
  jackin hardline chainargos/the-architect
  jackin hardline jackin-agent-smith-clone-1"
    )]
    Hardline {
        /// Agent class selector or container name to reconnect to.
        /// When omitted, uses the running agent in the workspace for the current directory.
        selector: Option<String>,
    },
    /// Stop an agent and clean up its container
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin eject agent-smith
  jackin eject agent-smith --all
  jackin eject agent-smith --purge
  jackin eject jackin-agent-smith-clone-1"
    )]
    Eject {
        /// Agent class selector or container name to stop
        selector: String,
        /// Stop every running instance of this agent class
        #[arg(long)]
        all: bool,
        /// Also delete persisted state after stopping
        #[arg(long)]
        purge: bool,
    },
    /// Pull every running agent out at once
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Exile,
    /// Delete persisted state for an agent class
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin purge agent-smith
  jackin purge agent-smith --all
  jackin purge chainargos/the-architect"
    )]
    Purge {
        /// Agent class selector (e.g. `agent-smith`, `chainargos/agent-brown`)
        selector: String,
        /// Delete state for every instance, not just the default
        #[arg(long)]
        all: bool,
    },
    /// Open the interactive TUI launcher to pick a workspace and agent
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Launch {
        /// Print raw container output for troubleshooting
        #[arg(long, default_value_t = false)]
        debug: bool,
    },
    /// Manage saved workspaces
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Workspace {
        #[command(subcommand)]
        command: WorkspaceCommand,
    },
    /// View and modify operator configuration
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_load_command() {
        let cli = Cli::try_parse_from(["jackin", "load", "agent-smith"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Load {
                selector: Some(ref s),
                target: None,
                no_intro: false,
                debug: false,
                ..
            } if s == "agent-smith"
        ));
    }

    #[test]
    fn parses_load_without_selector() {
        let cli = Cli::try_parse_from(["jackin", "load"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Load {
                selector: None,
                target: None,
                ..
            }
        ));
    }

    #[test]
    fn parses_load_rebuild_without_selector() {
        let cli = Cli::try_parse_from(["jackin", "load", "--rebuild"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Load {
                selector: None,
                rebuild: true,
                ..
            }
        ));
    }

    #[test]
    fn parses_load_with_target_path() {
        let cli =
            Cli::try_parse_from(["jackin", "load", "agent-smith", "~/Projects/my-app"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Load {
                target: Some(ref t),
                ..
            } if t == "~/Projects/my-app"
        ));
    }

    #[test]
    fn parses_load_with_target_and_mount() {
        let cli = Cli::try_parse_from([
            "jackin",
            "load",
            "agent-smith",
            "big-monorepo",
            "--mount",
            "/tmp/cache:/workspace/cache:ro",
        ])
        .unwrap();

        assert!(matches!(
            cli.command,
            Command::Load {
                target: Some(ref t),
                ref mounts,
                ..
            } if t == "big-monorepo" && mounts.len() == 1
        ));
    }

    #[test]
    fn parses_load_with_mount_only() {
        let cli = Cli::try_parse_from([
            "jackin",
            "load",
            "agent-smith",
            "--mount",
            "/tmp/project:/workspace/project",
            "--mount",
            "/tmp/cache:/workspace/cache:ro",
        ])
        .unwrap();

        assert!(matches!(
            cli.command,
            Command::Load {
                target: None,
                ref mounts,
                ..
            } if mounts.len() == 2
        ));
    }

    #[test]
    fn parses_launch_command() {
        let cli = Cli::try_parse_from(["jackin", "launch"]).unwrap();
        assert!(matches!(cli.command, Command::Launch { debug: false }));
    }

    /// Strip ANSI escape sequences for clean test assertions.
    fn strip_ansi(s: &str) -> String {
        let mut result = String::with_capacity(s.len());
        let mut chars = s.chars();
        while let Some(ch) = chars.next() {
            if ch == '\x1b' {
                // Skip until 'm' (SGR) or other terminator
                for inner in chars.by_ref() {
                    if inner.is_ascii_alphabetic() {
                        break;
                    }
                }
            } else {
                result.push(ch);
            }
        }
        result
    }

    fn help_text(args: &[&str]) -> String {
        let err = Cli::try_parse_from(args).unwrap_err();
        strip_ansi(&err.to_string())
    }

    // ── Banner tests ────────────────────────────────────────────────────

    #[test]
    fn root_help_shows_banner_with_top_padding() {
        let help = help_text(&["jackin", "--help"]);
        // Banner should have an empty line before the circuit art (top padding)
        let circuit_line = "│ │";
        let banner_pos = help.find(circuit_line).expect("circuit art missing");
        let before_banner = &help[..banner_pos];
        let newline_count = before_banner.chars().filter(|&c| c == '\n').count();
        assert!(
            newline_count >= 2,
            "banner missing top padding (expected >=2 newlines, got {newline_count}): {before_banner:?}"
        );
        assert!(help.contains("j a c k i n"), "banner text missing");
        assert!(help.contains("operator terminal"), "banner tagline missing");
    }

    #[test]
    fn root_help_shows_all_commands() {
        let help = help_text(&["jackin", "--help"]);
        assert!(
            help.contains(
                "Operator's CLI for orchestrating AI coding agents in isolated containers"
            )
        );
        for cmd in [
            "load",
            "hardline",
            "eject",
            "exile",
            "purge",
            "launch",
            "workspace",
            "config",
        ] {
            assert!(help.contains(cmd), "missing command: {cmd}");
        }
    }

    // ── Load help ───────────────────────────────────────────────────────

    #[test]
    fn load_help_shows_description_and_examples() {
        let help = help_text(&["jackin", "load", "--help"]);
        assert!(help.contains("Jack an agent into an isolated container"));
        assert!(help.contains("Examples:"));
        assert!(help.contains("jackin load agent-smith"));
        assert!(help.contains("jackin load agent-smith big-monorepo"));
    }

    #[test]
    fn load_help_shows_mount_format() {
        let help = help_text(&["jackin", "load", "--help"]);
        assert!(
            help.contains("path[:ro]") && help.contains("src:dst[:ro]"),
            "mount format missing"
        );
    }

    // ── Hardline help ───────────────────────────────────────────────────

    #[test]
    fn hardline_help_shows_examples() {
        let help = help_text(&["jackin", "hardline", "--help"]);
        assert!(help.contains("Reattach to a running agent"));
        assert!(help.contains("jackin hardline agent-smith"));
        assert!(
            help.contains("jackin hardline ") && help.contains("auto-detect workspace"),
            "missing no-arg usage in hardline help: {help}"
        );
    }

    #[test]
    fn parses_hardline_without_selector() {
        let cli = Cli::try_parse_from(["jackin", "hardline"]).unwrap();
        assert!(matches!(cli.command, Command::Hardline { selector: None }));
    }

    #[test]
    fn parses_hardline_with_selector() {
        let cli = Cli::try_parse_from(["jackin", "hardline", "agent-smith"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Hardline { selector: Some(ref s) } if s == "agent-smith"
        ));
    }

    // ── Eject help ──────────────────────────────────────────────────────

    #[test]
    fn eject_help_shows_examples() {
        let help = help_text(&["jackin", "eject", "--help"]);
        assert!(help.contains("Stop an agent and clean up its container"));
        assert!(help.contains("jackin eject agent-smith --all"));
        assert!(help.contains("jackin eject agent-smith --purge"));
    }

    // ── Purge help ──────────────────────────────────────────────────────

    #[test]
    fn purge_help_shows_examples() {
        let help = help_text(&["jackin", "purge", "--help"]);
        assert!(help.contains("Delete persisted state"));
        assert!(help.contains("jackin purge agent-smith --all"));
    }

    // ── Subcommand banner consistency ───────────────────────────────────

    #[test]
    fn all_subcommand_help_pages_show_banner() {
        let subcommands = [
            vec!["jackin", "load", "--help"],
            vec!["jackin", "hardline", "--help"],
            vec!["jackin", "eject", "--help"],
            vec!["jackin", "exile", "--help"],
            vec!["jackin", "purge", "--help"],
            vec!["jackin", "launch", "--help"],
            vec!["jackin", "workspace", "create", "--help"],
            vec!["jackin", "workspace", "list", "--help"],
            vec!["jackin", "workspace", "show", "--help"],
            vec!["jackin", "workspace", "edit", "--help"],
            vec!["jackin", "workspace", "remove", "--help"],
            vec!["jackin", "config", "mount", "add", "--help"],
            vec!["jackin", "config", "mount", "remove", "--help"],
            vec!["jackin", "config", "mount", "list", "--help"],
            vec!["jackin", "config", "auth", "set", "--help"],
            vec!["jackin", "config", "auth", "show", "--help"],
        ];
        for args in &subcommands {
            let help = help_text(args);
            assert!(
                help.contains("j a c k i n"),
                "banner missing in: {}",
                args.join(" ")
            );
            assert!(
                help.contains("operator terminal"),
                "tagline missing in: {}",
                args.join(" ")
            );
        }
    }
}
