use clap::Args;

use super::{BANNER, HELP_STYLES};

/// Jack an role into an isolated container
///
/// TARGET can be a path (~/Projects/my-app), a path with container
/// destination (~/Projects/my-app:/app), or a saved workspace name.
/// When omitted, the current directory is used.
//
// Five launch-time toggles (rebuild / no_intro / debug / force) plus the
// positional `selector` / `target` / `mounts` map directly to CLI flags;
// bundling them into nested structs would obscure rather than clarify.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Args, PartialEq, Eq)]
#[command(
    before_help = BANNER,
    styles = HELP_STYLES,
    after_long_help = "\
Examples:
  jackin load                                          # use workspace + last role for cwd
  jackin load --rebuild                                # same, with fresh agent install
  jackin load agent-smith
  jackin load agent-smith ~/Projects/my-app
  jackin load agent-smith ~/Projects/my-app:/app
  jackin load agent-smith big-monorepo
  jackin load agent-smith big-monorepo --mount ~/extra-data
  jackin load agent-smith ~/app --mount ~/cache:/cache:ro"
)]
pub struct LoadArgs {
    /// Role class selector (e.g. `agent-smith`, `chainargos/agent-brown`).
    /// When omitted, uses the last-used or default role for the workspace.
    pub selector: Option<String>,
    /// Path, `path:container-dest`, or saved workspace name
    #[arg(value_name = "TARGET")]
    pub target: Option<String>,
    /// Additional bind-mount spec as `path[:ro]` or `src:dst[:ro]` (repeatable)
    #[arg(long = "mount")]
    pub mounts: Vec<String>,
    /// Force rebuild the Docker image and refresh agent CLI install layers
    #[arg(long, default_value_t = false)]
    pub rebuild: bool,
    /// Skip the animated intro sequence
    #[arg(long, default_value_t = false)]
    pub no_intro: bool,
    /// Print raw container output for troubleshooting
    //
    // The `action`/`value_parser` overrides are deliberate. clap
    // derive's default for a `bool` field is `Set` + `BoolValueParser`
    // — fine for CLI but rejects env values like `JACKIN_DEBUG=1` (only
    // literal `"true"` / `"false"` parse). Forcing `SetTrue` makes
    // `--debug` a presence flag again, and `FalseyValueParser` makes
    // env truthy/falsy strings (`1`/`0`/`yes`/`no`/empty) parse the
    // way an operator would expect.
    #[arg(
        long,
        env = "JACKIN_DEBUG",
        action = clap::ArgAction::SetTrue,
        value_parser = clap::builder::FalseyValueParser::new(),
    )]
    pub debug: bool,
    /// Acknowledge a dirty host working tree for isolated mounts.
    #[arg(long)]
    pub force: bool,
    /// Agent to launch under (claude or codex). Overrides the
    /// workspace's `default_agent` field for this launch only. When
    /// neither is set, defaults to claude.
    #[arg(long, value_parser = parse_agent)]
    pub agent: Option<crate::agent::Agent>,
}

fn parse_agent(s: &str) -> Result<crate::agent::Agent, String> {
    s.parse()
        .map_err(|e: crate::agent::ParseAgentError| e.to_string())
}

/// Reattach to a running role's session
///
/// When omitted, finds the saved workspace for the current directory and
/// reconnects to a running role container belonging to it.
#[derive(Debug, Args, PartialEq, Eq)]
#[command(
    before_help = BANNER,
    styles = HELP_STYLES,
    after_long_help = "\
Examples:
  jackin hardline                              # auto-detect workspace + running role for cwd
  jackin hardline agent-smith
  jackin hardline chainargos/the-architect
  jackin hardline jackin-agent-smith-clone-1"
)]
pub struct HardlineArgs {
    /// Role class selector or container name to reconnect to.
    /// When omitted, uses the running role in the workspace for the current directory.
    pub selector: Option<String>,
}

/// Open the operator console to manage workspaces, launch roles, and more
///
/// Running `jackin` with no subcommand on an interactive terminal opens the
/// same console. This struct also flattens into the top-level `Cli` so
/// `jackin --debug` is equivalent to `jackin console --debug`.
#[derive(Debug, Args, PartialEq, Eq, Default, Clone)]
#[command(before_help = BANNER, styles = HELP_STYLES)]
pub struct ConsoleArgs {
    /// Print raw container output for troubleshooting
    //
    // See `LoadArgs.debug` for why action/value_parser are explicit.
    #[arg(
        long,
        env = "JACKIN_DEBUG",
        action = clap::ArgAction::SetTrue,
        value_parser = clap::builder::FalseyValueParser::new(),
    )]
    pub debug: bool,
}

/// Backwards-compat alias for `ConsoleArgs`.
///
/// `jackin launch` is the pre-rename spelling of `jackin console`. The
/// struct is identical; the type alias keeps older module imports working
/// without a big-bang rename.
pub type LaunchArgs = ConsoleArgs;

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
    fn load_args_parses_agent_flag() {
        let cli =
            Cli::try_parse_from(["jackin", "load", "agent-smith", "--agent", "codex"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Load(super::LoadArgs {
                agent: Some(crate::agent::Agent::Codex),
                ..
            }))
        ));
    }

    #[test]
    fn load_args_rejects_unknown_agent() {
        let res = Cli::try_parse_from(["jackin", "load", "agent-smith", "--agent", "amp"]);
        assert!(res.is_err());
    }

    #[test]
    fn load_args_agent_optional() {
        let cli = Cli::try_parse_from(["jackin", "load", "agent-smith"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Load(super::LoadArgs { agent: None, .. }))
        ));
    }

    #[test]
    fn parses_load_command() {
        let cli = Cli::try_parse_from(["jackin", "load", "agent-smith"]).unwrap();
        // `debug` is omitted from the pattern: it is env-backed
        // (`JACKIN_DEBUG`), so its default depends on the runner's env.
        // `tests/cli_debug_env.rs` covers the env-driven behavior.
        assert!(matches!(
            cli.command,
            Some(Command::Load(super::LoadArgs {
                selector: Some(ref s),
                target: None,
                no_intro: false,
                ..
            })) if s == "agent-smith"
        ));
    }

    #[test]
    fn parses_load_without_selector() {
        let cli = Cli::try_parse_from(["jackin", "load"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Load(super::LoadArgs {
                selector: None,
                target: None,
                ..
            }))
        ));
    }

    #[test]
    fn parses_load_rebuild_without_selector() {
        let cli = Cli::try_parse_from(["jackin", "load", "--rebuild"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Load(super::LoadArgs {
                selector: None,
                rebuild: true,
                ..
            }))
        ));
    }

    #[test]
    fn parses_load_with_target_path() {
        let cli =
            Cli::try_parse_from(["jackin", "load", "agent-smith", "~/Projects/my-app"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Load(super::LoadArgs {
                target: Some(ref t),
                ..
            })) if t == "~/Projects/my-app"
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
            Some(Command::Load(super::LoadArgs {
                target: Some(ref t),
                ref mounts,
                ..
            })) if t == "big-monorepo" && mounts.len() == 1
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
            Some(Command::Load(super::LoadArgs {
                target: None,
                ref mounts,
                ..
            })) if mounts.len() == 2
        ));
    }

    #[test]
    fn parses_launch_command() {
        let cli = Cli::try_parse_from(["jackin", "launch"]).unwrap();
        // See `parses_load_command` for why `debug` is matched with `..`.
        assert!(matches!(
            cli.command,
            Some(Command::Launch(super::LaunchArgs { .. }))
        ));
    }

    #[test]
    fn parses_console_command() {
        let cli = Cli::try_parse_from(["jackin", "console"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Console(super::ConsoleArgs { .. }))
        ));
    }

    #[test]
    fn parses_console_with_debug() {
        let cli = Cli::try_parse_from(["jackin", "console", "--debug"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Console(super::ConsoleArgs { debug: true }))
        ));
    }

    #[test]
    fn parses_bare_jackin_as_no_subcommand() {
        let cli = Cli::try_parse_from(["jackin"]).unwrap();
        assert!(cli.command.is_none());
        // `console_args.debug` not asserted here: env-backed (see
        // `LoadArgs.debug` / `parses_load_command`).
    }

    #[test]
    fn parses_bare_jackin_with_top_level_debug() {
        let cli = Cli::try_parse_from(["jackin", "--debug"]).unwrap();
        assert!(cli.command.is_none());
        // CLI flag wins over env, so this assertion holds even when
        // `JACKIN_DEBUG=0` is set in the runner's env.
        assert!(cli.console_args.debug);
    }

    // ── Load help ───────────────────────────────────────────────────────

    #[test]
    fn load_help_shows_description_and_examples() {
        let help = help_text(&["jackin", "load", "--help"]);
        assert!(help.contains("Jack an role into an isolated container"));
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
        assert!(help.contains("Reattach to a running role"));
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
            Some(Command::Hardline(super::HardlineArgs { selector: None }))
        ));
    }

    #[test]
    fn parses_hardline_with_selector() {
        let cli = Cli::try_parse_from(["jackin", "hardline", "agent-smith"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Hardline(super::HardlineArgs { selector: Some(ref s) })) if s == "agent-smith"
        ));
    }
}
