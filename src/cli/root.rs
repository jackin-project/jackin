use clap::{Parser, Subcommand};

use super::agent::{HardlineArgs, LaunchArgs, LoadArgs};
use super::cleanup::{EjectArgs, PurgeArgs};
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
    Load(LoadArgs),
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
    Hardline(HardlineArgs),
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
    Eject(EjectArgs),
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
    Purge(PurgeArgs),
    /// Open the interactive TUI launcher to pick a workspace and agent
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Launch(LaunchArgs),
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
