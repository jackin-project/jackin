// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use clap::builder::styling::{AnsiColor, Effects, Styles};
use clap::{Parser, Subcommand};

use cleanup::{EjectArgs, PurgeArgs};
use role::{ConsoleArgs, HardlineArgs, LoadArgs, RoleCommand};

pub(super) const HELP_STYLES: Styles = Styles::styled()
    .header(AnsiColor::BrightGreen.on_default().effects(Effects::BOLD))
    .usage(AnsiColor::BrightGreen.on_default().effects(Effects::BOLD))
    .literal(AnsiColor::Green.on_default().effects(Effects::BOLD))
    .placeholder(AnsiColor::White.on_default())
    .valid(AnsiColor::BrightGreen.on_default())
    .invalid(AnsiColor::Red.on_default().effects(Effects::BOLD))
    .error(AnsiColor::Red.on_default().effects(Effects::BOLD));

pub(super) const BANNER: &str = crate::brand_output::BRAND_BANNER;

pub mod account;
pub mod cleanup;
pub mod config;
#[cfg(unix)]
pub mod daemon;
pub mod diagnostics;
pub mod dispatch;
pub mod doctor;
pub mod format;
pub mod help;
pub mod prewarm;
pub mod prune;
pub mod role;
pub mod status;
pub mod telemetry;
pub mod usage;
pub mod workspace;

pub use config::{
    CoauthorTrailerCommand, ConfigCommand, DcoCommand, EnvCommand, GitCommand, MountCommand,
    TrustCommand,
};
#[cfg(unix)]
pub use daemon::DaemonCommand;
pub use diagnostics::DiagnosticsCommand;
pub use prewarm::PrewarmArgs;
pub use prune::PruneCommand;
pub use telemetry::command_name;
pub use workspace::{
    WorkspaceCommand, WorkspaceEnvCommand, WorkspaceFormatArgs, WorkspaceShowArgs,
};

#[derive(Debug, Parser)]
#[command(
    name = "jackin",
    version = env!("JACKIN_VERSION"),
    styles = HELP_STYLES,
    disable_help_subcommand = true,
    after_help = "Run 'jackin help <command>' for more detailed information."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
    #[command(flatten)]
    pub console_args: ConsoleArgs,
    #[arg(
        long,
        global = true,
        env = "JACKIN_DEBUG",
        action = clap::ArgAction::SetTrue,
        value_parser = clap::builder::FalseyValueParser::new(),
    )]
    pub debug: bool,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum Command {
    Load(LoadArgs),
    Hardline(HardlineArgs),
    Eject(EjectArgs),
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Exile,
    Purge(PurgeArgs),
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Prewarm(PrewarmArgs),
    #[command(subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Prune(PruneCommand),
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Console(ConsoleArgs),
    #[command(subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Role(RoleCommand),
    #[command(subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Workspace(WorkspaceCommand),
    #[command(subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Account(account::AccountCommand),
    #[command(subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Config(ConfigCommand),
    #[cfg(unix)]
    #[command(subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Daemon(DaemonCommand),
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Doctor(doctor::DoctorArgs),
    #[command(subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Diagnostics(DiagnosticsCommand),
    #[command(before_help = BANNER, styles = HELP_STYLES, visible_alias = "ps")]
    Status(status::StatusArgs),
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Usage(usage::UsageArgs),
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Help {
        #[arg(trailing_var_arg = true, num_args = 0..)]
        command: Vec<String>,
    },
}

#[cfg(test)]
mod tests;
