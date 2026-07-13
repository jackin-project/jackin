// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! CLI argument structs for `jackin config` subcommands (auth, mounts, git, env, trust).
//!
//! Not responsible for: reading or writing config state — structs are parsed
//! by `clap` and dispatched to handlers in `src/commands/config/`.

use clap::Subcommand;

use super::{BANNER, HELP_STYLES};

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum ConfigCommand {
    /// Manage global mount configurations
    #[command(subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Mount(MountCommand),
    /// Manage trust for third-party role sources
    #[command(subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Trust(TrustCommand),
    /// Manage Claude Code authentication forwarding from host
    #[command(subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Auth(AuthCommand),
    /// Manage operator env vars at global and per-role scope
    #[command(subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Env(EnvCommand),
    /// Manage global git settings
    #[command(subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Git(GitCommand),
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum GitCommand {
    /// Configure automatic Co-authored-by trailer injection for agent commits
    #[command(subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    CoauthorTrailer(CoauthorTrailerCommand),
    /// Configure DCO Signed-off-by trailer injection for agent commits
    #[command(subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Dco(DcoCommand),
}

#[derive(Debug, clap::Subcommand, PartialEq, Eq)]
pub enum DcoCommand {
    /// Enable DCO Signed-off-by injection
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Enable,
    /// Disable DCO Signed-off-by injection
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Disable,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum CoauthorTrailerCommand {
    /// Enable automatic Co-authored-by trailer injection inside agent containers
    ///
    /// When enabled, jackin installs a prepare-commit-msg hook inside every
    /// launched container via core.hooksPath. The hook appends the correct
    /// Co-authored-by trailer whenever Git prepares a commit message.
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Enable,
    /// Disable automatic Co-authored-by trailer injection (default)
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Disable,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum EnvCommand {
    /// Set an env var at global or per-role scope
    ///
    /// Without `--role`, writes the env var globally. With
    /// `--role <SELECTOR>`, scopes it to that role only. The role
    /// selector is not pre-validated — the value is recorded regardless
    /// of whether that role is registered, matching `config auth set`.
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin config env set API_TOKEN \"op://Personal/api/token\"
  jackin config env set LOG_LEVEL debug --role agent-smith
  jackin config env set OPENAI_KEY \"op://Work/OpenAI/key\" --comment \"rotate quarterly\""
    )]
    Set {
        /// Env var name (stored verbatim; no POSIX validation)
        key: String,
        /// Env var value (use `op://...`, `$VAR`, `${VAR}`, or literal)
        value: String,
        /// Apply to a specific role instead of globally
        #[arg(long)]
        role: Option<String>,
        /// Attach a comment to the key (recorded alongside the value)
        #[arg(long)]
        comment: Option<String>,
    },
    /// Unset an env var at global or per-role scope
    ///
    /// Idempotent: if the key is not present, prints "KEY not set." and
    /// exits 0 without saving the config.
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin config env unset API_TOKEN
  jackin config env unset LOG_LEVEL --role agent-smith"
    )]
    Unset {
        /// Env var name to remove
        key: String,
        /// Unset from a specific role instead of globally
        #[arg(long)]
        role: Option<String>,
    },
    /// List env vars at global or per-role scope
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin config env list
  jackin config env list --role agent-smith"
    )]
    List {
        /// List vars for a specific role instead of the global scope
        #[arg(long)]
        role: Option<String>,
    },
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum AuthCommand {
    /// Set the global authentication forwarding mode for an agent
    ///
    /// Controls how the host's agent authentication is made available to
    /// role containers at the global layer. Defaults to `claude` when
    /// `--agent` is omitted. GitHub CLI auth is configured through the
    /// operator console's Auth tab today, not this CLI verb.
    ///
    /// Modes: sync (default — overwrite container auth from host on each
    /// launch when host auth exists; preserve container auth when host auth
    /// is absent), ignore (revoke and never forward), `oauth_token` (Claude
    /// only — long-lived `CLAUDE_CODE_OAUTH_TOKEN` resolved from the operator
    /// env), `api_key` (short-lived `ANTHROPIC_API_KEY` / `OPENAI_API_KEY` /
    /// `AMP_API_KEY` from the operator env). Tokens and keys are never
    /// written to disk. Modes unsupported by the chosen agent are rejected
    /// — see `jackin` docs on auth forwarding for setup.
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin config auth set sync
  jackin config auth set ignore
  jackin config auth set oauth_token
  jackin config auth set api_key
  jackin config auth set api_key --agent codex
  jackin config auth set sync --agent amp"
    )]
    Set {
        /// Authentication forwarding mode: sync, ignore, `api_key`, or `oauth_token`
        mode: String,
        /// Agent to configure: `claude` (default), `codex`, or `amp`.
        #[arg(long, default_value = "claude")]
        agent: String,
    },
    /// Show the current authentication forwarding mode
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin config auth show"
    )]
    Show,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum MountCommand {
    /// Register a new global mount applied to matching roles
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin config mount add gradle-cache --src ~/.gradle/caches --dst /home/agent/.gradle/caches --readonly
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
        /// Apply only to matching roles (e.g. `chainargos/*` or `chainargos/agent-brown`)
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
    /// Mark a third-party role source as trusted
    ///
    /// Trust controls whether jackin will build and run a role without
    /// prompting. Untrusted roles require interactive confirmation on
    /// every load.
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin config trust grant chainargos/the-architect"
    )]
    Grant {
        /// Role class selector (e.g. `chainargos/agent-brown`)
        selector: String,
    },
    /// Revoke trust for a third-party role source
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
        /// Role class selector (e.g. `chainargos/agent-brown`)
        selector: String,
    },
    /// List all currently trusted role sources
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    List,
}

#[cfg(test)]
mod tests;
