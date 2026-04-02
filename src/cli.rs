use clap::builder::styling::{AnsiColor, Effects, Styles};
use clap::{Parser, Subcommand};

const HELP_STYLES: Styles = Styles::styled()
    .header(AnsiColor::BrightGreen.on_default().effects(Effects::BOLD))
    .usage(AnsiColor::BrightGreen.on_default().effects(Effects::BOLD))
    .literal(AnsiColor::Green.on_default().effects(Effects::BOLD))
    .placeholder(AnsiColor::Green.on_default())
    .valid(AnsiColor::BrightGreen.on_default())
    .invalid(AnsiColor::Red.on_default().effects(Effects::BOLD))
    .error(AnsiColor::Red.on_default().effects(Effects::BOLD));

const BANNER: &str = r#"
    │ │╷│ │╷│ ╷  │╷│ │╷│ │╷│
    │ ╵│ │╵│ ╵ ╷ ╵│ │╵│ │╵│
    ╵  ╵ ╵ ╵  │  ╵ ╵ ╵ ╵ ╵
               ╵
          j a c k i n
       operator terminal
"#;

/// Send agents into the Matrix
#[derive(Debug, Parser)]
#[command(name = "jackin", version, styles = HELP_STYLES, before_help = BANNER)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum Command {
    /// Jack an agent into the Matrix
    Load {
        /// Agent class selector (e.g. agent-smith, chainargos/agent-brown)
        selector: String,
        /// Bypass the construct sequence
        #[arg(long, default_value_t = false)]
        no_intro: bool,
        /// Show raw signal output
        #[arg(long, default_value_t = false)]
        debug: bool,
    },
    /// Reattach to a running agent
    Hardline {
        /// Container name to reattach to
        container: String,
    },
    /// Pull an agent out of the Matrix
    Eject {
        /// Agent class selector or container name
        selector: String,
        /// Pull every instance of this class
        #[arg(long)]
        all: bool,
        /// Delete persisted state after ejection
        #[arg(long)]
        purge: bool,
    },
    /// Pull every agent out
    Exile,
    /// Delete persisted state for an agent class
    Purge {
        /// Agent class selector
        selector: String,
        /// Delete state for every instance of this class
        #[arg(long)]
        all: bool,
    },
    /// Operator configuration
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum ConfigCommand {
    Mount {
        #[command(subcommand)]
        command: MountCommand,
    },
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum MountCommand {
    Add {
        /// Mount name (used as identifier for removal)
        name: String,
        /// Host source path
        #[arg(long)]
        src: String,
        /// Container destination path
        #[arg(long)]
        dst: String,
        /// Mount as read-only
        #[arg(long, default_value_t = false)]
        readonly: bool,
        /// Scope pattern (e.g. "chainargos/*" or "chainargos/agent-brown")
        #[arg(long)]
        scope: Option<String>,
    },
    Remove {
        /// Mount name to remove
        name: String,
        /// Scope pattern to remove from
        #[arg(long)]
        scope: Option<String>,
    },
    List,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_config_mount_add() {
        let cli = Cli::try_parse_from([
            "jackin", "config", "mount", "add", "gradle-cache",
            "--src", "~/.gradle/caches",
            "--dst", "/home/claude/.gradle/caches",
            "--readonly",
            "--scope", "chainargos/*",
        ]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Config { command: ConfigCommand::Mount { command: MountCommand::Add { .. } } }
        ));
    }

    #[test]
    fn parses_config_mount_remove() {
        let cli = Cli::try_parse_from([
            "jackin", "config", "mount", "remove", "gradle-cache",
        ]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Config { command: ConfigCommand::Mount { command: MountCommand::Remove { .. } } }
        ));
    }

    #[test]
    fn parses_config_mount_list() {
        let cli = Cli::try_parse_from([
            "jackin", "config", "mount", "list",
        ]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Config { command: ConfigCommand::Mount { command: MountCommand::List } }
        ));
    }

    #[test]
    fn parses_load_command() {
        let cli = Cli::try_parse_from(["jackin", "load", "agent-smith"]).unwrap();
        assert_eq!(
            cli.command,
            Command::Load {
                selector: "agent-smith".to_string(),
                no_intro: false,
                debug: false,
            }
        );
    }

    #[test]
    fn help_contains_banner_and_matrix_descriptions() {
        let err = Cli::try_parse_from(["jackin", "--help"]).unwrap_err();
        let help = err.to_string();
        assert!(help.contains("j a c k i n"), "banner missing");
        assert!(help.contains("operator terminal"), "banner tagline missing");
        assert!(help.contains("Send agents into the Matrix"), "about text missing");
    }

    #[test]
    fn load_help_contains_matrix_description() {
        let err = Cli::try_parse_from(["jackin", "load", "--help"]).unwrap_err();
        let help = err.to_string();
        assert!(help.contains("Jack an agent into the Matrix"), "load description missing");
    }
}
