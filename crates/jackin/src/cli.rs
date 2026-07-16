// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! CLI argument types: `Cli` root struct, `Command` enum, and all subcommand
//! arg structs parsed by `clap`.
//!
//! Parsing only — no business logic. `app/mod.rs` maps these parsed structs to
//! runtime and console calls.

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

// The canonical jackin❯ logo, shared with the host and capsule status bars.
pub(super) const BANNER: &str = jackin_tui::ansi::BRAND_BANNER;

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
pub mod usage;
pub mod workspace;

pub use config::{
    AuthCommand, CoauthorTrailerCommand, ConfigCommand, DcoCommand, EnvCommand, GitCommand,
    MountCommand, TrustCommand,
};
#[cfg(unix)]
pub use daemon::DaemonCommand;
pub use diagnostics::DiagnosticsCommand;
pub use prewarm::PrewarmArgs;
pub use prune::PruneCommand;
pub use workspace::{
    WorkspaceClaudeTokenCommand, WorkspaceCommand, WorkspaceEnvCommand, WorkspaceFormatArgs,
    WorkspaceShowArgs,
};

/// Operator's CLI for orchestrating AI coding roles in isolated containers
///
/// Running `jackin` with no subcommand opens the operator console when
/// stdout is attached to a reasonably-sized interactive terminal, and
/// otherwise prints this help page (exit 0, silent).
#[derive(Debug, Parser)]
// Root help shows the one-line `BANNER` pill inherited from the flattened
// `ConsoleArgs`. It needs no explicit `before_help`; the binary layers the
// frozen-rain field above that pill on a wide interactive terminal (clap
// reflows multi-line ANSI art, so the rain is printed directly, not here).
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
    /// Top-level console args — carried through to the console runner when
    /// no subcommand is given (i.e. bare `jackin`).
    #[command(flatten)]
    pub console_args: ConsoleArgs,
    /// Print raw container output for troubleshooting.
    ///
    /// Global flag: accepted before or after any subcommand.
    /// Also enabled by setting `JACKIN_DEBUG=1` in the environment.
    //
    // `SetTrue` + `FalseyValueParser` lets `JACKIN_DEBUG=1` parse as
    // true while the absence of `--debug` defaults to false.
    #[arg(
        long,
        global = true,
        env = "JACKIN_DEBUG",
        action = clap::ArgAction::SetTrue,
        value_parser = clap::builder::FalseyValueParser::new(),
    )]
    pub debug: bool,
}

/// Top-level `jackin` subcommand dispatch.
///
/// Variants that wrap an `#[derive(Args)]` struct carry their help text on
/// the struct itself — see e.g. `cli::role::LoadArgs`. Variants that wrap
/// a `#[derive(Subcommand)]` enum (`Workspace`, `Config`) keep their
/// parent-command help on the outer variant: Clap's subcommand-enum
/// attribute propagation targets nested variants, not the parent help
/// page. `Exile` is a unit variant with no payload, so its attributes
/// also live here.
///
/// All variants use tuple form (`Load(LoadArgs)`, `Workspace(WorkspaceCommand)`),
/// never struct form with inline `{ ... }`. This keeps dispatch symmetry.
#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum Command {
    Load(LoadArgs),
    Hardline(HardlineArgs),
    Eject(EjectArgs),
    /// Pull every running role out at once
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Exile,
    Purge(PurgeArgs),
    /// Prewarm jackin-owned runtime caches before launch
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Prewarm(PrewarmArgs),
    /// Delete cached or stale jackin data
    #[command(subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Prune(PruneCommand),
    /// Open the operator console to manage workspaces, launch roles, and more
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Console(ConsoleArgs),
    /// Validate, migrate, and scaffold role repositories
    #[command(subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Role(RoleCommand),
    /// Manage saved workspaces
    #[command(subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Workspace(WorkspaceCommand),
    /// View and modify operator configuration
    #[command(subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Config(ConfigCommand),
    /// Manage the per-user jackin❯ host daemon
    #[cfg(unix)]
    #[command(subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Daemon(DaemonCommand),
    /// Run pre-flight health checks for your jackin❯ setup
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Doctor(doctor::DoctorArgs),
    /// Validate direct OTLP telemetry delivery
    #[command(subcommand, before_help = BANNER, styles = HELP_STYLES, disable_help_subcommand = true)]
    Diagnostics(DiagnosticsCommand),
    /// Show fleet status — workspaces, instances, and agents
    #[command(before_help = BANNER, styles = HELP_STYLES, visible_alias = "ps")]
    Status(status::StatusArgs),
    /// Read cached usage and quota data from a running Capsule daemon
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Usage(usage::UsageArgs),
    /// Print help documentation for a jackin command
    ///
    /// With no arguments, displays the jackin manual.
    /// With a command name, displays the manual for that command:
    ///
    ///   jackin help config
    ///   jackin help config auth
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Help {
        /// Command path to get help for (e.g. `config auth`)
        #[arg(trailing_var_arg = true, num_args = 0..)]
        command: Vec<String>,
    },
}

#[cfg(test)]
mod tests;
