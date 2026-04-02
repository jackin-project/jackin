use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "jackin", version, about = "Matrix-inspired Claude agent operator")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum Command {
    Load {
        selector: String,
        /// Skip the Matrix intro/outro animations
        #[arg(long, default_value_t = false)]
        no_intro: bool,
        /// Show verbose output (e.g. Docker build logs)
        #[arg(long, default_value_t = false)]
        debug: bool,
    },
    Hardline { container: String },
    Eject {
        selector: String,
        #[arg(long)]
        all: bool,
        #[arg(long)]
        purge: bool,
    },
    Exile,
    Purge {
        selector: String,
        #[arg(long)]
        all: bool,
    },
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
}
