use clap::Subcommand;

use super::{BANNER, HELP_STYLES};

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum ConfigCommand {
    /// Manage global mount configurations
    #[command(subcommand, before_help = BANNER, styles = HELP_STYLES)]
    Mount(MountCommand),
    /// Manage trust for third-party agent sources
    #[command(subcommand, before_help = BANNER, styles = HELP_STYLES)]
    Trust(TrustCommand),
    /// Manage Claude Code authentication forwarding from host
    #[command(subcommand, before_help = BANNER, styles = HELP_STYLES)]
    Auth(AuthCommand),
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum AuthCommand {
    /// Set the authentication forwarding mode
    ///
    /// Controls how the host's Claude Code authentication is made available
    /// to agent containers.
    /// Modes: sync (default — overwrite container auth from host on each
    /// launch when host auth exists; preserve container auth when host auth
    /// is absent), ignore (revoke and never forward), token (use a long-lived
    /// `CLAUDE_CODE_OAUTH_TOKEN` resolved from the operator env — the token
    /// itself is never written to disk; see `jackin` docs on auth forwarding
    /// for setup).
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin config auth set sync
  jackin config auth set ignore
  jackin config auth set token
  jackin config auth set sync --agent agent-smith
  jackin config auth set token --agent chainargos/the-architect"
    )]
    Set {
        /// Authentication forwarding mode: sync, ignore, or token
        mode: String,
        /// Apply to a specific agent instead of globally
        #[arg(long)]
        agent: Option<String>,
    },
    /// Show the current authentication forwarding mode
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin config auth show
  jackin config auth show --agent agent-smith"
    )]
    Show {
        /// Show mode for a specific agent (including inheritance)
        #[arg(long)]
        agent: Option<String>,
    },
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum MountCommand {
    /// Register a new global mount applied to matching agents
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin config mount add gradle-cache --src ~/.gradle/caches --dst /home/claude/.gradle/caches --readonly
  jackin config mount add secrets --src ~/.chainargos/secrets --dst /secrets --readonly --scope \"chainargos/*\""
    )]
    Add {
        /// Unique name for this mount (used to identify it later)
        name: String,
        /// Path on the host machine to mount from
        #[arg(long)]
        src: String,
        /// Path inside the container to mount to
        #[arg(long)]
        dst: String,
        /// Make this mount read-only inside the container
        #[arg(long, default_value_t = false)]
        readonly: bool,
        /// Apply only to matching agents (e.g. `chainargos/*` or `chainargos/agent-brown`)
        #[arg(long)]
        scope: Option<String>,
    },
    /// Unregister a global mount by name
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin config mount remove gradle-cache
  jackin config mount remove secrets --scope \"chainargos/*\""
    )]
    Remove {
        /// Name of the mount to remove
        name: String,
        /// Only remove from this scope (leave other scopes untouched)
        #[arg(long)]
        scope: Option<String>,
    },
    /// List all registered global mounts
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    List,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum TrustCommand {
    /// Mark a third-party agent source as trusted
    ///
    /// Trust controls whether jackin' will build and run an agent without
    /// prompting.  Untrusted agents require interactive confirmation on
    /// every load.
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin config trust grant chainargos/the-architect"
    )]
    Grant {
        /// Agent class selector (e.g. `chainargos/agent-brown`)
        selector: String,
    },
    /// Revoke trust for a third-party agent source
    ///
    /// The next `jackin load` will prompt for confirmation again.
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin config trust revoke chainargos/the-architect"
    )]
    Revoke {
        /// Agent class selector (e.g. `chainargos/agent-brown`)
        selector: String,
    },
    /// List all currently trusted agent sources
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    List,
}

#[cfg(test)]
mod tests {
    use super::super::{Cli, Command};
    use super::*;
    use clap::Parser;

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

    #[test]
    fn parses_config_mount_add() {
        let cli = Cli::try_parse_from([
            "jackin",
            "config",
            "mount",
            "add",
            "gradle-cache",
            "--src",
            "~/.gradle/caches",
            "--dst",
            "/home/claude/.gradle/caches",
            "--readonly",
            "--scope",
            "chainargos/*",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Config(ConfigCommand::Mount(
                MountCommand::Add { .. }
            )))
        ));
    }

    #[test]
    fn parses_config_mount_remove() {
        let cli =
            Cli::try_parse_from(["jackin", "config", "mount", "remove", "gradle-cache"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Config(ConfigCommand::Mount(
                MountCommand::Remove { .. }
            )))
        ));
    }

    #[test]
    fn parses_config_mount_list() {
        let cli = Cli::try_parse_from(["jackin", "config", "mount", "list"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Config(ConfigCommand::Mount(MountCommand::List)))
        ));
    }

    #[test]
    fn parses_config_trust_grant() {
        let cli = Cli::try_parse_from([
            "jackin",
            "config",
            "trust",
            "grant",
            "chainargos/the-architect",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Config(ConfigCommand::Trust(
                TrustCommand::Grant { .. }
            )))
        ));
    }

    #[test]
    fn parses_config_trust_revoke() {
        let cli = Cli::try_parse_from([
            "jackin",
            "config",
            "trust",
            "revoke",
            "chainargos/the-architect",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Config(ConfigCommand::Trust(
                TrustCommand::Revoke { .. }
            )))
        ));
    }

    #[test]
    fn parses_config_trust_list() {
        let cli = Cli::try_parse_from(["jackin", "config", "trust", "list"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Config(ConfigCommand::Trust(TrustCommand::List)))
        ));
    }

    // ── Config mount help ───────────────────────────────────────────────

    #[test]
    fn config_mount_add_help_shows_examples() {
        let help = help_text(&["jackin", "config", "mount", "add", "--help"]);
        assert!(help.contains("Examples:"));
        assert!(help.contains("jackin config mount add gradle-cache"));
        assert!(help.contains("--scope"));
    }

    #[test]
    fn config_mount_remove_help_shows_examples() {
        let help = help_text(&["jackin", "config", "mount", "remove", "--help"]);
        assert!(help.contains("Examples:"));
        assert!(help.contains("jackin config mount remove gradle-cache"));
    }

    // ── Config auth help ─────────────────────────────────────────────────

    #[test]
    fn config_auth_set_help_shows_examples() {
        let help = help_text(&["jackin", "config", "auth", "set", "--help"]);
        assert!(help.contains("Examples:"));
        assert!(help.contains("jackin config auth set sync"));
        assert!(help.contains("jackin config auth set token"));
        assert!(help.contains("--agent"));
    }

    #[test]
    fn config_auth_set_help_lists_token_as_accepted_mode() {
        let help = help_text(&["jackin", "config", "auth", "set", "--help"]);
        // Modes are listed in the subcommand doc comment. After this
        // PR the accepted modes are sync, ignore, and token.
        assert!(help.contains("sync"));
        assert!(help.contains("ignore"));
        assert!(
            help.contains("token"),
            "help text must advertise the token mode; got:\n{help}"
        );
    }

    #[test]
    fn config_auth_show_help_shows_examples() {
        let help = help_text(&["jackin", "config", "auth", "show", "--help"]);
        assert!(help.contains("Examples:"));
        assert!(help.contains("jackin config auth show"));
    }

    #[test]
    fn parses_config_auth_set_global() {
        let cli = Cli::try_parse_from(["jackin", "config", "auth", "set", "sync"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Config(ConfigCommand::Auth(AuthCommand::Set {
                        ref mode,
                        agent: None,
                    }))) if mode == "sync"
        ));
    }

    #[test]
    fn parses_config_auth_set_token_global() {
        let cli = Cli::try_parse_from(["jackin", "config", "auth", "set", "token"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Config(ConfigCommand::Auth(AuthCommand::Set {
                        ref mode,
                        agent: None,
                    }))) if mode == "token"
        ));
    }

    #[test]
    fn parses_config_auth_set_per_agent() {
        let cli = Cli::try_parse_from([
            "jackin",
            "config",
            "auth",
            "set",
            "sync",
            "--agent",
            "agent-smith",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Config(ConfigCommand::Auth(AuthCommand::Set {
                        ref mode,
                        agent: Some(ref agent),
                    }))) if mode == "sync" && agent == "agent-smith"
        ));
    }

    #[test]
    fn parses_config_auth_show() {
        let cli = Cli::try_parse_from(["jackin", "config", "auth", "show"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Config(ConfigCommand::Auth(AuthCommand::Show {
                agent: None
            })))
        ));
    }
}
