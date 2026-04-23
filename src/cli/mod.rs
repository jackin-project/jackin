use clap::builder::styling::{AnsiColor, Effects, Styles};
use clap::{Parser, Subcommand};

use agent::{HardlineArgs, LaunchArgs, LoadArgs};
use cleanup::{EjectArgs, PurgeArgs};

pub(super) const HELP_STYLES: Styles = Styles::styled()
    .header(AnsiColor::BrightGreen.on_default().effects(Effects::BOLD))
    .usage(AnsiColor::BrightGreen.on_default().effects(Effects::BOLD))
    .literal(AnsiColor::Green.on_default().effects(Effects::BOLD))
    .placeholder(AnsiColor::White.on_default())
    .valid(AnsiColor::BrightGreen.on_default())
    .invalid(AnsiColor::Red.on_default().effects(Effects::BOLD))
    .error(AnsiColor::Red.on_default().effects(Effects::BOLD));

pub(super) const BANNER: &str = concat!(
    "\n\n\n",
    "\x1b[94m",
    "    │ │╷│ │╷│ ╷  │╷│ │╷│ │╷│\n",
    "    │ ╵│ │╵│ ╵ ╷ ╵│ │╵│ │╵│\n",
    "    ╵  ╵ ╵ ╵  │  ╵ ╵ ╵ ╵ ╵\n",
    "               ╵\n",
    "\x1b[0m",
    "\x1b[1;97m",
    "          j a c k i n\n",
    "\x1b[0m",
    "\x1b[38;5;67m",
    "       operator terminal\n",
    "\x1b[0m",
);

pub mod agent;
pub mod cleanup;
pub mod config;
pub mod workspace;

pub use config::{AuthCommand, ConfigCommand, MountCommand, TrustCommand};
pub use workspace::WorkspaceCommand;

/// Operator's CLI for orchestrating AI coding agents in isolated containers
#[derive(Debug, Parser)]
#[command(name = "jackin", version = env!("JACKIN_VERSION"), styles = HELP_STYLES, before_help = BANNER)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

/// Top-level `jackin` subcommand dispatch.
///
/// Variants that wrap an `#[derive(Args)]` struct carry their help text on
/// the struct itself — see e.g. `cli::agent::LoadArgs`. Variants that wrap
/// a `#[derive(Subcommand)]` enum (`Workspace`, `Config`) keep their
/// parent-command help on the outer variant: Clap's subcommand-enum
/// attribute propagation targets nested variants, not the parent help
/// page. `Exile` is a unit variant with no payload, so its attributes
/// also live here.
///
/// All variants use tuple form (`Load(LoadArgs)`, `Workspace(WorkspaceCommand)`),
/// never struct form with inline `{ ... }`. This keeps dispatch symmetry.
#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum Command {
    Load(LoadArgs),
    Hardline(HardlineArgs),
    Eject(EjectArgs),
    /// Pull every running agent out at once
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Exile,
    Purge(PurgeArgs),
    Launch(LaunchArgs),
    /// Manage saved workspaces
    #[command(subcommand, before_help = BANNER, styles = HELP_STYLES)]
    Workspace(WorkspaceCommand),
    /// View and modify operator configuration
    #[command(subcommand, before_help = BANNER, styles = HELP_STYLES)]
    Config(ConfigCommand),
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
