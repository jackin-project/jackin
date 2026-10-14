// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use clap::Subcommand;

use super::{BANNER, HELP_STYLES};

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum ConfigCommand {
    #[command(subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Mount(MountCommand),

    #[command(subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Trust(TrustCommand),

    #[command(subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Env(EnvCommand),

    #[command(subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Git(GitCommand),
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum GitCommand {
    #[command(subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    CoauthorTrailer(CoauthorTrailerCommand),

    #[command(subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Dco(DcoCommand),
}

#[derive(Debug, clap::Subcommand, PartialEq, Eq)]
pub enum DcoCommand {
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Enable,

    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Disable,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum CoauthorTrailerCommand {
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Enable,

    #[command(before_help = BANNER, styles = HELP_STYLES)]
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
    Set {
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
  jackin config env unset API_TOKEN
  jackin config env unset LOG_LEVEL --role agent-smith"
    )]
    Unset {
        key: String,

        #[arg(long)]
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
    List {
        #[arg(long)]
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
    Add {
        name: String,

        #[arg(long)]
        src: String,

        #[arg(long)]
        dst: String,

        #[arg(long, default_value_t = false)]
        readonly: bool,

        #[arg(long)]
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
    Remove {
        name: String,

        #[arg(long)]
        scope: Option<String>,
    },

    #[command(before_help = BANNER, styles = HELP_STYLES)]
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
    Grant { selector: String },

    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin config trust revoke chainargos/the-architect"
    )]
    Revoke { selector: String },

    #[command(before_help = BANNER, styles = HELP_STYLES)]
    List,
}

#[cfg(test)]
mod tests;
