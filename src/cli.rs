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

macro_rules! banner {
    () => {
        r"
    │ │╷│ │╷│ ╷  │╷│ │╷│ │╷│
    │ ╵│ │╵│ ╵ ╷ ╵│ │╵│ │╵│
    ╵  ╵ ╵ ╵  │  ╵ ╵ ╵ ╵ ╵
               ╵
          j a c k i n
       operator terminal
"
    };
}

const BANNER: &str = banner!();
const SUB_BANNER: &str = concat!("\n", banner!());

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
    #[command(before_help = SUB_BANNER, styles = HELP_STYLES)]
    Load {
        /// Agent class selector (e.g. agent-smith, chainargos/agent-brown)
        selector: String,
        /// Direct path to a directory to mount as the agent's workspace
        #[arg(value_name = "PATH", conflicts_with_all = ["workspace", "mounts", "workdir"])]
        path: Option<String>,
        /// Use a previously saved workspace by name
        #[arg(short = 'w', long = "workspace", conflicts_with_all = ["path", "mounts", "workdir"])]
        workspace: Option<String>,
        /// Bind-mount spec as src:dst[:ro] (repeatable)
        #[arg(long = "mount", conflicts_with_all = ["path", "workspace"])]
        mounts: Vec<String>,
        /// Working directory inside the container (required with --mount)
        #[arg(long, requires = "mounts", conflicts_with_all = ["path", "workspace"])]
        workdir: Option<String>,
        /// Skip the animated intro sequence
        #[arg(long, default_value_t = false)]
        no_intro: bool,
        /// Print raw container output for troubleshooting
        #[arg(long, default_value_t = false)]
        debug: bool,
    },
    /// Reattach to a running agent's session
    #[command(before_help = SUB_BANNER, styles = HELP_STYLES)]
    Hardline {
        /// Name of the running container to reconnect to
        container: String,
    },
    /// Pull an agent out of the Matrix
    #[command(before_help = SUB_BANNER, styles = HELP_STYLES)]
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
    #[command(before_help = SUB_BANNER, styles = HELP_STYLES)]
    Exile,
    /// Delete persisted state for an agent class
    #[command(before_help = SUB_BANNER, styles = HELP_STYLES)]
    Purge {
        /// Agent class selector (e.g. agent-smith, chainargos/agent-brown)
        selector: String,
        /// Delete state for every instance, not just the default
        #[arg(long)]
        all: bool,
    },
    /// Open the interactive TUI launcher to pick a workspace and agent
    #[command(before_help = SUB_BANNER, styles = HELP_STYLES)]
    Launch,
    /// Manage saved workspaces
    #[command(before_help = SUB_BANNER, styles = HELP_STYLES)]
    Workspace {
        #[command(subcommand)]
        command: WorkspaceCommand,
    },
    /// View and modify operator configuration
    #[command(before_help = SUB_BANNER, styles = HELP_STYLES)]
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum ConfigCommand {
    /// Manage global mount configurations
    #[command(before_help = SUB_BANNER, styles = HELP_STYLES)]
    Mount {
        #[command(subcommand)]
        command: MountCommand,
    },
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum WorkspaceCommand {
    /// Save a new workspace definition
    #[command(before_help = SUB_BANNER, styles = HELP_STYLES)]
    Add {
        /// Unique name for this workspace
        name: String,
        /// Working directory inside the container
        #[arg(long)]
        workdir: String,
        /// Bind-mount spec as src:dst[:ro] (repeatable, at least one required)
        #[arg(long = "mount", required = true)]
        mounts: Vec<String>,
        /// Restrict which agents may use this workspace (repeatable)
        #[arg(long = "allowed-agent")]
        allowed_agents: Vec<String>,
        /// Agent to select by default when loading this workspace
        #[arg(long = "default-agent")]
        default_agent: Option<String>,
    },
    /// List all saved workspaces
    #[command(before_help = SUB_BANNER, styles = HELP_STYLES)]
    List,
    /// Display details of a saved workspace
    #[command(before_help = SUB_BANNER, styles = HELP_STYLES)]
    Show {
        /// Name of the workspace to display
        name: String,
    },
    /// Modify an existing workspace
    #[command(before_help = SUB_BANNER, styles = HELP_STYLES)]
    Edit {
        /// Name of the workspace to modify
        name: String,
        /// Update the container working directory
        #[arg(long)]
        workdir: Option<String>,
        /// Add a bind-mount spec as src:dst[:ro] (repeatable)
        #[arg(long = "mount")]
        mounts: Vec<String>,
        /// Remove a mount by its container destination path (repeatable)
        #[arg(long = "remove-destination")]
        remove_destinations: Vec<String>,
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
    #[command(before_help = SUB_BANNER, styles = HELP_STYLES)]
    Remove {
        /// Name of the workspace to delete
        name: String,
    },
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum MountCommand {
    /// Register a new global mount applied to matching agents
    #[command(before_help = SUB_BANNER, styles = HELP_STYLES)]
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
        /// Apply only to matching agents (e.g. "chainargos/*" or "chainargos/agent-brown")
        #[arg(long)]
        scope: Option<String>,
    },
    /// Unregister a global mount by name
    #[command(before_help = SUB_BANNER, styles = HELP_STYLES)]
    Remove {
        /// Name of the mount to remove
        name: String,
        /// Only remove from this scope (leave other scopes untouched)
        #[arg(long)]
        scope: Option<String>,
    },
    /// List all registered global mounts
    #[command(before_help = SUB_BANNER, styles = HELP_STYLES)]
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
    fn parses_load_command() {
        let cli = Cli::try_parse_from(["jackin", "load", "agent-smith"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Load {
                ref selector,
                path: None,
                workspace: None,
                workdir: None,
                no_intro: false,
                debug: false,
                ..
            } if selector == "agent-smith"
        ));
    }

    #[test]
    fn parses_load_with_workspace_short_flag() {
        let cli =
            Cli::try_parse_from(["jackin", "load", "agent-smith", "-w", "big-monorepo"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Load {
                workspace: Some(_),
                ..
            }
        ));
    }

    #[test]
    fn parses_load_with_custom_mounts() {
        let cli = Cli::try_parse_from([
            "jackin",
            "load",
            "agent-smith",
            "--mount",
            "/tmp/project:/workspace/project",
            "--mount",
            "/tmp/cache:/workspace/cache:ro",
            "--workdir",
            "/workspace/project",
        ])
        .unwrap();

        assert!(matches!(cli.command, Command::Load { .. }));
    }

    #[test]
    fn parses_workspace_add_command() {
        let cli = Cli::try_parse_from([
            "jackin",
            "workspace",
            "add",
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
                command: WorkspaceCommand::Add { .. }
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

    #[test]
    fn help_contains_banner_and_matrix_descriptions() {
        let err = Cli::try_parse_from(["jackin", "--help"]).unwrap_err();
        let help = err.to_string();
        assert!(help.contains("j a c k i n"), "banner missing");
        assert!(help.contains("operator terminal"), "banner tagline missing");
        assert!(
            help.contains("Send agents into the Matrix"),
            "about text missing"
        );
    }

    #[test]
    fn load_help_contains_matrix_description() {
        let err = Cli::try_parse_from(["jackin", "load", "--help"]).unwrap_err();
        let help = err.to_string();
        assert!(
            help.contains("Jack an agent into the Matrix"),
            "load description missing"
        );
    }
}
