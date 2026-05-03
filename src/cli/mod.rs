use clap::builder::styling::{AnsiColor, Effects, Styles};
use clap::{Parser, Subcommand};

use cleanup::{EjectArgs, PurgeArgs};
use role::{ConsoleArgs, HardlineArgs, LaunchArgs, LoadArgs};

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

pub mod cleanup;
pub mod config;
pub mod dispatch;
pub mod help;
pub mod role;
pub mod workspace;

pub use config::{AuthCommand, ConfigCommand, EnvCommand, MountCommand, TrustCommand};
pub use workspace::{WorkspaceCommand, WorkspaceEnvCommand};

/// Operator's CLI for orchestrating AI coding roles in isolated containers
///
/// Running `jackin` with no subcommand opens the operator console when
/// stdout is attached to a reasonably-sized interactive terminal, and
/// otherwise prints this help page (exit 0, silent). The flattened
/// [`ConsoleArgs`] make `jackin --debug` equivalent to
/// `jackin console --debug`.
#[derive(Debug, Parser)]
#[command(
    name = "jackin",
    version = env!("JACKIN_VERSION"),
    styles = HELP_STYLES,
    before_help = BANNER,
    disable_help_subcommand = true,
    after_help = "Run 'jackin help <command>' for more detailed information."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
    /// Top-level console args — carried through to the console runner when
    /// no subcommand is given (i.e. bare `jackin`).
    #[command(flatten)]
    pub console_args: ConsoleArgs,
}

/// Top-level `jackin` subcommand dispatch.
///
/// Variants that wrap an `#[derive(Args)]` struct carry their help text on
/// the struct itself — see e.g. `cli::role::LoadArgs`. Variants that wrap
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
    /// Pull every running role out at once
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Exile,
    Purge(PurgeArgs),
    Console(ConsoleArgs),
    /// Open the operator console (deprecated — use `jackin` or `jackin console`)
    #[command(hide = true)]
    Launch(LaunchArgs),
    /// Manage saved workspaces
    #[command(subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Workspace(WorkspaceCommand),
    /// View and modify operator configuration
    #[command(subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Config(ConfigCommand),
    /// Print help documentation for a jackin command
    ///
    /// With no arguments, displays the jackin manual.
    /// With a command name, displays the manual for that command:
    ///
    ///   jackin help config
    ///   jackin help config auth
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Help {
        /// Command path to get help for (e.g. `config auth`)
        #[arg(trailing_var_arg = true, num_args = 0..)]
        command: Vec<String>,
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
                "Operator's CLI for orchestrating AI coding roles in isolated containers"
            )
        );
        for cmd in [
            "load",
            "hardline",
            "eject",
            "exile",
            "purge",
            "console",
            "workspace",
            "config",
        ] {
            assert!(help.contains(cmd), "missing command: {cmd}");
        }
        // `launch` is hidden (deprecated alias for `console`) and should
        // not appear in the top-level command list.
        assert!(
            !help.contains("\n  launch "),
            "deprecated `launch` should be hidden from help"
        );
    }

    // ── help subcommand disabled ────────────────────────────────────────

    #[test]
    fn root_help_lists_help_subcommand() {
        // Our explicit `help` command must appear in the top-level listing.
        let help = help_text(&["jackin", "--help"]);
        assert!(
            help.contains("\n  help "),
            "root `help` subcommand should be listed"
        );
    }

    #[test]
    fn config_help_does_not_list_help_subcommand() {
        let help = help_text(&["jackin", "config", "--help"]);
        assert!(
            !help.contains("\n  help"),
            "`config help` subcommand should be disabled"
        );
    }

    #[test]
    fn workspace_help_does_not_list_help_subcommand() {
        let help = help_text(&["jackin", "workspace", "--help"]);
        assert!(
            !help.contains("\n  help"),
            "`workspace help` subcommand should be disabled"
        );
    }

    // ── Help command ────────────────────────────────────────────────────

    #[test]
    fn parses_help_with_no_args() {
        let cli = Cli::try_parse_from(["jackin", "help"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Help { ref command }) if command.is_empty()
        ));
    }

    #[test]
    fn parses_help_with_single_subcommand() {
        let cli = Cli::try_parse_from(["jackin", "help", "config"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Help { ref command }) if command == &["config"]
        ));
    }

    #[test]
    fn parses_help_with_nested_subcommand() {
        let cli = Cli::try_parse_from(["jackin", "help", "config", "auth"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Help { ref command }) if command == &["config", "auth"]
        ));
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
            vec!["jackin", "console", "--help"],
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
