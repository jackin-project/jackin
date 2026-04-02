use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "jackin", version, about = "Matrix-inspired Claude agent operator")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum Command {
    Load { selector: String },
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
            }
        );
    }
}
