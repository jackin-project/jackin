use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "jackin", version, about = " Claude agent operator")]
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
