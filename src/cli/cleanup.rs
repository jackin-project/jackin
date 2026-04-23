use clap::Args;

use super::{BANNER, HELP_STYLES};

/// Stop an agent and clean up its container
#[derive(Debug, Args, PartialEq, Eq)]
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
pub struct EjectArgs {
    /// Agent class selector or container name to stop
    pub selector: String,
    /// Stop every running instance of this agent class
    #[arg(long)]
    pub all: bool,
    /// Also delete persisted state after stopping
    #[arg(long)]
    pub purge: bool,
}

/// Delete persisted state for an agent class
#[derive(Debug, Args, PartialEq, Eq)]
#[command(
    before_help = BANNER,
    styles = HELP_STYLES,
    after_long_help = "\
Examples:
  jackin purge agent-smith
  jackin purge agent-smith --all
  jackin purge chainargos/the-architect"
)]
pub struct PurgeArgs {
    /// Agent class selector (e.g. `agent-smith`, `chainargos/agent-brown`)
    pub selector: String,
    /// Delete state for every instance, not just the default
    #[arg(long)]
    pub all: bool,
}

#[cfg(test)]
mod tests {
    use crate::cli::Cli;
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
}
