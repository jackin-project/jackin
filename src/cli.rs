use clap::builder::styling::{AnsiColor, Effects, Styles};
use clap::{Parser, Subcommand};

const HELP_STYLES: Styles = Styles::styled()
    .header(AnsiColor::BrightGreen.on_default().effects(Effects::BOLD))
    .usage(AnsiColor::BrightGreen.on_default().effects(Effects::BOLD))
    .literal(AnsiColor::Green.on_default().effects(Effects::BOLD))
    .placeholder(AnsiColor::White.on_default())
    .valid(AnsiColor::BrightGreen.on_default())
    .invalid(AnsiColor::Red.on_default().effects(Effects::BOLD))
    .error(AnsiColor::Red.on_default().effects(Effects::BOLD));

const BANNER: &str = concat!(
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

/// Send agents into the Matrix
#[derive(Debug, Parser)]
#[command(name = "jackin", version = env!("JACKIN_VERSION"), styles = HELP_STYLES, before_help = BANNER)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum Command {
    /// Jack an agent into the Matrix
    ///
    /// TARGET can be a path (~/Projects/my-app), a path with container
    /// destination (~/Projects/my-app:/app), or a saved workspace name.
    /// When omitted, the current directory is used.
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
    Load {
        /// Agent class selector (e.g. `agent-smith`, `chainargos/agent-brown`).
        /// When omitted, uses the last-used or default agent for the workspace.
        selector: Option<String>,
        /// Path, `path:container-dest`, or saved workspace name
        #[arg(value_name = "TARGET")]
        target: Option<String>,
        /// Additional bind-mount spec as `path[:ro]` or `src:dst[:ro]` (repeatable)
        #[arg(long = "mount")]
        mounts: Vec<String>,
        /// Force rebuild the Docker image (updates Claude to latest version)
        #[arg(long, default_value_t = false)]
        rebuild: bool,
        /// Skip the animated intro sequence
        #[arg(long, default_value_t = false)]
        no_intro: bool,
        /// Print raw container output for troubleshooting
        #[arg(long, default_value_t = false)]
        debug: bool,
    },
    /// Reattach to a running agent's session
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin hardline agent-smith
  jackin hardline chainargos/the-architect
  jackin hardline jackin-agent-smith-clone-1"
    )]
    Hardline {
        /// Agent class selector or container name to reconnect to
        selector: String,
    },
    /// Pull an agent out of the Matrix
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
    Eject {
        /// Agent class selector or container name to stop
        selector: String,
        /// Stop every running instance of this agent class
        #[arg(long)]
        all: bool,
        /// Also delete persisted state after stopping
        #[arg(long)]
        purge: bool,
    },
    /// Pull every running agent out at once
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Exile,
    /// Delete persisted state for an agent class
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin purge agent-smith
  jackin purge agent-smith --all
  jackin purge chainargos/the-architect"
    )]
    Purge {
        /// Agent class selector (e.g. `agent-smith`, `chainargos/agent-brown`)
        selector: String,
        /// Delete state for every instance, not just the default
        #[arg(long)]
        all: bool,
    },
    /// Open the interactive TUI launcher to pick a workspace and agent
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Launch,
    /// Manage saved workspaces
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Workspace {
        #[command(subcommand)]
        command: WorkspaceCommand,
    },
    /// View and modify operator configuration
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum ConfigCommand {
    /// Manage global mount configurations
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Mount {
        #[command(subcommand)]
        command: MountCommand,
    },
    /// Manage trust for third-party agent sources
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Trust {
        #[command(subcommand)]
        command: TrustCommand,
    },
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum WorkspaceCommand {
    /// Create a new workspace definition
    ///
    /// By default the workdir path is automatically mounted into the container
    /// at the same location (host path = container path). Use --no-workdir-mount
    /// to disable this and provide all mounts explicitly.
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin workspace create my-app --workdir ~/Projects/my-app
  jackin workspace create my-app --workdir ~/Projects/my-app --mount ~/cache:/cache:ro
  jackin workspace create monorepo --workdir /workspace --no-workdir-mount --mount ~/src:/workspace
  jackin workspace create restricted --workdir ~/app --allowed-agent agent-smith --default-agent agent-smith"
    )]
    Create {
        /// Unique name for this workspace
        name: String,
        /// Working directory (automatically mounted at the same path unless --no-workdir-mount)
        #[arg(long)]
        workdir: String,
        /// Additional bind-mount spec as `path[:ro]` or `src:dst[:ro]` (repeatable)
        #[arg(long = "mount")]
        mounts: Vec<String>,
        /// Do not auto-mount the workdir; provide all mounts explicitly with --mount
        #[arg(long, default_value_t = false)]
        no_workdir_mount: bool,
        /// Restrict which agents may use this workspace (repeatable)
        #[arg(long = "allowed-agent")]
        allowed_agents: Vec<String>,
        /// Agent to select by default when loading this workspace
        #[arg(long = "default-agent")]
        default_agent: Option<String>,
    },
    /// List all saved workspaces
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    List,
    /// Display details of a saved workspace
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin workspace show my-app"
    )]
    Show {
        /// Name of the workspace to display
        name: String,
    },
    /// Modify an existing workspace
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin workspace edit my-app --workdir ~/new-dir
  jackin workspace edit my-app --mount ~/cache:/cache:ro
  jackin workspace edit my-app --remove-destination /old-mount
  jackin workspace edit my-app --no-workdir-mount
  jackin workspace edit my-app --allowed-agent chainargos/the-architect
  jackin workspace edit my-app --default-agent agent-smith
  jackin workspace edit my-app --clear-default-agent"
    )]
    Edit {
        /// Name of the workspace to modify
        name: String,
        /// Update the container working directory
        #[arg(long)]
        workdir: Option<String>,
        /// Add a bind-mount spec as `path[:ro]` or `src:dst[:ro]` (repeatable)
        #[arg(long = "mount")]
        mounts: Vec<String>,
        /// Remove a mount by its container destination path (repeatable)
        #[arg(long = "remove-destination")]
        remove_destinations: Vec<String>,
        /// Remove the auto-mounted workdir (the mount where src = dst = workdir)
        #[arg(long, default_value_t = false)]
        no_workdir_mount: bool,
        /// Grant an agent access to this workspace (repeatable)
        #[arg(long = "allowed-agent")]
        allowed_agents: Vec<String>,
        /// Revoke an agent's access to this workspace (repeatable)
        #[arg(long = "remove-allowed-agent")]
        remove_allowed_agents: Vec<String>,
        /// Set the default agent for this workspace
        #[arg(long = "default-agent")]
        default_agent: Option<String>,
        /// Clear the current default agent
        #[arg(
            long = "clear-default-agent",
            conflicts_with = "default_agent",
            default_value_t = false
        )]
        clear_default_agent: bool,
    },
    /// Delete a saved workspace
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin workspace remove my-app"
    )]
    Remove {
        /// Name of the workspace to delete
        name: String,
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
    use super::*;

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
            Command::Config {
                command: ConfigCommand::Mount {
                    command: MountCommand::Add { .. }
                }
            }
        ));
    }

    #[test]
    fn parses_config_mount_remove() {
        let cli =
            Cli::try_parse_from(["jackin", "config", "mount", "remove", "gradle-cache"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Config {
                command: ConfigCommand::Mount {
                    command: MountCommand::Remove { .. }
                }
            }
        ));
    }

    #[test]
    fn parses_config_mount_list() {
        let cli = Cli::try_parse_from(["jackin", "config", "mount", "list"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Config {
                command: ConfigCommand::Mount {
                    command: MountCommand::List
                }
            }
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
            Command::Config {
                command: ConfigCommand::Trust {
                    command: TrustCommand::Grant { .. }
                }
            }
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
            Command::Config {
                command: ConfigCommand::Trust {
                    command: TrustCommand::Revoke { .. }
                }
            }
        ));
    }

    #[test]
    fn parses_config_trust_list() {
        let cli = Cli::try_parse_from(["jackin", "config", "trust", "list"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Config {
                command: ConfigCommand::Trust {
                    command: TrustCommand::List
                }
            }
        ));
    }

    #[test]
    fn parses_load_command() {
        let cli = Cli::try_parse_from(["jackin", "load", "agent-smith"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Load {
                selector: Some(ref s),
                target: None,
                no_intro: false,
                debug: false,
                ..
            } if s == "agent-smith"
        ));
    }

    #[test]
    fn parses_load_without_selector() {
        let cli = Cli::try_parse_from(["jackin", "load"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Load {
                selector: None,
                target: None,
                ..
            }
        ));
    }

    #[test]
    fn parses_load_rebuild_without_selector() {
        let cli = Cli::try_parse_from(["jackin", "load", "--rebuild"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Load {
                selector: None,
                rebuild: true,
                ..
            }
        ));
    }

    #[test]
    fn parses_load_with_target_path() {
        let cli =
            Cli::try_parse_from(["jackin", "load", "agent-smith", "~/Projects/my-app"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Load {
                target: Some(ref t),
                ..
            } if t == "~/Projects/my-app"
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
            Command::Load {
                target: Some(ref t),
                ref mounts,
                ..
            } if t == "big-monorepo" && mounts.len() == 1
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
            Command::Load {
                target: None,
                ref mounts,
                ..
            } if mounts.len() == 2
        ));
    }

    #[test]
    fn parses_workspace_create_command() {
        let cli = Cli::try_parse_from([
            "jackin",
            "workspace",
            "create",
            "big-monorepo",
            "--workdir",
            "/workspace/project",
            "--mount",
            "/tmp/project:/workspace/project",
            "--mount",
            "/tmp/cache:/workspace/cache:ro",
            "--allowed-agent",
            "agent-smith",
            "--default-agent",
            "agent-smith",
        ])
        .unwrap();

        assert!(matches!(
            cli.command,
            Command::Workspace {
                command: WorkspaceCommand::Create { .. }
            }
        ));
    }

    #[test]
    fn parses_workspace_create_with_workdir_only() {
        let cli = Cli::try_parse_from([
            "jackin",
            "workspace",
            "create",
            "my-app",
            "--workdir",
            "/tmp/my-app",
        ])
        .unwrap();

        assert!(matches!(
            cli.command,
            Command::Workspace {
                command: WorkspaceCommand::Create {
                    no_workdir_mount: false,
                    ..
                }
            }
        ));
    }

    #[test]
    fn parses_workspace_create_with_no_workdir_mount() {
        let cli = Cli::try_parse_from([
            "jackin",
            "workspace",
            "create",
            "monorepo",
            "--workdir",
            "/workspace",
            "--no-workdir-mount",
            "--mount",
            "/tmp/src:/workspace",
        ])
        .unwrap();

        assert!(matches!(
            cli.command,
            Command::Workspace {
                command: WorkspaceCommand::Create {
                    no_workdir_mount: true,
                    ..
                }
            }
        ));
    }

    #[test]
    fn parses_workspace_edit_command() {
        let cli = Cli::try_parse_from([
            "jackin",
            "workspace",
            "edit",
            "big-monorepo",
            "--mount",
            "/tmp/new-cache:/workspace/cache:ro",
            "--remove-destination",
            "/workspace/shared",
        ])
        .unwrap();

        assert!(matches!(
            cli.command,
            Command::Workspace {
                command: WorkspaceCommand::Edit { .. }
            }
        ));
    }

    #[test]
    fn parses_workspace_edit_with_no_workdir_mount() {
        let cli = Cli::try_parse_from([
            "jackin",
            "workspace",
            "edit",
            "my-app",
            "--no-workdir-mount",
        ])
        .unwrap();

        assert!(matches!(
            cli.command,
            Command::Workspace {
                command: WorkspaceCommand::Edit {
                    no_workdir_mount: true,
                    ..
                }
            }
        ));
    }

    #[test]
    fn rejects_conflicting_workspace_edit_default_agent_flags() {
        let err = Cli::try_parse_from([
            "jackin",
            "workspace",
            "edit",
            "big-monorepo",
            "--default-agent",
            "agent-smith",
            "--clear-default-agent",
        ])
        .unwrap_err();

        assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
    }

    #[test]
    fn parses_launch_command() {
        let cli = Cli::try_parse_from(["jackin", "launch"]).unwrap();
        assert!(matches!(cli.command, Command::Launch));
    }

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
        assert!(help.contains("Send agents into the Matrix"));
        for cmd in [
            "load",
            "hardline",
            "eject",
            "exile",
            "purge",
            "launch",
            "workspace",
            "config",
        ] {
            assert!(help.contains(cmd), "missing command: {cmd}");
        }
    }

    // ── Load help ───────────────────────────────────────────────────────

    #[test]
    fn load_help_shows_description_and_examples() {
        let help = help_text(&["jackin", "load", "--help"]);
        assert!(help.contains("Jack an agent into the Matrix"));
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
    }

    // ── Eject help ──────────────────────────────────────────────────────

    #[test]
    fn eject_help_shows_examples() {
        let help = help_text(&["jackin", "eject", "--help"]);
        assert!(help.contains("Pull an agent out"));
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

    // ── Workspace subcommand help ───────────────────────────────────────

    #[test]
    fn workspace_create_help_shows_auto_mount_and_examples() {
        let help = help_text(&["jackin", "workspace", "create", "--help"]);
        assert!(
            help.contains("automatically mounted"),
            "auto-mount behavior not documented"
        );
        assert!(help.contains("--no-workdir-mount"), "opt-out flag missing");
        assert!(help.contains("Examples:"));
        assert!(help.contains("jackin workspace create my-app --workdir ~/Projects/my-app"));
    }

    #[test]
    fn workspace_create_help_shows_mount_format() {
        let help = help_text(&["jackin", "workspace", "create", "--help"]);
        assert!(
            help.contains("path[:ro]") && help.contains("src:dst[:ro]"),
            "mount format missing"
        );
    }

    #[test]
    fn workspace_edit_help_shows_examples() {
        let help = help_text(&["jackin", "workspace", "edit", "--help"]);
        assert!(help.contains("Modify an existing workspace"));
        assert!(help.contains("Examples:"));
        assert!(help.contains("jackin workspace edit my-app --workdir ~/new-dir"));
        assert!(help.contains("--clear-default-agent"));
    }

    #[test]
    fn workspace_edit_help_shows_mount_format() {
        let help = help_text(&["jackin", "workspace", "edit", "--help"]);
        assert!(
            help.contains("path[:ro]") && help.contains("src:dst[:ro]"),
            "mount format missing"
        );
    }

    #[test]
    fn workspace_show_help_shows_examples() {
        let help = help_text(&["jackin", "workspace", "show", "--help"]);
        assert!(help.contains("jackin workspace show my-app"));
    }

    #[test]
    fn workspace_remove_help_shows_examples() {
        let help = help_text(&["jackin", "workspace", "remove", "--help"]);
        assert!(help.contains("jackin workspace remove my-app"));
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

    // ── Subcommand banner consistency ───────────────────────────────────

    #[test]
    fn all_subcommand_help_pages_show_banner() {
        let subcommands = [
            vec!["jackin", "load", "--help"],
            vec!["jackin", "hardline", "--help"],
            vec!["jackin", "eject", "--help"],
            vec!["jackin", "exile", "--help"],
            vec!["jackin", "purge", "--help"],
            vec!["jackin", "launch", "--help"],
            vec!["jackin", "workspace", "create", "--help"],
            vec!["jackin", "workspace", "list", "--help"],
            vec!["jackin", "workspace", "show", "--help"],
            vec!["jackin", "workspace", "edit", "--help"],
            vec!["jackin", "workspace", "remove", "--help"],
            vec!["jackin", "config", "mount", "add", "--help"],
            vec!["jackin", "config", "mount", "remove", "--help"],
            vec!["jackin", "config", "mount", "list", "--help"],
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
