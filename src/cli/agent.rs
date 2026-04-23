use clap::Args;

use super::{BANNER, HELP_STYLES};

/// Jack an agent into an isolated container
///
/// TARGET can be a path (~/Projects/my-app), a path with container
/// destination (~/Projects/my-app:/app), or a saved workspace name.
/// When omitted, the current directory is used.
#[derive(Debug, Args, PartialEq, Eq)]
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
pub struct LoadArgs {
    /// Agent class selector (e.g. `agent-smith`, `chainargos/agent-brown`).
    /// When omitted, uses the last-used or default agent for the workspace.
    pub selector: Option<String>,
    /// Path, `path:container-dest`, or saved workspace name
    #[arg(value_name = "TARGET")]
    pub target: Option<String>,
    /// Additional bind-mount spec as `path[:ro]` or `src:dst[:ro]` (repeatable)
    #[arg(long = "mount")]
    pub mounts: Vec<String>,
    /// Force rebuild the Docker image (updates Claude to latest version)
    #[arg(long, default_value_t = false)]
    pub rebuild: bool,
    /// Skip the animated intro sequence
    #[arg(long, default_value_t = false)]
    pub no_intro: bool,
    /// Print raw container output for troubleshooting
    #[arg(long, default_value_t = false)]
    pub debug: bool,
}

/// Reattach to a running agent's session
///
/// When omitted, finds the saved workspace for the current directory and
/// reconnects to a running agent container belonging to it.
#[derive(Debug, Args, PartialEq, Eq)]
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
pub struct HardlineArgs {
    /// Agent class selector or container name to reconnect to.
    /// When omitted, uses the running agent in the workspace for the current directory.
    pub selector: Option<String>,
}

/// Open the interactive TUI launcher to pick a workspace and agent
#[derive(Debug, Args, PartialEq, Eq)]
#[command(before_help = BANNER, styles = HELP_STYLES)]
pub struct LaunchArgs {
    /// Print raw container output for troubleshooting
    #[arg(long, default_value_t = false)]
    pub debug: bool,
}

#[cfg(test)]
mod tests {
    use crate::cli::{Cli, Command};
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
    fn parses_load_command() {
        let cli = Cli::try_parse_from(["jackin", "load", "agent-smith"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Load(super::LoadArgs {
                selector: Some(ref s),
                target: None,
                no_intro: false,
                debug: false,
                ..
            }) if s == "agent-smith"
        ));
    }

    #[test]
    fn parses_load_without_selector() {
        let cli = Cli::try_parse_from(["jackin", "load"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Load(super::LoadArgs {
                selector: None,
                target: None,
                ..
            })
        ));
    }

    #[test]
    fn parses_load_rebuild_without_selector() {
        let cli = Cli::try_parse_from(["jackin", "load", "--rebuild"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Load(super::LoadArgs {
                selector: None,
                rebuild: true,
                ..
            })
        ));
    }

    #[test]
    fn parses_load_with_target_path() {
        let cli =
            Cli::try_parse_from(["jackin", "load", "agent-smith", "~/Projects/my-app"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Load(super::LoadArgs {
                target: Some(ref t),
                ..
            }) if t == "~/Projects/my-app"
        ));
    }

    #[test]
    fn parses_load_with_target_and_mount() {
        let cli = Cli::try_parse_from([
            "jackin",
            "load",
            "agent-smith",
            "big-monorepo",
            "--mount",
            "/tmp/cache:/workspace/cache:ro",
        ])
        .unwrap();

        assert!(matches!(
            cli.command,
            Command::Load(super::LoadArgs {
                target: Some(ref t),
                ref mounts,
                ..
            }) if t == "big-monorepo" && mounts.len() == 1
        ));
    }

    #[test]
    fn parses_load_with_mount_only() {
        let cli = Cli::try_parse_from([
            "jackin",
            "load",
            "agent-smith",
            "--mount",
            "/tmp/project:/workspace/project",
            "--mount",
            "/tmp/cache:/workspace/cache:ro",
        ])
        .unwrap();

        assert!(matches!(
            cli.command,
            Command::Load(super::LoadArgs {
                target: None,
                ref mounts,
                ..
            }) if mounts.len() == 2
        ));
    }

    #[test]
    fn parses_launch_command() {
        let cli = Cli::try_parse_from(["jackin", "launch"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Launch(super::LaunchArgs { debug: false })
        ));
    }

    // ── Load help ───────────────────────────────────────────────────────

    #[test]
    fn load_help_shows_description_and_examples() {
        let help = help_text(&["jackin", "load", "--help"]);
        assert!(help.contains("Jack an agent into an isolated container"));
        assert!(help.contains("Examples:"));
        assert!(help.contains("jackin load agent-smith"));
        assert!(help.contains("jackin load agent-smith big-monorepo"));
    }

    #[test]
    fn load_help_shows_mount_format() {
        let help = help_text(&["jackin", "load", "--help"]);
        assert!(
            help.contains("path[:ro]") && help.contains("src:dst[:ro]"),
            "mount format missing"
        );
    }

    // ── Hardline help ───────────────────────────────────────────────────

    #[test]
    fn hardline_help_shows_examples() {
        let help = help_text(&["jackin", "hardline", "--help"]);
        assert!(help.contains("Reattach to a running agent"));
        assert!(help.contains("jackin hardline agent-smith"));
        assert!(
            help.contains("jackin hardline ") && help.contains("auto-detect workspace"),
            "missing no-arg usage in hardline help: {help}"
        );
    }

    #[test]
    fn parses_hardline_without_selector() {
        let cli = Cli::try_parse_from(["jackin", "hardline"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Hardline(super::HardlineArgs { selector: None })
        ));
    }

    #[test]
    fn parses_hardline_with_selector() {
        let cli = Cli::try_parse_from(["jackin", "hardline", "agent-smith"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Hardline(super::HardlineArgs { selector: Some(ref s) }) if s == "agent-smith"
        ));
    }
}
