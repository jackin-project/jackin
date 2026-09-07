// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! CLI argument structs for `jackin workspace` subcommands (add, remove, list, edit).
//!
//! Not responsible for: reading or writing workspace state — structs are parsed
//! by `clap` and dispatched to handlers in `src/commands/workspace/`.

use clap::{Args, Subcommand};
use std::str::FromStr;

use super::{BANNER, HELP_STYLES};
use jackin_core::MountIsolation;

fn parse_mount_isolation(s: &str) -> anyhow::Result<(String, MountIsolation)> {
    let (dst, ty) = s
        .split_once('=')
        .ok_or_else(|| anyhow::anyhow!("expected DST=TYPE, got `{s}`"))?;
    if dst.is_empty() {
        anyhow::bail!("mount destination cannot be empty in `{s}`");
    }
    let mode = MountIsolation::from_str(ty)?;
    Ok((dst.into(), mode))
}

fn parse_agent(s: &str) -> Result<jackin_core::Agent, String> {
    s.parse()
        .map_err(|e: jackin_core::ParseAgentError| e.to_string())
}

/// Shared args for read-only workspace subcommands that support `--format`.
#[derive(Debug, Args, PartialEq, Eq)]
pub struct WorkspaceFormatArgs {
    /// Output format (`human` or `json`)
    #[arg(long, value_name = "FORMAT", default_value = "human")]
    pub format: String,
}

/// Args for `jackin workspace list`
pub type WorkspaceListArgs = WorkspaceFormatArgs;

/// Args for `jackin workspace show`
#[derive(Debug, Args, PartialEq, Eq)]
pub struct WorkspaceShowArgs {
    /// Name of the workspace to display
    pub name: String,
    #[command(flatten)]
    pub fmt: WorkspaceFormatArgs,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum WorkspaceCommand {
    /// Create a new workspace definition
    ///
    /// The workdir is the path jackin starts the agent in. It is not mounted
    /// implicitly; provide one or more --mount entries for the directories the
    /// container should see.
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin workspace create my-app --workdir ~/Projects/my-app --mount ~/Projects/my-app
  jackin workspace create my-app --workdir ~/Projects/my-app --mount ~/Projects/my-app --mount ~/cache:/cache:ro
  jackin workspace create my-app --workdir ~/Projects/my-app --mount ~/Projects/my-app --default-agent amp
  jackin workspace create monorepo --workdir /workspace --mount ~/src:/workspace
  jackin workspace create restricted --workdir ~/app --mount ~/app --allowed-role agent-smith --default-role agent-smith"
    )]
    Create {
        /// Unique name for this workspace
        name: String,
        /// Working directory inside the container
        #[arg(long)]
        workdir: String,
        /// Additional bind-mount spec as `path[:ro]` or `src:dst[:ro]` (repeatable)
        #[arg(long = "mount", required = true)]
        mounts: Vec<String>,
        /// Restrict which roles may use this workspace (repeatable)
        #[arg(long = "allowed-role")]
        allowed_roles: Vec<String>,
        /// Role to select by default when loading this workspace
        #[arg(long = "default-role")]
        default_role: Option<String>,
        /// Default agent for this workspace (claude, codex, amp, or opencode)
        #[arg(long, value_parser = parse_agent)]
        default_agent: Option<jackin_core::Agent>,
        /// Set isolation mode for a mount destination. Repeatable.
        /// Format: `<container-dst>=<shared|worktree|clone>`.
        #[arg(
            long = "mount-isolation",
            value_name = "DST=TYPE",
            value_parser = parse_mount_isolation,
            action = clap::ArgAction::Append
        )]
        mount_isolation: Vec<(String, MountIsolation)>,
        /// Opt the workspace into the macOS keep-awake reconciler.
        /// While any role in this workspace is running, jackin holds a
        /// `caffeinate -imsu` assertion so the host stays awake. Silent
        /// no-op on Linux/Windows.
        #[arg(long = "keep-awake", default_value_t = false)]
        keep_awake: bool,
        /// Run `git pull` on all mounted git repositories before starting the
        /// agent. Executed on the host. Failures are warnings — the launch
        /// continues even when offline.
        #[arg(long = "git-pull", default_value_t = false)]
        git_pull: bool,
    },
    /// List all saved workspaces
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    List(WorkspaceListArgs),
    /// Display details of a saved workspace
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin workspace show my-app"
    )]
    Show(WorkspaceShowArgs),
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
  jackin workspace edit my-app --allowed-role chainargos/the-architect
  jackin workspace edit my-app --default-role agent-smith
  jackin workspace edit my-app --clear-default-role
  jackin workspace edit my-app --default-agent amp
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
        /// Grant a role access to this workspace (repeatable)
        #[arg(long = "allowed-role")]
        allowed_roles: Vec<String>,
        /// Revoke a role's access to this workspace (repeatable)
        #[arg(long = "remove-allowed-role")]
        remove_allowed_roles: Vec<String>,
        /// Set the default role for this workspace
        #[arg(long = "default-role")]
        default_role: Option<String>,
        /// Clear the current default role
        #[arg(
            long = "clear-default-role",
            conflicts_with = "default_role",
            default_value_t = false
        )]
        clear_default_role: bool,
        /// Set the default agent for this workspace
        #[arg(long, value_parser = parse_agent)]
        default_agent: Option<jackin_core::Agent>,
        /// Clear the explicit default agent so the workspace falls back to claude
        #[arg(long, conflicts_with = "default_agent", default_value_t = false)]
        clear_default_agent: bool,
        /// Skip confirmation prompts for mount collapses
        #[arg(long = "yes", short = 'y', default_value_t = false)]
        assume_yes: bool,
        /// Also remove pre-existing redundant mounts (rule-C violations) as part of this edit
        #[arg(long, default_value_t = false)]
        prune: bool,
        /// Set isolation mode for a mount destination. Repeatable.
        /// Format: `<container-dst>=<shared|worktree|clone>`.
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
        /// Opt the workspace into the macOS keep-awake reconciler. Mutually
        /// exclusive with `--no-keep-awake`. See `workspace create`.
        #[arg(long = "keep-awake", default_value_t = false)]
        keep_awake: bool,
        /// Opt the workspace OUT of the keep-awake reconciler. Mutually
        /// exclusive with `--keep-awake`.
        #[arg(
            long = "no-keep-awake",
            conflicts_with = "keep_awake",
            default_value_t = false
        )]
        no_keep_awake: bool,
        /// Enable git pull on entry for this workspace. Mutually exclusive with
        /// `--no-git-pull`.
        #[arg(long = "git-pull", default_value_t = false)]
        git_pull: bool,
        /// Disable git pull on entry for this workspace. Mutually exclusive with
        /// `--git-pull`.
        #[arg(
            long = "no-git-pull",
            conflicts_with = "git_pull",
            default_value_t = false
        )]
        no_git_pull: bool,
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
    /// Manage operator env vars at workspace and workspace-role scope
    #[command(subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Env(WorkspaceEnvCommand),
    /// Assign accounts and choose agent defaults for this workspace
    #[command(subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Account(super::account::WorkspaceAccountCommand),
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum WorkspaceEnvCommand {
    /// Set an env var at workspace or workspace-role scope
    ///
    /// Without `--role`, scopes the env var to the whole workspace. With
    /// `--role <SELECTOR>`, narrows it to that role within the workspace.
    /// The role selector is not pre-validated.
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin workspace env set prod DB_URL \"op://Work/Prod/db-url\"
  jackin workspace env set prod OPENAI_KEY \"op://Work/OpenAI/key\" --role agent-smith
  jackin workspace env set prod DEBUG \"1\" --comment \"temporary; remove after Q2\"
  jackin workspace env set prod OP_TOKEN \"op://Work/svc/credential\" --on-demand"
    )]
    Set {
        /// Workspace name
        workspace: String,
        /// Env var name (stored verbatim; no POSIX validation)
        key: String,
        /// Env var value (use `op://...`, `$VAR`, `${VAR}`, or literal)
        value: String,
        /// Apply to a specific role inside this workspace
        #[arg(long)]
        role: Option<String>,
        /// Attach a comment to the key (recorded alongside the value)
        #[arg(long)]
        comment: Option<String>,
        /// Inject on demand at `jackin-exec` time instead of at launch. The
        /// value is never resolved during launch; the agent asks for it and
        /// the operator approves each use.
        #[arg(long = "on-demand")]
        on_demand: bool,
    },
    /// Unset an env var at workspace or workspace-role scope
    ///
    /// Idempotent: if the key is not present, prints "KEY not set." and
    /// exits 0 without saving the config.
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin workspace env unset prod DB_URL
  jackin workspace env unset prod OPENAI_KEY --role agent-smith"
    )]
    Unset {
        /// Workspace name
        workspace: String,
        /// Env var name to remove
        key: String,
        /// Unset from a specific role inside this workspace
        #[arg(long)]
        role: Option<String>,
    },
    /// List env vars at workspace or workspace-role scope
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin workspace env list prod
  jackin workspace env list prod --role agent-smith"
    )]
    List {
        /// Workspace name
        workspace: String,
        /// List vars for a specific role inside this workspace
        #[arg(long)]
        role: Option<String>,
    },
}

#[cfg(test)]
mod tests;
