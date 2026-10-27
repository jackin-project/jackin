// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use clap::Args;

use super::{BANNER, HELP_STYLES};

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
#[command(
    about = "Stop a role and clean up its container",
    long_about = "Stop a role and clean up its container"
)]
pub struct EjectArgs {
    #[arg(
        help = "Role class selector, instance ID, or container name to stop",
        long_help = "Role class selector, instance ID, or container name to stop"
    )]
    pub selector: String,
    #[arg(
        help = "Stop every matching instance (otherwise errors if multiple exist)",
        long_help = "Stop every matching instance (otherwise errors if multiple exist)",
        long
    )]
    pub all: bool,
    #[arg(
        help = "Also delete persisted state after stopping",
        long_help = "Also delete persisted state after stopping",
        long
    )]
    pub purge: bool,
}

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
#[command(
    about = "Delete persisted state for a role class",
    long_about = "Delete persisted state for a role class"
)]
pub struct PurgeArgs {
    #[arg(
        help = "Role class selector, instance ID, or container name",
        long_help = "Role class selector, instance ID, or container name"
    )]
    pub selector: String,
    #[arg(
        help = "Delete state for every matching instance (otherwise errors if multiple exist)",
        long_help = "Delete state for every matching instance (otherwise errors if multiple exist)",
        long
    )]
    pub all: bool,
}
