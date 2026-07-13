// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! CLI argument structs for `jackin eject` and `jackin purge` subcommands.
//!
//! Not responsible for: executing eject or purge logic — structs are parsed
//! by `clap` and dispatched to handlers in `src/commands/`.

use clap::Args;

use super::{BANNER, HELP_STYLES};

/// Stop a role and clean up its container
#[derive(Debug, Args, PartialEq, Eq)]
#[command(
    before_help = BANNER,
    styles = HELP_STYLES,
    after_long_help = "\
Examples:
  jackin eject agent-smith
  jackin eject agent-smith --all
  jackin eject agent-smith --purge
  jackin eject k7p9m2xq
  jackin eject jk-k7p9m2xq-agentsmith"
)]
pub struct EjectArgs {
    /// Role class selector, instance ID, or container name to stop
    pub selector: String,
    /// Stop every matching instance (otherwise errors if multiple exist)
    #[arg(long)]
    pub all: bool,
    /// Also delete persisted state after stopping
    #[arg(long)]
    pub purge: bool,
}

/// Delete persisted state for a role class
#[derive(Debug, Args, PartialEq, Eq)]
#[command(
    before_help = BANNER,
    styles = HELP_STYLES,
    after_long_help = "\
Examples:
  jackin purge agent-smith
  jackin purge agent-smith --all
  jackin purge k7p9m2xq
  jackin purge chainargos/the-architect"
)]
pub struct PurgeArgs {
    /// Role class selector, instance ID, or container name
    pub selector: String,
    /// Delete state for every matching instance (otherwise errors if multiple exist)
    #[arg(long)]
    pub all: bool,
}

#[cfg(test)]
mod tests;
