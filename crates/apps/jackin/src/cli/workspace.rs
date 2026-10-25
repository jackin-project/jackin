// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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

#[derive(Debug, Args, PartialEq, Eq)]
#[command(
    about = "Shared args for read-only workspace subcommands that support `--format`.",
    long_about = "Shared args for read-only workspace subcommands that support `--format`."
)]
pub struct WorkspaceFormatArgs {
    #[arg(
        help = "Output format (`human` or `json`)",
        long_help = "Output format (`human` or `json`)",
        long,
        value_name = "FORMAT",
        default_value = "human"
    )]
    pub format: String,
}

pub type WorkspaceListArgs = WorkspaceFormatArgs;

#[derive(Debug, Args, PartialEq, Eq)]
#[command(
    about = "Args for `jackin workspace show`",
    long_about = "Args for `jackin workspace show`"
)]
pub struct WorkspaceShowArgs {
    #[arg(
        help = "Name of the workspace to display",
        long_help = "Name of the workspace to display"
    )]
    pub name: String,
    #[command(flatten)]
    pub fmt: WorkspaceFormatArgs,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum WorkspaceCommand {
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
    #[command(
        about = "Create a new workspace definition",
        long_about = "Create a new workspace definition

The workdir is the path jackin starts the agent in. It is not mounted implicitly; provide one or more --mount entries for the directories the container should see."
    )]
    Create {
        #[arg(
            help = "Unique name for this workspace",
            long_help = "Unique name for this workspace"
        )]
        name: String,
        #[arg(
            help = "Working directory inside the container",
            long_help = "Working directory inside the container",
            long
        )]
        workdir: String,
        #[arg(
            help = "Additional bind-mount spec as `path[:ro]` or `src:dst[:ro]` (repeatable)",
            long_help = "Additional bind-mount spec as `path[:ro]` or `src:dst[:ro]` (repeatable)",
            long = "mount",
            required = true
        )]
        mounts: Vec<String>,
        #[arg(
            help = "Restrict which roles may use this workspace (repeatable)",
            long_help = "Restrict which roles may use this workspace (repeatable)",
            long = "allowed-role"
        )]
        allowed_roles: Vec<String>,
        #[arg(
            help = "Role to select by default when loading this workspace",
            long_help = "Role to select by default when loading this workspace",
            long = "default-role"
        )]
        default_role: Option<String>,
        #[arg(help = "Default agent for this workspace (claude, codex, amp, kimi, opencode, grok, antigravity, gemini, cursor, muse, omp, or hermes)", long_help = "Default agent for this workspace (claude, codex, amp, kimi, opencode, grok, antigravity, gemini, cursor, muse, omp, or hermes)", long, value_parser = parse_agent)]
        default_agent: Option<jackin_core::Agent>,
        #[arg(
            long = "mount-isolation",
            value_name = "DST=TYPE",
            value_parser = parse_mount_isolation,
            action = clap::ArgAction::Append
        )]
        #[arg(
            help = "Set isolation mode for a mount destination. Repeatable. Format: `<container-dst>=<shared|worktree|clone>`.",
            long_help = "Set isolation mode for a mount destination. Repeatable. Format: `<container-dst>=<shared|worktree|clone>`."
        )]
        mount_isolation: Vec<(String, MountIsolation)>,
        #[arg(
            help = "Opt the workspace into the macOS keep-awake reconciler. While any role in this workspace is running, jackin holds a `caffeinate -imsu` assertion so the host stays awake. Silent no-op on Linux/Windows.",
            long_help = "Opt the workspace into the macOS keep-awake reconciler. While any role in this workspace is running, jackin holds a `caffeinate -imsu` assertion so the host stays awake. Silent no-op on Linux/Windows.",
            long = "keep-awake",
            default_value_t = false
        )]
        keep_awake: bool,
        #[arg(
            help = "Run `git pull` on all mounted git repositories before starting the agent. Executed on the host. Failures are warnings — the launch continues even when offline.",
            long_help = "Run `git pull` on all mounted git repositories before starting the agent. Executed on the host. Failures are warnings — the launch continues even when offline.",
            long = "git-pull",
            default_value_t = false
        )]
        git_pull: bool,
    },
    #[command(about = "List all saved workspaces", long_about = "List all saved workspaces", before_help = BANNER, styles = HELP_STYLES)]
    List(WorkspaceListArgs),
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin workspace show my-app"
    )]
    #[command(
        about = "Display details of a saved workspace",
        long_about = "Display details of a saved workspace"
    )]
    Show(WorkspaceShowArgs),
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
  jackin workspace edit my-app --default-launch claude-main --default-launch codex-main
  jackin workspace edit my-app --clear-default-launch
  jackin workspace edit my-app --mount ~/Projects/my-app --yes
  jackin workspace edit my-app --prune"
    )]
    #[command(
        about = "Modify an existing workspace",
        long_about = "Modify an existing workspace"
    )]
    Edit {
        #[arg(
            help = "Name of the workspace to modify",
            long_help = "Name of the workspace to modify"
        )]
        name: String,
        #[arg(
            help = "Update the container working directory",
            long_help = "Update the container working directory",
            long
        )]
        workdir: Option<String>,
        #[arg(
            help = "Add a bind-mount spec as `path[:ro]` or `src:dst[:ro]` (repeatable)",
            long_help = "Add a bind-mount spec as `path[:ro]` or `src:dst[:ro]` (repeatable)",
            long = "mount"
        )]
        mounts: Vec<String>,
        #[arg(
            help = "Remove a mount by its container destination path (repeatable)",
            long_help = "Remove a mount by its container destination path (repeatable)",
            long = "remove-destination"
        )]
        remove_destinations: Vec<String>,
        #[arg(
            help = "Remove the auto-mounted workdir (the mount where src = dst = workdir)",
            long_help = "Remove the auto-mounted workdir (the mount where src = dst = workdir)",
            long,
            default_value_t = false
        )]
        no_workdir_mount: bool,
        #[arg(
            help = "Grant a role access to this workspace (repeatable)",
            long_help = "Grant a role access to this workspace (repeatable)",
            long = "allowed-role"
        )]
        allowed_roles: Vec<String>,
        #[arg(
            help = "Revoke a role's access to this workspace (repeatable)",
            long_help = "Revoke a role's access to this workspace (repeatable)",
            long = "remove-allowed-role"
        )]
        remove_allowed_roles: Vec<String>,
        #[arg(
            help = "Set the default role for this workspace",
            long_help = "Set the default role for this workspace",
            long = "default-role"
        )]
        default_role: Option<String>,
        #[arg(
            long = "clear-default-role",
            conflicts_with = "default_role",
            default_value_t = false
        )]
        #[arg(
            help = "Clear the current default role",
            long_help = "Clear the current default role"
        )]
        clear_default_role: bool,
        #[arg(help = "Set the default agent for this workspace", long_help = "Set the default agent for this workspace", long, value_parser = parse_agent)]
        default_agent: Option<jackin_core::Agent>,
        #[arg(
            help = "Clear the explicit default agent so the workspace falls back to claude",
            long_help = "Clear the explicit default agent so the workspace falls back to claude",
            long,
            conflicts_with = "default_agent",
            default_value_t = false
        )]
        clear_default_agent: bool,
        #[arg(
            help = "Skip confirmation prompts for mount collapses",
            long_help = "Skip confirmation prompts for mount collapses",
            long = "yes",
            short = 'y',
            default_value_t = false
        )]
        assume_yes: bool,
        #[arg(
            help = "Also remove pre-existing redundant mounts (rule-C violations) as part of this edit",
            long_help = "Also remove pre-existing redundant mounts (rule-C violations) as part of this edit",
            long,
            default_value_t = false
        )]
        prune: bool,
        #[arg(
            long = "mount-isolation",
            value_name = "DST=TYPE",
            value_parser = parse_mount_isolation,
            action = clap::ArgAction::Append
        )]
        #[arg(
            help = "Set isolation mode for a mount destination. Repeatable. Format: `<container-dst>=<shared|worktree|clone>`.",
            long_help = "Set isolation mode for a mount destination. Repeatable. Format: `<container-dst>=<shared|worktree|clone>`."
        )]
        mount_isolation: Vec<(String, MountIsolation)>,
        #[arg(
            help = "Allow this edit to delete preserved isolated worktree state. Required when --mount source changes for a mount whose dst has active isolation records on a stopped container.",
            long_help = "Allow this edit to delete preserved isolated worktree state. Required when --mount source changes for a mount whose dst has active isolation records on a stopped container.",
            long
        )]
        delete_isolated_state: bool,
        #[arg(
            help = "Opt the workspace into the macOS keep-awake reconciler. Mutually exclusive with `--no-keep-awake`. See `workspace create`.",
            long_help = "Opt the workspace into the macOS keep-awake reconciler. Mutually exclusive with `--no-keep-awake`. See `workspace create`.",
            long = "keep-awake",
            default_value_t = false
        )]
        keep_awake: bool,
        #[arg(
            long = "no-keep-awake",
            conflicts_with = "keep_awake",
            default_value_t = false
        )]
        #[arg(
            help = "Opt the workspace OUT of the keep-awake reconciler. Mutually exclusive with `--keep-awake`.",
            long_help = "Opt the workspace OUT of the keep-awake reconciler. Mutually exclusive with `--keep-awake`."
        )]
        no_keep_awake: bool,
        #[arg(
            help = "Enable git pull on entry for this workspace. Mutually exclusive with `--no-git-pull`.",
            long_help = "Enable git pull on entry for this workspace. Mutually exclusive with `--no-git-pull`.",
            long = "git-pull",
            default_value_t = false
        )]
        git_pull: bool,
        #[arg(
            long = "no-git-pull",
            conflicts_with = "git_pull",
            default_value_t = false
        )]
        #[arg(
            help = "Disable git pull on entry for this workspace. Mutually exclusive with `--git-pull`.",
            long_help = "Disable git pull on entry for this workspace. Mutually exclusive with `--git-pull`."
        )]
        no_git_pull: bool,
        #[arg(
            long = "default-launch",
            value_name = "CONFIG_ID",
            action = clap::ArgAction::Append
        )]
        #[arg(
            help = "Set the workspace launch admission set: agent-configuration IDs admitted to this workspace's containers (repeatable). Replaces the inherited global default entirely.",
            long_help = "Set the workspace launch admission set: agent-configuration IDs admitted to this workspace's containers (repeatable). Replaces the inherited global default entirely."
        )]
        default_launch: Vec<String>,
        #[arg(
            long = "clear-default-launch",
            conflicts_with = "default_launch",
            default_value_t = false
        )]
        #[arg(
            help = "Clear the workspace launch admission set so the workspace inherits the global default again.",
            long_help = "Clear the workspace launch admission set so the workspace inherits the global default again."
        )]
        clear_default_launch: bool,
    },
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin workspace prune my-app
  jackin workspace prune my-app --yes"
    )]
    #[command(
        about = "Remove redundant mounts (rule-C violations) from a saved workspace",
        long_about = "Remove redundant mounts (rule-C violations) from a saved workspace"
    )]
    Prune {
        #[arg(
            help = "Name of the workspace to prune",
            long_help = "Name of the workspace to prune"
        )]
        name: String,
        #[arg(
            help = "Skip the confirmation prompt",
            long_help = "Skip the confirmation prompt",
            long = "yes",
            short = 'y',
            default_value_t = false
        )]
        assume_yes: bool,
    },
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin workspace remove my-app"
    )]
    #[command(
        about = "Delete a saved workspace",
        long_about = "Delete a saved workspace"
    )]
    Remove {
        #[arg(
            help = "Name of the workspace to delete",
            long_help = "Name of the workspace to delete"
        )]
        name: String,
    },

    #[command(about = "Manage operator env vars at workspace and workspace-role scope", long_about = "Manage operator env vars at workspace and workspace-role scope", subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Env(WorkspaceEnvCommand),
    #[command(about = "Assign accounts and choose agent defaults for this workspace", long_about = "Assign accounts and choose agent defaults for this workspace", subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Account(super::account::WorkspaceAccountCommand),
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum WorkspaceEnvCommand {
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
    #[command(
        about = "Set an env var at workspace or workspace-role scope",
        long_about = "Set an env var at workspace or workspace-role scope

Without `--role`, scopes the env var to the whole workspace. With `--role <SELECTOR>`, narrows it to that role within the workspace. The role selector is not pre-validated."
    )]
    Set {
        #[arg(help = "Workspace name", long_help = "Workspace name")]
        workspace: String,
        #[arg(
            help = "Env var name (stored verbatim; no POSIX validation)",
            long_help = "Env var name (stored verbatim; no POSIX validation)"
        )]
        key: String,
        #[arg(
            help = "Env var value (use `op://...`, `$VAR`, `${VAR}`, or literal)",
            long_help = "Env var value (use `op://...`, `$VAR`, `${VAR}`, or literal)"
        )]
        value: String,
        #[arg(
            help = "Apply to a specific role inside this workspace",
            long_help = "Apply to a specific role inside this workspace",
            long
        )]
        role: Option<String>,
        #[arg(
            help = "Attach a comment to the key (recorded alongside the value)",
            long_help = "Attach a comment to the key (recorded alongside the value)",
            long
        )]
        comment: Option<String>,
        #[arg(
            help = "Inject on demand at `jackin-exec` time instead of at launch. The value is never resolved during launch; the agent asks for it and the operator approves each use.",
            long_help = "Inject on demand at `jackin-exec` time instead of at launch. The value is never resolved during launch; the agent asks for it and the operator approves each use.",
            long = "on-demand"
        )]
        on_demand: bool,
    },
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin workspace env unset prod DB_URL
  jackin workspace env unset prod OPENAI_KEY --role agent-smith"
    )]
    #[command(
        about = "Unset an env var at workspace or workspace-role scope",
        long_about = "Unset an env var at workspace or workspace-role scope

Idempotent: if the key is not present, prints \"KEY not set.\" and exits 0 without saving the config."
    )]
    Unset {
        #[arg(help = "Workspace name", long_help = "Workspace name")]
        workspace: String,
        #[arg(help = "Env var name to remove", long_help = "Env var name to remove")]
        key: String,
        #[arg(
            help = "Unset from a specific role inside this workspace",
            long_help = "Unset from a specific role inside this workspace",
            long
        )]
        role: Option<String>,
    },
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin workspace env list prod
  jackin workspace env list prod --role agent-smith"
    )]
    #[command(
        about = "List env vars at workspace or workspace-role scope",
        long_about = "List env vars at workspace or workspace-role scope"
    )]
    List {
        #[arg(help = "Workspace name", long_help = "Workspace name")]
        workspace: String,
        #[arg(
            help = "List vars for a specific role inside this workspace",
            long_help = "List vars for a specific role inside this workspace",
            long
        )]
        role: Option<String>,
    },
}
