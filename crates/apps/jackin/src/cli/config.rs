// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use clap::Subcommand;

use super::{BANNER, HELP_STYLES};

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum ConfigCommand {
    #[command(about = "Manage global mount configurations", long_about = "Manage global mount configurations", subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Mount(MountCommand),
    #[command(about = "Manage trust for third-party role sources", long_about = "Manage trust for third-party role sources", subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Trust(TrustCommand),
    #[command(about = "Manage operator env vars at global and per-role scope", long_about = "Manage operator env vars at global and per-role scope", subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Env(EnvCommand),
    #[command(about = "Manage global git settings", long_about = "Manage global git settings", subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Git(GitCommand),
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum GitCommand {
    #[command(about = "Configure automatic Co-authored-by trailer injection for agent commits", long_about = "Configure automatic Co-authored-by trailer injection for agent commits", subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    CoauthorTrailer(CoauthorTrailerCommand),
    #[command(about = "Configure DCO Signed-off-by trailer injection for agent commits", long_about = "Configure DCO Signed-off-by trailer injection for agent commits", subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Dco(DcoCommand),
}

#[derive(Debug, clap::Subcommand, PartialEq, Eq)]
pub enum DcoCommand {
    #[command(about = "Enable DCO Signed-off-by injection", long_about = "Enable DCO Signed-off-by injection", before_help = BANNER, styles = HELP_STYLES)]
    Enable,
    #[command(about = "Disable DCO Signed-off-by injection", long_about = "Disable DCO Signed-off-by injection", before_help = BANNER, styles = HELP_STYLES)]
    Disable,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum CoauthorTrailerCommand {
    #[command(about = "Enable automatic Co-authored-by trailer injection inside agent containers", long_about = "Enable automatic Co-authored-by trailer injection inside agent containers

When enabled, jackin installs a prepare-commit-msg hook inside every launched container via core.hooksPath. The hook appends the correct Co-authored-by trailer whenever Git prepares a commit message.", before_help = BANNER, styles = HELP_STYLES)]
    Enable,
    #[command(about = "Disable automatic Co-authored-by trailer injection (default)", long_about = "Disable automatic Co-authored-by trailer injection (default)", before_help = BANNER, styles = HELP_STYLES)]
    Disable,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum EnvCommand {
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin config env set API_TOKEN \"op://Personal/api/token\"
  jackin config env set LOG_LEVEL debug --role agent-smith
  jackin config env set OPENAI_KEY \"op://Work/OpenAI/key\" --comment \"rotate quarterly\"
  jackin config env set OP_TOKEN \"op://Work/svc/credential\" --on-demand"
    )]
    #[command(
        about = "Set an env var at global or per-role scope",
        long_about = "Set an env var at global or per-role scope

Without `--role`, writes the env var globally. With `--role <SELECTOR>`, scopes it to that role only. The role selector is not pre-validated — the value is recorded regardless of whether that role is registered, even if the role is not registered."
    )]
    Set {
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
            help = "Apply to a specific role instead of globally",
            long_help = "Apply to a specific role instead of globally",
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
  jackin config env unset API_TOKEN
  jackin config env unset LOG_LEVEL --role agent-smith"
    )]
    #[command(
        about = "Unset an env var at global or per-role scope",
        long_about = "Unset an env var at global or per-role scope

Idempotent: if the key is not present, prints \"KEY not set.\" and exits 0 without saving the config."
    )]
    Unset {
        #[arg(help = "Env var name to remove", long_help = "Env var name to remove")]
        key: String,
        #[arg(
            help = "Unset from a specific role instead of globally",
            long_help = "Unset from a specific role instead of globally",
            long
        )]
        role: Option<String>,
    },
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin config env list
  jackin config env list --role agent-smith"
    )]
    #[command(
        about = "List env vars at global or per-role scope",
        long_about = "List env vars at global or per-role scope"
    )]
    List {
        #[arg(
            help = "List vars for a specific role instead of the global scope",
            long_help = "List vars for a specific role instead of the global scope",
            long
        )]
        role: Option<String>,
    },
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum MountCommand {
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin config mount add gradle-cache --src ~/.gradle/caches --dst /home/agent/.gradle/caches --readonly
  jackin config mount add secrets --src ~/.chainargos/secrets --dst /secrets --readonly --scope \"chainargos/*\""
    )]
    #[command(
        about = "Register a new global mount applied to matching roles",
        long_about = "Register a new global mount applied to matching roles"
    )]
    Add {
        #[arg(
            help = "Unique name for this mount (used to identify it later)",
            long_help = "Unique name for this mount (used to identify it later)"
        )]
        name: String,
        #[arg(
            help = "Path on the host machine to mount from",
            long_help = "Path on the host machine to mount from",
            long
        )]
        src: String,
        #[arg(
            help = "Path inside the container to mount to",
            long_help = "Path inside the container to mount to",
            long
        )]
        dst: String,
        #[arg(
            help = "Make this mount read-only inside the container",
            long_help = "Make this mount read-only inside the container",
            long,
            default_value_t = false
        )]
        readonly: bool,
        #[arg(
            help = "Apply only to matching roles (e.g. `chainargos/*` or `chainargos/agent-brown`)",
            long_help = "Apply only to matching roles (e.g. `chainargos/*` or `chainargos/agent-brown`)",
            long
        )]
        scope: Option<String>,
    },
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin config mount remove gradle-cache
  jackin config mount remove secrets --scope \"chainargos/*\""
    )]
    #[command(
        about = "Unregister a global mount by name",
        long_about = "Unregister a global mount by name"
    )]
    Remove {
        #[arg(
            help = "Name of the mount to remove",
            long_help = "Name of the mount to remove"
        )]
        name: String,
        #[arg(
            help = "Only remove from this scope (leave other scopes untouched)",
            long_help = "Only remove from this scope (leave other scopes untouched)",
            long
        )]
        scope: Option<String>,
    },
    #[command(about = "List all registered global mounts", long_about = "List all registered global mounts", before_help = BANNER, styles = HELP_STYLES)]
    List,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum TrustCommand {
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin config trust grant chainargos/the-architect"
    )]
    #[command(
        about = "Mark a third-party role source as trusted",
        long_about = "Mark a third-party role source as trusted

Trust controls whether jackin will build and run a role without prompting. Untrusted roles require interactive confirmation on every load."
    )]
    Grant {
        #[arg(
            help = "Role class selector (e.g. `chainargos/agent-brown`)",
            long_help = "Role class selector (e.g. `chainargos/agent-brown`)"
        )]
        selector: String,
    },

    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin config trust revoke chainargos/the-architect"
    )]
    #[command(
        about = "Revoke trust for a third-party role source",
        long_about = "Revoke trust for a third-party role source

The next `jackin load` will prompt for confirmation again."
    )]
    Revoke {
        #[arg(
            help = "Role class selector (e.g. `chainargos/agent-brown`)",
            long_help = "Role class selector (e.g. `chainargos/agent-brown`)"
        )]
        selector: String,
    },

    #[command(about = "List all currently trusted role sources", long_about = "List all currently trusted role sources", before_help = BANNER, styles = HELP_STYLES)]
    List,
}

#[cfg(test)]
mod tests;
