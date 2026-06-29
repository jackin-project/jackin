//! CLI argument structs for `jackin workspace` subcommands (add, remove, list, edit).
//!
//! Not responsible for: reading or writing workspace state — structs are parsed
//! by `clap` and dispatched to handlers in `src/commands/workspace/`.

use clap::{Args, Subcommand};
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
    /// Manage the workspace's long-lived Claude OAuth token
    ///
    /// Wraps `claude setup-token`, the 1Password write API, and
    /// the workspace config so the operator can move from "no
    /// token" to "token mode active" in one command. See
    /// <https://jackin.tailrocks.com/reference/roadmap/workspace-claude-token-setup/>
    /// for the full design.
    #[command(subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    ClaudeToken(WorkspaceClaudeTokenCommand),
}

/// `jackin workspace claude-token <action>` — guided lifecycle for
/// the workspace's long-lived Claude OAuth token.
#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum WorkspaceClaudeTokenCommand {
    /// Generate a token, store it in 1Password, and wire the
    /// workspace config — end-to-end with no copy-paste.
    ///
    /// Requires `claude` and `op` on PATH. The first invocation for a
    /// workspace requires `--vault`; the orchestrator drops a
    /// deterministic 1Password item (default name:
    /// `jackin · {workspace} · claude-token`) and writes the
    /// canonical-slot reference into `[workspaces.<ws>.claude]`.
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin workspace claude-token setup my-app --vault Personal
  jackin workspace claude-token setup my-app --interactive
  jackin workspace claude-token setup my-app --vault Personal --item-name \"jackin · {ws} · claude\"
  jackin workspace claude-token setup my-app --reuse op://Personal/Existing/token
  jackin workspace claude-token setup my-app --vault Work --op-account Work"
    )]
    Setup {
        /// Workspace whose `[workspaces.<NAME>.claude]` block should
        /// be wired
        workspace: String,
        /// Wire the token for a specific role override
        /// (`[workspaces.<ws>.roles.<role>]`) instead of all roles in
        /// the workspace. Omit to wire the workspace-level slot.
        #[arg(long)]
        role: Option<String>,
        /// 1Password vault name or UUID for the new item. Required
        /// unless `--reuse` or `--interactive` is supplied. Mutually
        /// exclusive with `--reuse`.
        #[arg(long, conflicts_with = "reuse")]
        vault: Option<String>,
        /// Override the default item title — `{ws}` substitutes the
        /// workspace name
        #[arg(long = "item-name")]
        item_name: Option<String>,
        /// Pin this run to a specific 1Password account (UUID,
        /// label, or email). Recorded on the created `op://` ref.
        #[arg(long = "op-account")]
        op_account: Option<String>,
        /// Reuse an existing `op://` reference instead of generating
        /// a fresh token. Mutually exclusive with `--vault` (the
        /// vault is implicit in the supplied reference).
        #[arg(long, conflicts_with = "vault")]
        reuse: Option<String>,
        /// Mint and store the token as a plaintext literal in config
        /// instead of 1Password. Mutually exclusive with `--vault`,
        /// `--reuse`, and `--interactive`.
        #[arg(long, conflicts_with_all = ["vault", "reuse", "interactive"])]
        plain: bool,
        /// Interactively choose the 1Password account, vault, item, and
        /// field with CLI prompts instead of passing them as flags.
        /// Offers `[ + New item ]` / `[ + New field ]`. Mutually
        /// exclusive with `--vault`, `--reuse`, and `--plain`.
        #[arg(short = 'i', long, conflicts_with_all = ["vault", "reuse", "plain"])]
        interactive: bool,
    },
    /// Generate a fresh token and overwrite the workspace's existing
    /// canonical slot.
    ///
    /// Equivalent to `setup` with one extra step: after the new item
    /// is created and validated, the prior 1Password item (if any)
    /// is deleted so the old token cannot be silently re-used.
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin workspace claude-token rotate my-app
  jackin workspace claude-token rotate my-app --vault Personal
  jackin workspace claude-token rotate my-app --role chainargos/the-architect"
    )]
    Rotate {
        /// Workspace name
        workspace: String,
        /// Rotate the token for a specific role override
        /// (`[workspaces.<ws>.roles.<role>]`) instead of the
        /// workspace-level slot. Must match the scope `setup` wired.
        #[arg(long)]
        role: Option<String>,
        /// Override vault for the new item (defaults to the vault
        /// that holds the prior item)
        #[arg(long)]
        vault: Option<String>,
        /// Override the default item title — `{ws}` substitutes the
        /// workspace name
        #[arg(long = "item-name")]
        item_name: Option<String>,
        /// Pin this run to a specific 1Password account
        #[arg(long = "op-account")]
        op_account: Option<String>,
    },
    /// Clear the workspace's canonical slot and switch
    /// `auth_forward` to `ignore`.
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin workspace claude-token revoke my-app
  jackin workspace claude-token revoke my-app --delete-op-item"
    )]
    Revoke {
        /// Workspace name
        workspace: String,
        /// Also delete the 1P item the prior slot pointed to. No-op
        /// when the slot held a literal value rather than an op-ref.
        #[arg(long = "delete-op-item", default_value_t = false)]
        delete_op_item: bool,
    },
    /// Resolve the workspace's canonical slot through `op` and
    /// report whether the value resolves cleanly.
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin workspace claude-token doctor my-app"
    )]
    Doctor {
        /// Workspace name
        workspace: String,
    },
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
  jackin workspace env set prod DEBUG \"1\" --comment \"temporary; remove after Q2\""
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
