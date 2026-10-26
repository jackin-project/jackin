// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use clap::builder::styling::{AnsiColor, Effects, Styles};
use clap::{Parser, Subcommand};

use cleanup::{EjectArgs, PurgeArgs};
use role::{ConsoleArgs, HardlineArgs, LoadArgs, RoleCommand};
use workflow::WorkflowCommand;

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
pub mod workflow;
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
#[command(
    about = "Operator's CLI for orchestrating AI coding roles in isolated containers",
    long_about = "Operator's CLI for orchestrating AI coding roles in isolated containers

Running `jackin` with no subcommand opens the operator console when stdout is attached to a reasonably-sized interactive terminal, and otherwise prints this help page (exit 0, silent)."
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
    #[arg(
        help = "Print raw container output for troubleshooting.",
        long_help = "Print raw container output for troubleshooting.

Global flag: accepted before or after any subcommand. Also enabled by setting `JACKIN_DEBUG=1` in the environment."
    )]
    pub debug: bool,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum Command {
    Load(LoadArgs),
    Hardline(HardlineArgs),
    Eject(EjectArgs),
    #[command(about = "Pull every running role out at once", long_about = "Pull every running role out at once", before_help = BANNER, styles = HELP_STYLES)]
    Exile,
    Purge(PurgeArgs),
    #[command(about = "Prewarm jackin-owned runtime caches before launch", long_about = "Prewarm jackin-owned runtime caches before launch", before_help = BANNER, styles = HELP_STYLES)]
    Prewarm(PrewarmArgs),
    #[command(about = "Delete cached or stale jackin data", long_about = "Delete cached or stale jackin data", subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Prune(PruneCommand),
    #[command(about = "Open the operator console to manage workspaces, launch roles, and more", long_about = "Open the operator console to manage workspaces, launch roles, and more", before_help = BANNER, styles = HELP_STYLES)]
    Console(ConsoleArgs),
    #[command(about = "Validate, migrate, and scaffold role repositories", long_about = "Validate, migrate, and scaffold role repositories", subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Role(RoleCommand),
    #[command(about = "Manage saved workspaces", long_about = "Manage saved workspaces", subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Workspace(WorkspaceCommand),
    #[command(about = "Manage coding-agent and AI-provider accounts", long_about = "Manage coding-agent and AI-provider accounts", subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Account(account::AccountCommand),
    #[command(about = "View and modify operator configuration", long_about = "View and modify operator configuration", subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Config(ConfigCommand),
    #[cfg(unix)]
    #[command(about = "Manage the per-user jackin❯ host daemon", long_about = "Manage the per-user jackin❯ host daemon", subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Daemon(DaemonCommand),
    #[command(about = "Run pre-flight health checks for your jackin❯ setup", long_about = "Run pre-flight health checks for your jackin❯ setup", before_help = BANNER, styles = HELP_STYLES)]
    Doctor(doctor::DoctorArgs),
    #[command(about = "Validate direct OTLP telemetry delivery", long_about = "Validate direct OTLP telemetry delivery", subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Diagnostics(DiagnosticsCommand),
    #[command(about = "Show fleet status — workspaces, instances, and agents", long_about = "Show fleet status — workspaces, instances, and agents", before_help = BANNER, styles = HELP_STYLES, visible_alias = "ps")]
    Status(status::StatusArgs),
    #[command(about = "Validate and plan repair workflows", long_about = "Validate and plan repair workflows", subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Workflow(WorkflowCommand),
    #[command(about = "Read broker-owned cached usage and manage durable usage monitors", long_about = "Read broker-owned cached usage and manage durable usage monitors", before_help = BANNER, styles = HELP_STYLES)]
    Usage(usage::UsageArgs),
    #[command(about = "Print help documentation for a jackin command", long_about = "Print help documentation for a jackin command

With no arguments, displays the jackin manual. With a command name, displays the manual for that command:

jackin help config jackin help account", before_help = BANNER, styles = HELP_STYLES)]
    Help {
        #[arg(help = "Command path to get help for (e.g. `account`)", long_help = "Command path to get help for (e.g. `account`)", trailing_var_arg = true, num_args = 0..)]
        command: Vec<String>,
    },
}
