use clap::Subcommand;

use super::{BANNER, HELP_STYLES};

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum ConfigCommand {
    /// Manage global mount configurations
    #[command(subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Mount(MountCommand),
    /// Manage trust for third-party role sources
    #[command(subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Trust(TrustCommand),
    /// Manage Claude Code authentication forwarding from host
    #[command(subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Auth(AuthCommand),
    /// Manage operator env vars at global and per-role scope
    #[command(subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Env(EnvCommand),
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum EnvCommand {
    /// Set an env var at global or per-role scope
    ///
    /// Without `--role`, writes to the global `[env]` table. With
    /// `--role <SELECTOR>`, writes to `[roles.<selector>.env]`. The role
    /// selector is not pre-validated — the table path is written regardless
    /// of whether that role is registered, matching `config auth set`.
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin config env set API_TOKEN \"op://Personal/api/token\"
  jackin config env set LOG_LEVEL debug --role agent-smith
  jackin config env set OPENAI_KEY \"op://Work/OpenAI/key\" --comment \"rotate quarterly\""
    )]
    Set {
        /// Env var name (stored verbatim; no POSIX validation)
        key: String,
        /// Env var value (use `op://...`, `$VAR`, `${VAR}`, or literal)
        value: String,
        /// Apply to a specific role instead of globally
        #[arg(long)]
        role: Option<String>,
        /// Write a TOML comment line above the key
        #[arg(long)]
        comment: Option<String>,
    },
    /// Unset an env var at global or per-role scope
    ///
    /// Idempotent: if the key is not present, prints "KEY not set." and
    /// exits 0 without saving the config.
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin config env unset API_TOKEN
  jackin config env unset LOG_LEVEL --role agent-smith"
    )]
    Unset {
        /// Env var name to remove
        key: String,
        /// Unset from a specific role instead of globally
        #[arg(long)]
        role: Option<String>,
    },
    /// List env vars at global or per-role scope
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin config env list
  jackin config env list --role agent-smith"
    )]
    List {
        /// List vars for a specific role instead of the global scope
        #[arg(long)]
        role: Option<String>,
    },
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum AuthCommand {
    /// Set the authentication forwarding mode
    ///
    /// Controls how the host's agent authentication is made available
    /// to role containers.
    /// Modes: sync (default — overwrite container auth from host on each
    /// launch when host auth exists; preserve container auth when host auth
    /// is absent), ignore (revoke and never forward), `oauth_token` (use a
    /// long-lived `CLAUDE_CODE_OAUTH_TOKEN` resolved from the operator env),
    /// `api_key` (use a short-lived API key — e.g. `ANTHROPIC_API_KEY` /
    /// `OPENAI_API_KEY` — resolved from the operator env). Tokens and keys
    /// are never written to disk; see `jackin` docs on auth forwarding for
    /// setup.
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin config auth set sync
  jackin config auth set ignore
  jackin config auth set oauth_token
  jackin config auth set api_key"
    )]
    Set {
        /// Authentication forwarding mode: sync, ignore, `api_key`, or `oauth_token`
        mode: String,
    },
    /// Show the current authentication forwarding mode
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin config auth show"
    )]
    Show,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum MountCommand {
    /// Register a new global mount applied to matching roles
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin config mount add gradle-cache --src ~/.gradle/caches --dst /home/agent/.gradle/caches --readonly
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
        /// Apply only to matching roles (e.g. `chainargos/*` or `chainargos/agent-brown`)
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
    /// Mark a third-party role source as trusted
    ///
    /// Trust controls whether jackin' will build and run an role without
    /// prompting.  Untrusted roles require interactive confirmation on
    /// every load.
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin config trust grant chainargos/the-architect"
    )]
    Grant {
        /// Role class selector (e.g. `chainargos/agent-brown`)
        selector: String,
    },
    /// Revoke trust for a third-party role source
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
        /// Role class selector (e.g. `chainargos/agent-brown`)
        selector: String,
    },
    /// List all currently trusted role sources
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
            "/home/agent/.gradle/caches",
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

    // ── help subcommand disabled ────────────────────────────────────────

    #[test]
    fn config_auth_rejects_help_subcommand() {
        let err = Cli::try_parse_from(["jackin", "config", "auth", "help"]).unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::InvalidSubcommand);
    }

    #[test]
    fn config_mount_rejects_help_subcommand() {
        let err = Cli::try_parse_from(["jackin", "config", "mount", "help"]).unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::InvalidSubcommand);
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
        assert!(help.contains("jackin config auth set oauth_token"));
        assert!(help.contains("jackin config auth set api_key"));
    }

    #[test]
    fn config_auth_set_help_lists_token_as_accepted_mode() {
        let help = help_text(&["jackin", "config", "auth", "set", "--help"]);
        // Modes are listed in the subcommand doc comment.
        assert!(help.contains("sync"));
        assert!(help.contains("ignore"));
        assert!(
            help.contains("oauth_token"),
            "help text must advertise the oauth_token mode; got:\n{help}"
        );
        assert!(
            help.contains("api_key"),
            "help text must advertise the api_key mode; got:\n{help}"
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
                    }))) if mode == "sync"
        ));
    }

    #[test]
    fn parses_config_auth_set_oauth_token_global() {
        let cli = Cli::try_parse_from(["jackin", "config", "auth", "set", "oauth_token"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Config(ConfigCommand::Auth(AuthCommand::Set {
                        ref mode,
                    }))) if mode == "oauth_token"
        ));
    }

    #[test]
    fn parses_config_auth_show() {
        let cli = Cli::try_parse_from(["jackin", "config", "auth", "show"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Config(ConfigCommand::Auth(AuthCommand::Show)))
        ));
    }
}
