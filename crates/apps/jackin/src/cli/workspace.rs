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
pub struct WorkspaceFormatArgs {
    #[arg(long, value_name = "FORMAT", default_value = "human")]
    pub format: String,
}

pub type WorkspaceListArgs = WorkspaceFormatArgs;

#[derive(Debug, Args, PartialEq, Eq)]
pub struct WorkspaceShowArgs {
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
    Create {
        name: String,

        #[arg(long)]
        workdir: String,

        #[arg(long = "mount", required = true)]
        mounts: Vec<String>,

        #[arg(long = "allowed-role")]
        allowed_roles: Vec<String>,

        #[arg(long = "default-role")]
        default_role: Option<String>,

        #[arg(long, value_parser = parse_agent)]
        default_agent: Option<jackin_core::Agent>,

        #[arg(
            long = "mount-isolation",
            value_name = "DST=TYPE",
            value_parser = parse_mount_isolation,
            action = clap::ArgAction::Append
        )]
        mount_isolation: Vec<(String, MountIsolation)>,

        #[arg(long = "keep-awake", default_value_t = false)]
        keep_awake: bool,

        #[arg(long = "git-pull", default_value_t = false)]
        git_pull: bool,
    },

    #[command(before_help = BANNER, styles = HELP_STYLES)]
    List(WorkspaceListArgs),

    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin workspace show my-app"
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
    Edit {
        name: String,

        #[arg(long)]
        workdir: Option<String>,

        #[arg(long = "mount")]
        mounts: Vec<String>,

        #[arg(long = "remove-destination")]
        remove_destinations: Vec<String>,

        #[arg(long, default_value_t = false)]
        no_workdir_mount: bool,

        #[arg(long = "allowed-role")]
        allowed_roles: Vec<String>,

        #[arg(long = "remove-allowed-role")]
        remove_allowed_roles: Vec<String>,

        #[arg(long = "default-role")]
        default_role: Option<String>,

        #[arg(
            long = "clear-default-role",
            conflicts_with = "default_role",
            default_value_t = false
        )]
        clear_default_role: bool,

        #[arg(long, value_parser = parse_agent)]
        default_agent: Option<jackin_core::Agent>,

        #[arg(long, conflicts_with = "default_agent", default_value_t = false)]
        clear_default_agent: bool,

        #[arg(long = "yes", short = 'y', default_value_t = false)]
        assume_yes: bool,

        #[arg(long, default_value_t = false)]
        prune: bool,

        #[arg(
            long = "mount-isolation",
            value_name = "DST=TYPE",
            value_parser = parse_mount_isolation,
            action = clap::ArgAction::Append
        )]
        mount_isolation: Vec<(String, MountIsolation)>,

        #[arg(long)]
        delete_isolated_state: bool,

        #[arg(long = "keep-awake", default_value_t = false)]
        keep_awake: bool,

        #[arg(
            long = "no-keep-awake",
            conflicts_with = "keep_awake",
            default_value_t = false
        )]
        no_keep_awake: bool,

        #[arg(long = "git-pull", default_value_t = false)]
        git_pull: bool,

        #[arg(
            long = "no-git-pull",
            conflicts_with = "git_pull",
            default_value_t = false
        )]
        no_git_pull: bool,

        #[arg(
            long = "default-launch",
            value_name = "CONFIG_ID",
            action = clap::ArgAction::Append
        )]
        default_launch: Vec<String>,

        #[arg(
            long = "clear-default-launch",
            conflicts_with = "default_launch",
            default_value_t = false
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
    Prune {
        name: String,

        #[arg(long = "yes", short = 'y', default_value_t = false)]
        assume_yes: bool,
    },

    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin workspace remove my-app"
    )]
    Remove { name: String },

    #[command(subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Env(WorkspaceEnvCommand),

    #[command(subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
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
    Set {
        workspace: String,

        key: String,

        value: String,

        #[arg(long)]
        role: Option<String>,

        #[arg(long)]
        comment: Option<String>,

        #[arg(long = "on-demand")]
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
    Unset {
        workspace: String,

        key: String,

        #[arg(long)]
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
    List {
        workspace: String,

        #[arg(long)]
        role: Option<String>,
    },
}

#[cfg(test)]
mod tests;
