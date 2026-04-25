use clap::Subcommand;
use std::str::FromStr;

use super::{BANNER, HELP_STYLES};
use crate::isolation::MountIsolation;

fn parse_mount_isolation(s: &str) -> anyhow::Result<(String, MountIsolation)> {
    let (dst, ty) = s
        .split_once('=')
        .ok_or_else(|| anyhow::anyhow!("expected DST=TYPE, got `{s}`"))?;
    if dst.is_empty() {
        anyhow::bail!("mount destination cannot be empty in `{s}`");
    }
    let mode = MountIsolation::from_str(ty)?;
    if matches!(mode, MountIsolation::Clone) {
        anyhow::bail!(
            "isolation mode `clone` is reserved for a future release but not implemented yet"
        );
    }
    Ok((dst.into(), mode))
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
        /// Set isolation mode for a mount destination. Repeatable.
        /// Format: `<container-dst>=<shared|worktree>`.
        #[arg(
            long = "mount-isolation",
            value_name = "DST=TYPE",
            value_parser = parse_mount_isolation,
            action = clap::ArgAction::Append
        )]
        mount_isolation: Vec<(String, MountIsolation)>,
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
  jackin workspace edit my-app --clear-default-agent
  jackin workspace edit my-app --mount ~/Projects/my-app --yes
  jackin workspace edit my-app --prune"
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
        /// Skip confirmation prompts for mount collapses
        #[arg(long = "yes", short = 'y', default_value_t = false)]
        assume_yes: bool,
        /// Also remove pre-existing redundant mounts (rule-C violations) as part of this edit
        #[arg(long, default_value_t = false)]
        prune: bool,
        /// Set isolation mode for a mount destination. Repeatable.
        /// Format: `<container-dst>=<shared|worktree>`.
        #[arg(
            long = "mount-isolation",
            value_name = "DST=TYPE",
            value_parser = parse_mount_isolation,
            action = clap::ArgAction::Append
        )]
        mount_isolation: Vec<(String, MountIsolation)>,
        /// Allow this edit to delete preserved isolated worktree state.
        /// Required when --mount source changes for a mount whose dst has
        /// active isolation records on a stopped container.
        #[arg(long)]
        delete_isolated_state: bool,
    },
    /// Remove redundant mounts (rule-C violations) from a saved workspace
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin workspace prune my-app
  jackin workspace prune my-app --yes"
    )]
    Prune {
        /// Name of the workspace to prune
        name: String,
        /// Skip the confirmation prompt
        #[arg(long = "yes", short = 'y', default_value_t = false)]
        assume_yes: bool,
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
    /// Manage operator env vars at workspace and workspace-agent scope
    #[command(subcommand, before_help = BANNER, styles = HELP_STYLES)]
    Env(WorkspaceEnvCommand),
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum WorkspaceEnvCommand {
    /// Set an env var at workspace or workspace-agent scope
    ///
    /// Without `--agent`, writes to `[workspaces.<workspace>.env]`. With
    /// `--agent <SELECTOR>`, writes to `[workspaces.<workspace>.agents.<selector>.env]`.
    /// The agent selector is not pre-validated.
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin workspace env set prod DB_URL \"op://Work/Prod/db-url\"
  jackin workspace env set prod OPENAI_KEY \"op://Work/OpenAI/key\" --agent agent-smith
  jackin workspace env set prod DEBUG \"1\" --comment \"temporary; remove after Q2\""
    )]
    Set {
        /// Workspace name
        workspace: String,
        /// Env var name (stored verbatim; no POSIX validation)
        key: String,
        /// Env var value (use `op://...`, `$VAR`, `${VAR}`, or literal)
        value: String,
        /// Apply to a specific agent inside this workspace
        #[arg(long)]
        agent: Option<String>,
        /// Write a TOML comment line above the key
        #[arg(long)]
        comment: Option<String>,
    },
    /// Unset an env var at workspace or workspace-agent scope
    ///
    /// Idempotent: if the key is not present, prints "KEY not set." and
    /// exits 0 without saving the config.
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin workspace env unset prod DB_URL
  jackin workspace env unset prod OPENAI_KEY --agent agent-smith"
    )]
    Unset {
        /// Workspace name
        workspace: String,
        /// Env var name to remove
        key: String,
        /// Unset from a specific agent inside this workspace
        #[arg(long)]
        agent: Option<String>,
    },
    /// List env vars at workspace or workspace-agent scope
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin workspace env list prod
  jackin workspace env list prod --agent agent-smith"
    )]
    List {
        /// Workspace name
        workspace: String,
        /// List vars for a specific agent inside this workspace
        #[arg(long)]
        agent: Option<String>,
    },
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
            Some(Command::Workspace(WorkspaceCommand::Create { .. }))
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
            Some(Command::Workspace(WorkspaceCommand::Create {
                no_workdir_mount: false,
                ..
            }))
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
            Some(Command::Workspace(WorkspaceCommand::Create {
                no_workdir_mount: true,
                ..
            }))
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
            Some(Command::Workspace(WorkspaceCommand::Edit { .. }))
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
            Some(Command::Workspace(WorkspaceCommand::Edit {
                no_workdir_mount: true,
                ..
            }))
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
    fn parses_workspace_edit_with_yes_flag() {
        let cli = Cli::try_parse_from([
            "jackin",
            "workspace",
            "edit",
            "proj-alpha",
            "--mount",
            "/tmp/proj-alpha",
            "--yes",
        ])
        .unwrap();
        match cli.command {
            Some(Command::Workspace(WorkspaceCommand::Edit { assume_yes, .. })) => {
                assert!(assume_yes);
            }
            other => panic!("unexpected command {other:?}"),
        }
    }

    #[test]
    fn parses_workspace_edit_with_prune_flag() {
        let cli =
            Cli::try_parse_from(["jackin", "workspace", "edit", "proj-alpha", "--prune"]).unwrap();
        match cli.command {
            Some(Command::Workspace(WorkspaceCommand::Edit { prune, .. })) => assert!(prune),
            other => panic!("unexpected command {other:?}"),
        }
    }

    #[test]
    fn parses_workspace_edit_with_yes_short_form() {
        let cli = Cli::try_parse_from(["jackin", "workspace", "edit", "proj-alpha", "-y"]).unwrap();
        match cli.command {
            Some(Command::Workspace(WorkspaceCommand::Edit { assume_yes, .. })) => {
                assert!(assume_yes);
            }
            other => panic!("unexpected command {other:?}"),
        }
    }

    #[test]
    fn parses_workspace_prune_command() {
        let cli = Cli::try_parse_from(["jackin", "workspace", "prune", "proj-alpha"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Workspace(WorkspaceCommand::Prune { .. }))
        ));
    }

    #[test]
    fn parses_workspace_prune_with_yes() {
        let cli =
            Cli::try_parse_from(["jackin", "workspace", "prune", "proj-alpha", "--yes"]).unwrap();
        match cli.command {
            Some(Command::Workspace(WorkspaceCommand::Prune { assume_yes, .. })) => {
                assert!(assume_yes);
            }
            other => panic!("unexpected command {other:?}"),
        }
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

    #[test]
    fn parse_mount_isolation_accepts_worktree() {
        let (dst, mode) = parse_mount_isolation("/workspace/jackin=worktree").unwrap();
        assert_eq!(dst, "/workspace/jackin");
        assert_eq!(mode, MountIsolation::Worktree);
    }

    #[test]
    fn parse_mount_isolation_rejects_clone() {
        let err = parse_mount_isolation("/workspace/jackin=clone").unwrap_err();
        assert!(err.to_string().contains("not implemented yet"));
    }

    #[test]
    fn parse_mount_isolation_rejects_missing_equals() {
        let err = parse_mount_isolation("/workspace/jackin").unwrap_err();
        assert!(err.to_string().contains("expected DST=TYPE"));
    }

    #[test]
    fn parse_mount_isolation_rejects_empty_dst() {
        let err = parse_mount_isolation("=worktree").unwrap_err();
        assert!(err.to_string().contains("destination cannot be empty"));
    }
}
